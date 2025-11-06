//! Validation rules for VSA structure

use super::{EnhancedValidationReport, Severity, Suggestion, ValidationContext, ValidationIssue};
use crate::error::Result;
use crate::patterns::PatternMatcher;
use crate::scanner::Scanner;

/// A validation rule that can be applied to a VSA project
pub trait ValidationRule {
    /// Get the rule name
    fn name(&self) -> &str;

    /// Get the rule code (e.g., "VSA001")
    fn code(&self) -> &str;

    /// Validate and add issues to the report
    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()>;
}

/// Collection of validation rules
pub struct ValidationRuleSet {
    rules: Vec<Box<dyn ValidationRule>>,
}

impl ValidationRuleSet {
    /// Create a new empty rule set
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// Create a new rule set with default rules
    pub fn default_rules() -> Self {
        use super::{
            ContextBoundariesRule, IntegrationEventNamingRule, IntegrationEventsLocationRule,
            NoCircularDependenciesRule, NoDuplicateIntegrationEventsRule, RequireSharedFolderRule,
        };

        let rules: Vec<Box<dyn ValidationRule>> = vec![
            // Basic structure rules
            Box::new(RequireTestsRule),
            Box::new(RequireHandlerForCommandRule),
            Box::new(RequireEventForCommandRule),
            Box::new(NamingConventionRule),
            Box::new(MaxNestingDepthRule),
            Box::new(SharedFolderRule),
            // Integration event rules
            Box::new(NoDuplicateIntegrationEventsRule),
            Box::new(IntegrationEventsLocationRule),
            Box::new(IntegrationEventNamingRule),
            // Bounded context rules
            Box::new(NoCircularDependenciesRule),
            Box::new(ContextBoundariesRule),
            Box::new(RequireSharedFolderRule),
        ];

        Self { rules }
    }

    /// Add a custom rule
    pub fn add_rule(&mut self, rule: Box<dyn ValidationRule>) {
        self.rules.push(rule);
    }

    /// Validate all rules
    pub fn validate_all(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        for rule in &self.rules {
            rule.validate(ctx, report)?;
        }
        Ok(())
    }
}

impl Default for ValidationRuleSet {
    fn default() -> Self {
        Self::default_rules()
    }
}

/// Rule: Features with commands should have tests
struct RequireTestsRule;

impl ValidationRule for RequireTestsRule {
    fn name(&self) -> &str {
        "require-tests"
    }

    fn code(&self) -> &str {
        "VSA001"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        if !ctx.config.validation.require_tests {
            return Ok(());
        }

        let scanner = Scanner::new(ctx.config.clone(), ctx.root.clone());
        let pattern_matcher = PatternMatcher::new(
            ctx.config.patterns.clone(),
            ctx.config.file_extension().to_string(),
        );

        let contexts = scanner.scan_contexts()?;

        for context in contexts {
            let features = scanner.scan_features(&context.path)?;

            for feature in features {
                let files = scanner.scan_feature_files(&feature.path)?;

                let has_command = files.iter().any(|f| pattern_matcher.is_command(&f.path));
                let has_handler = files.iter().any(|f| pattern_matcher.is_handler(&f.path));
                let has_test = files.iter().any(|f| pattern_matcher.is_test(&f.path));

                if (has_command || has_handler) && !has_test {
                    let test_file_name =
                        format!("{}.test.{}", feature.name, ctx.config.file_extension());
                    let test_path = feature.path.join(&test_file_name);

                    report.warnings.push(ValidationIssue {
                        path: feature.path.clone(),
                        code: self.code().to_string(),
                        severity: Severity::Warning,
                        message: format!(
                            "Feature '{}' in context '{}' is missing tests",
                            feature.name, context.name
                        ),
                        suggestions: vec![Suggestion::create_file(
                            test_path,
                            format!("Create {test_file_name} with unit tests"),
                        )],
                    });
                }
            }
        }

        Ok(())
    }
}

/// Rule: Commands should have handlers
struct RequireHandlerForCommandRule;

impl ValidationRule for RequireHandlerForCommandRule {
    fn name(&self) -> &str {
        "require-handler-for-command"
    }

    fn code(&self) -> &str {
        "VSA002"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        let scanner = Scanner::new(ctx.config.clone(), ctx.root.clone());
        let pattern_matcher = PatternMatcher::new(
            ctx.config.patterns.clone(),
            ctx.config.file_extension().to_string(),
        );

        let contexts = scanner.scan_contexts()?;

        for context in contexts {
            let features = scanner.scan_features(&context.path)?;

            for feature in features {
                let files = scanner.scan_feature_files(&feature.path)?;

                let commands: Vec<_> =
                    files.iter().filter(|f| pattern_matcher.is_command(&f.path)).collect();
                let has_handler = files.iter().any(|f| pattern_matcher.is_handler(&f.path));

                if !commands.is_empty() && !has_handler {
                    // Try to derive handler name from command name
                    let handler_suggestions = commands
                        .iter()
                        .map(|cmd| {
                            let cmd_name = cmd.path.file_stem().unwrap().to_string_lossy();
                            let handler_name = cmd_name.replace("Command", "Handler");
                            let handler_file =
                                format!("{handler_name}.{}", ctx.config.file_extension());
                            let handler_path = feature.path.join(&handler_file);

                            Suggestion::create_file(
                                handler_path,
                                format!("Create {handler_file} to handle the command"),
                            )
                        })
                        .collect();

                    report.errors.push(ValidationIssue {
                        path: feature.path.clone(),
                        code: self.code().to_string(),
                        severity: Severity::Error,
                        message: format!(
                            "Feature '{}' in context '{}' has command(s) but no handler",
                            feature.name, context.name
                        ),
                        suggestions: handler_suggestions,
                    });
                }
            }
        }

        Ok(())
    }
}

/// Rule: Commands should produce events
struct RequireEventForCommandRule;

impl ValidationRule for RequireEventForCommandRule {
    fn name(&self) -> &str {
        "require-event-for-command"
    }

    fn code(&self) -> &str {
        "VSA003"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        let scanner = Scanner::new(ctx.config.clone(), ctx.root.clone());
        let pattern_matcher = PatternMatcher::new(
            ctx.config.patterns.clone(),
            ctx.config.file_extension().to_string(),
        );

        let contexts = scanner.scan_contexts()?;

        for context in contexts {
            let features = scanner.scan_features(&context.path)?;

            for feature in features {
                let files = scanner.scan_feature_files(&feature.path)?;

                let has_command = files.iter().any(|f| pattern_matcher.is_command(&f.path));
                let has_event = files.iter().any(|f| pattern_matcher.is_event(&f.path));

                if has_command && !has_event {
                    report.warnings.push(ValidationIssue {
                        path: feature.path.clone(),
                        code: self.code().to_string(),
                        severity: Severity::Warning,
                        message: format!(
                            "Feature '{}' in context '{}' has command but no event (consider event sourcing)",
                            feature.name, context.name
                        ),
                        suggestions: vec![Suggestion::manual(
                            "Create an event that represents the outcome of this command"
                        )],
                    });
                }
            }
        }

        Ok(())
    }
}

/// Rule: Files should follow naming conventions
struct NamingConventionRule;

impl ValidationRule for NamingConventionRule {
    fn name(&self) -> &str {
        "naming-convention"
    }

    fn code(&self) -> &str {
        "VSA004"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        let scanner = Scanner::new(ctx.config.clone(), ctx.root.clone());
        let contexts = scanner.scan_contexts()?;

        for context in contexts {
            let features = scanner.scan_features(&context.path)?;

            for feature in features {
                let files = scanner.scan_feature_files(&feature.path)?;

                for file in files {
                    // Check for generic names
                    let file_stem = file.path.file_stem().unwrap().to_string_lossy();
                    if matches!(
                        file_stem.as_ref(),
                        "command" | "event" | "handler" | "query" | "index" | "types"
                    ) {
                        report.warnings.push(ValidationIssue {
                            path: file.path.clone(),
                            code: self.code().to_string(),
                            severity: Severity::Warning,
                            message: format!(
                                "File '{}' uses generic name - prefer specific names like 'CreateProductCommand'",
                                file.name
                            ),
                            suggestions: vec![Suggestion::manual(
                                format!("Rename to a specific name that describes what this {file_stem} does")
                            )],
                        });
                    }
                }
            }
        }

        Ok(())
    }
}

/// Rule: Enforce maximum nesting depth
struct MaxNestingDepthRule;

impl ValidationRule for MaxNestingDepthRule {
    fn name(&self) -> &str {
        "max-nesting-depth"
    }

    fn code(&self) -> &str {
        "VSA005"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        let scanner = Scanner::new(ctx.config.clone(), ctx.root.clone());
        let contexts = scanner.scan_contexts()?;

        let max_depth = ctx.config.validation.max_nesting_depth;

        for context in contexts {
            let features = scanner.scan_features(&context.path)?;

            for feature in features {
                let depth = feature.relative_path.components().count();

                if depth > max_depth {
                    report.warnings.push(ValidationIssue {
                        path: feature.path.clone(),
                        code: self.code().to_string(),
                        severity: Severity::Warning,
                        message: format!(
                            "Feature '{}' exceeds maximum nesting depth ({} > {})",
                            feature.name, depth, max_depth
                        ),
                        suggestions: vec![Suggestion::manual(
                            "Consider flattening the feature structure or adjusting max_nesting_depth in config"
                        )],
                    });
                }
            }
        }

        Ok(())
    }
}

/// Rule: _shared folder structure
struct SharedFolderRule;

impl ValidationRule for SharedFolderRule {
    fn name(&self) -> &str {
        "shared-folder-structure"
    }

    fn code(&self) -> &str {
        "VSA006"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        let scanner = Scanner::new(ctx.config.clone(), ctx.root.clone());
        let contexts = scanner.scan_contexts()?;

        for context in contexts {
            let shared_path = context.path.join("_shared");

            if shared_path.exists() {
                let integration_events_path = shared_path.join("integration-events");

                if !integration_events_path.exists()
                    && ctx.config.validation.require_integration_events_in_shared
                {
                    report.warnings.push(ValidationIssue {
                        path: shared_path.clone(),
                        code: self.code().to_string(),
                        severity: Severity::Warning,
                        message: format!(
                            "Context '{}' has _shared folder but no integration-events directory",
                            context.name
                        ),
                        suggestions: vec![Suggestion::create_file(
                            integration_events_path.join(".gitkeep"),
                            "Create _shared/integration-events/ directory",
                        )],
                    });
                }
            }
        }

        Ok(())
    }
}
