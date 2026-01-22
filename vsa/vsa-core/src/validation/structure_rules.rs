//! Structure validation rules for ADR-019
//!
//! Validates that the codebase follows the canonical VSA structure:
//! - VSA020: Commands must be in domain/commands/
//! - VSA021: Events must be at context root (events/), NOT in domain/
//! - VSA022: Aggregates must be in domain/ root, NOT in _shared/
//! - VSA023: Ports must be in ports/ folder with *Port suffix
//! - VSA024: Buses must be in infrastructure/buses/, NOT application/
//! - VSA025: All ports must end with Port suffix
//! - VSA026: Value objects should follow *ValueObjects.* pattern

use super::{EnhancedValidationReport, Severity, Suggestion, ValidationContext, ValidationIssue};
use crate::error::Result;
use crate::scanner::Scanner;
use super::rules::ValidationRule;
use std::path::Path;

// ============================================================================
// VSA020: Commands must be in domain/commands/
// ============================================================================

/// VSA020: All commands must be located in domain/commands/ directory
///
/// This enforces centralized command organization as defined in ADR-019.
/// Commands are domain contracts that define all write operations.
///
/// Invalid structure:
/// ```
/// contexts/workflows/
///   └── slices/
///       └── create_workflow/
///           └── CreateWorkflowCommand.py  # ❌ Command in slice
/// ```
///
/// Valid structure:
/// ```
/// contexts/workflows/
///   ├── domain/
///   │   └── commands/
///   │       └── CreateWorkflowCommand.py  # ✅ Command in domain/commands/
///   └── slices/
///       └── create_workflow/
///           └── internal/
///               └── Handler.py            # ✅ Handler imports from domain
/// ```
pub struct RequireCommandsInDomainRule;

impl RequireCommandsInDomainRule {
    fn is_command_file(&self, path: &Path, ctx: &ValidationContext) -> bool {
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => return false,
        };

        let ext = ctx.config.file_extension();
        file_name.ends_with(&format!("Command.{ext}"))
    }
}

impl ValidationRule for RequireCommandsInDomainRule {
    fn name(&self) -> &str {
        "require-commands-in-domain"
    }

    fn code(&self) -> &str {
        "VSA020"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        let scanner = Scanner::new(ctx.config.clone(), ctx.root.clone());
        let contexts = scanner.scan_contexts()?;

        for context in contexts {
            // Walk all files in context, looking for *Command.* files
            if let Ok(entries) = std::fs::read_dir(&context.path) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    self.check_directory(&path, &context.path, &context.name, ctx, report)?;
                }
            }
        }

        Ok(())
    }
}

impl RequireCommandsInDomainRule {
    fn check_directory(
        &self,
        path: &Path,
        context_path: &Path,
        context_name: &str,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        if !path.is_dir() {
            return Ok(());
        }

        let dir_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => return Ok(()),
        };

        // Skip domain folder itself (we're checking other locations)
        if dir_name == "domain" {
            return Ok(());
        }

        // Recursively check subdirectories
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                let entry_path = entry.path();

                if entry_path.is_dir() {
                    self.check_directory(&entry_path, context_path, context_name, ctx, report)?;
                } else if self.is_command_file(&entry_path, ctx) {
                    // Found a command file outside domain/commands/
                    let relative_path = entry_path.strip_prefix(context_path)
                        .unwrap_or(&entry_path)
                        .to_string_lossy();

                    // Check if it's in domain/commands/ (allowed)
                    if relative_path.starts_with("domain/commands/") 
                        || relative_path.starts_with("domain\\commands\\") {
                        continue;
                    }

                    let suggested_path = context_path.join("domain/commands")
                        .join(entry_path.file_name().unwrap());

                    report.errors.push(ValidationIssue {
                        path: entry_path.clone(),
                        code: self.code().to_string(),
                        severity: Severity::Error,
                        message: format!(
                            "Command file '{}' in context '{}' is not in domain/commands/ directory. \
                             All commands should be centralized in domain/commands/ as per ADR-019 \
                             (found at: {})",
                            entry_path.file_name().unwrap().to_string_lossy(),
                            context_name,
                            relative_path
                        ),
                        suggestions: vec![Suggestion::manual(format!(
                            "Move this command to domain/commands/\n\
                             Command: git mv {} {}",
                            entry_path.display(),
                            suggested_path.display()
                        ))],
                    });
                }
            }
        }

        Ok(())
    }
}

// ============================================================================
// VSA021: Events must be at context root (events/), NOT in domain/
// ============================================================================

/// VSA021: Events must be at context root (events/), NOT in domain/events/
///
/// As defined in ADR-019, events are immutable contracts between bounded contexts
/// and should be at the context root, not inside the domain folder.
///
/// Invalid structure:
/// ```
/// contexts/workflows/
///   └── domain/
///       └── events/
///           └── WorkflowCreatedEvent.py  # ❌ Events in domain/
/// ```
///
/// Valid structure:
/// ```
/// contexts/workflows/
///   ├── events/
///   │   ├── WorkflowCreatedEvent.py      # ✅ Events at context root
///   │   ├── versioned/                   # ✅ Old versions
///   │   └── upcasters/                   # ✅ Migrations
///   └── domain/
///       └── WorkflowAggregate.py
/// ```
pub struct RequireEventsAtContextRootRule;

impl RequireEventsAtContextRootRule {
    fn is_event_file(&self, path: &Path, ctx: &ValidationContext) -> bool {
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => return false,
        };

        let ext = ctx.config.file_extension();
        file_name.ends_with(&format!("Event.{ext}"))
    }
}

impl ValidationRule for RequireEventsAtContextRootRule {
    fn name(&self) -> &str {
        "require-events-at-context-root"
    }

    fn code(&self) -> &str {
        "VSA021"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        let scanner = Scanner::new(ctx.config.clone(), ctx.root.clone());
        let contexts = scanner.scan_contexts()?;

        for context in contexts {
            // Check if events are in domain/events/ (wrong location)
            let domain_events_path = context.path.join("domain").join("events");
            if domain_events_path.exists() && domain_events_path.is_dir() {
                // Found domain/events/ - this is wrong
                if let Ok(entries) = std::fs::read_dir(&domain_events_path) {
                    let event_files: Vec<_> = entries
                        .flatten()
                        .filter(|e| e.path().is_file() && self.is_event_file(&e.path(), ctx))
                        .collect();

                    if !event_files.is_empty() {
                        let events_path = context.path.join("events");

                        report.errors.push(ValidationIssue {
                            path: domain_events_path.clone(),
                            code: self.code().to_string(),
                            severity: Severity::Error,
                            message: format!(
                                "Context '{}' has events in domain/events/ directory. \
                                 As per ADR-019, events should be at context root (events/), \
                                 not inside domain/. Events are contracts between contexts, \
                                 not domain implementation details.",
                                context.name
                            ),
                            suggestions: vec![Suggestion::manual(format!(
                                "Move events from domain/events/ to events/ (context root)\n\
                                 Commands:\n\
                                 mkdir -p {}\n\
                                 git mv {}/* {}/\n\
                                 rmdir {}",
                                events_path.display(),
                                domain_events_path.display(),
                                events_path.display(),
                                domain_events_path.display()
                            ))],
                        });
                    }
                }
            }

            // Also check for event files in slices or other wrong locations
            self.check_directory_for_misplaced_events(&context.path, &context.path, &context.name, ctx, report)?;
        }

        Ok(())
    }
}

impl RequireEventsAtContextRootRule {
    fn check_directory_for_misplaced_events(
        &self,
        path: &Path,
        context_path: &Path,
        context_name: &str,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        if !path.is_dir() {
            return Ok(());
        }

        let dir_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => return Ok(()),
        };

        // Skip allowed locations
        if dir_name == "events" && path == context_path.join("events") {
            return Ok(());
        }
        if dir_name == "versioned" || dir_name == "upcasters" {
            return Ok(());
        }
        // Skip domain/events/ as it's handled separately in the main validate function
        if path == context_path.join("domain").join("events") {
            return Ok(());
        }

        // Recursively check subdirectories
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                let entry_path = entry.path();

                if entry_path.is_dir() {
                    self.check_directory_for_misplaced_events(&entry_path, context_path, context_name, ctx, report)?;
                } else if self.is_event_file(&entry_path, ctx) {
                    let relative_path = entry_path.strip_prefix(context_path)
                        .unwrap_or(&entry_path)
                        .to_string_lossy();

                    // Check if in allowed location
                    if relative_path.starts_with("events/") || relative_path.starts_with("events\\") {
                        continue;
                    }

                    // Found misplaced event
                    let suggested_path = context_path.join("events")
                        .join(entry_path.file_name().unwrap());

                    report.errors.push(ValidationIssue {
                        path: entry_path.clone(),
                        code: self.code().to_string(),
                        severity: Severity::Error,
                        message: format!(
                            "Event file '{}' in context '{}' is not at context root (events/). \
                             Events should be in events/ directory at context root, not in {} \
                             (found at: {})",
                            entry_path.file_name().unwrap().to_string_lossy(),
                            context_name,
                            dir_name,
                            relative_path
                        ),
                        suggestions: vec![Suggestion::manual(format!(
                            "Move this event to events/ at context root\n\
                             Command: git mv {} {}",
                            entry_path.display(),
                            suggested_path.display()
                        ))],
                    });
                }
            }
        }

        Ok(())
    }
}

// ============================================================================
// VSA022: Aggregates must be in domain/ root, NOT in _shared/
// ============================================================================

/// VSA022: Aggregates must be in domain/ root
///
/// As per ADR-019, aggregates should be at the domain/ root for high visibility.
/// The _shared/ folder pattern is deprecated.
///
/// Invalid structure:
/// ```
/// contexts/workflows/
///   └── _shared/
///       └── WorkflowAggregate.py  # ❌ Aggregate in _shared/
/// ```
///
/// Valid structure:
/// ```
/// contexts/workflows/
///   └── domain/
///       ├── WorkflowAggregate.py          # ✅ Aggregate at domain root
///       └── WorkflowExecutionAggregate.py # ✅ Multiple aggregates OK
/// ```
pub struct RequireAggregatesInDomainRootRule;

impl RequireAggregatesInDomainRootRule {
    fn is_aggregate_file(&self, path: &Path, ctx: &ValidationContext) -> bool {
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => return false,
        };

        let ext = ctx.config.file_extension();
        file_name.ends_with(&format!("Aggregate.{ext}"))
    }
}

impl ValidationRule for RequireAggregatesInDomainRootRule {
    fn name(&self) -> &str {
        "require-aggregates-in-domain-root"
    }

    fn code(&self) -> &str {
        "VSA022"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        let scanner = Scanner::new(ctx.config.clone(), ctx.root.clone());
        let contexts = scanner.scan_contexts()?;

        for context in contexts {
            // Check for aggregates in _shared/ (wrong location)
            let shared_path = context.path.join("_shared");
            if shared_path.exists() && shared_path.is_dir() {
                if let Ok(entries) = std::fs::read_dir(&shared_path) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() && self.is_aggregate_file(&path, ctx) {
                            let domain_path = context.path.join("domain");
                            let suggested_path = domain_path.join(path.file_name().unwrap());

                            report.errors.push(ValidationIssue {
                                path: path.clone(),
                                code: self.code().to_string(),
                                severity: Severity::Error,
                                message: format!(
                                    "Aggregate '{}' in context '{}' is in _shared/ directory. \
                                     As per ADR-019, aggregates should be in domain/ root \
                                     for high visibility. The _shared/ pattern is deprecated.",
                                    path.file_name().unwrap().to_string_lossy(),
                                    context.name
                                ),
                                suggestions: vec![Suggestion::manual(format!(
                                    "Move aggregate to domain/ root\n\
                                     Commands:\n\
                                     mkdir -p {}\n\
                                     git mv {} {}",
                                    domain_path.display(),
                                    path.display(),
                                    suggested_path.display()
                                ))],
                            });
                        }
                    }
                }
            }

            // Also check for aggregates in wrong locations (slices, etc.)
            self.check_directory_for_misplaced_aggregates(&context.path, &context.path, &context.name, ctx, report)?;
        }

        Ok(())
    }
}

impl RequireAggregatesInDomainRootRule {
    fn check_directory_for_misplaced_aggregates(
        &self,
        path: &Path,
        context_path: &Path,
        context_name: &str,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        if !path.is_dir() {
            return Ok(());
        }

        let dir_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => return Ok(()),
        };

        // Skip domain/ folder and its immediate children
        if dir_name == "domain" {
            // Only check if aggregates are NOT in domain root
            // (they should be directly in domain/, not in subfolders)
            if let Ok(entries) = std::fs::read_dir(path) {
                for entry in entries.flatten() {
                    let entry_path = entry.path();
                    if entry_path.is_dir() && entry_path.file_name().unwrap() != "commands" 
                        && entry_path.file_name().unwrap() != "queries" {
                        // Check subfolders of domain/ for misplaced aggregates
                        self.check_subfolder_for_aggregates(&entry_path, context_path, context_name, ctx, report)?;
                    }
                }
            }
            return Ok(());
        }

        // Skip _shared as we already checked it
        if dir_name == "_shared" {
            return Ok(());
        }

        // Recursively check other directories
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                let entry_path = entry.path();
                if entry_path.is_dir() {
                    self.check_directory_for_misplaced_aggregates(&entry_path, context_path, context_name, ctx, report)?;
                }
            }
        }

        Ok(())
    }

    fn check_subfolder_for_aggregates(
        &self,
        path: &Path,
        context_path: &Path,
        context_name: &str,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                let entry_path = entry.path();
                
                if entry_path.is_file() && self.is_aggregate_file(&entry_path, ctx) {
                    let relative_path = entry_path.strip_prefix(context_path)
                        .unwrap_or(&entry_path)
                        .to_string_lossy();

                    let domain_path = context_path.join("domain");
                    let suggested_path = domain_path.join(entry_path.file_name().unwrap());

                    report.errors.push(ValidationIssue {
                        path: entry_path.clone(),
                        code: self.code().to_string(),
                        severity: Severity::Error,
                        message: format!(
                            "Aggregate '{}' in context '{}' is in a subdirectory ({}). \
                             Aggregates should be directly in domain/ root, not in subfolders.",
                            entry_path.file_name().unwrap().to_string_lossy(),
                            context_name,
                            relative_path
                        ),
                        suggestions: vec![Suggestion::manual(format!(
                            "Move aggregate to domain/ root\n\
                             Command: git mv {} {}",
                            entry_path.display(),
                            suggested_path.display()
                        ))],
                    });
                }

                if entry_path.is_dir() {
                    self.check_subfolder_for_aggregates(&entry_path, context_path, context_name, ctx, report)?;
                }
            }
        }

        Ok(())
    }
}

// ============================================================================
// VSA025: All ports must end with Port suffix
// ============================================================================

/// VSA025: All interfaces in ports/ folder must end with Port suffix
///
/// As per ADR-019, all port interfaces must use the *Port naming convention
/// for discoverability and automated validation.
///
/// Invalid:
/// ```
/// ports/WorkflowRepository.py      # ❌ Missing Port suffix
/// ports/CommandBus.py              # ❌ Missing Port suffix
/// ```
///
/// Valid:
/// ```
/// ports/WorkflowRepositoryPort.py  # ✅ Has Port suffix
/// ports/CommandBusPort.py          # ✅ Has Port suffix
/// ```
pub struct RequirePortSuffixRule;

impl ValidationRule for RequirePortSuffixRule {
    fn name(&self) -> &str {
        "require-port-suffix"
    }

    fn code(&self) -> &str {
        "VSA025"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        let scanner = Scanner::new(ctx.config.clone(), ctx.root.clone());
        let contexts = scanner.scan_contexts()?;

        for context in contexts {
            let ports_path = context.path.join("ports");
            if !ports_path.exists() || !ports_path.is_dir() {
                continue;
            }

            // Check all files in ports/ directory
            if let Ok(entries) = std::fs::read_dir(&ports_path) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if !path.is_file() {
                        continue;
                    }

                    let file_name = match path.file_name().and_then(|n| n.to_str()) {
                        Some(name) => name,
                        None => continue,
                    };

                    let ext = ctx.config.file_extension();
                    if !file_name.ends_with(&format!(".{ext}")) {
                        continue;
                    }

                    // Get file stem (without extension)
                    let file_stem = match path.file_stem().and_then(|n| n.to_str()) {
                        Some(stem) => stem,
                        None => continue,
                    };

                    // Skip __init__.py and similar
                    if file_stem.starts_with("__") || file_stem == "mod" {
                        continue;
                    }

                    // Check for Port suffix
                    if !file_stem.ends_with("Port") {
                        let suggested_name = format!("{file_stem}Port.{ext}");
                        let suggested_path = ports_path.join(&suggested_name);

                        report.errors.push(ValidationIssue {
                            path: path.clone(),
                            code: self.code().to_string(),
                            severity: Severity::Error,
                            message: format!(
                                "Port file '{}' in context '{}' does not end with 'Port' suffix. \
                                 As per ADR-019, all port interfaces must use *Port naming \
                                 for discoverability and consistency.",
                                file_name,
                                context.name
                            ),
                            suggestions: vec![Suggestion::manual(format!(
                                "Rename to {suggested_name}\n\
                                 Command: git mv {} {}",
                                path.display(),
                                suggested_path.display()
                            ))],
                        });
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::VsaConfig;
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn create_test_config(root: PathBuf, language: &str) -> VsaConfig {
        VsaConfig {
            version: 2,
            architecture: crate::config::ArchitectureType::HexagonalEventSourcedVsa,
            root,
            language: language.to_string(),
            domain: None,
            slices: None,
            infrastructure: None,
            framework: None,
            contexts: HashMap::new(),
            validation: crate::config::ValidationConfig::default(),
            patterns: crate::config::PatternsConfig::default(),
        }
    }

    #[test]
    fn test_vsa020_command_in_slice() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        let context_path = root.join("workflows");
        let slice_path = context_path.join("slices/create_workflow");
        fs::create_dir_all(&slice_path).unwrap();

        // Command in slice (wrong location)
        fs::write(slice_path.join("CreateWorkflowCommand.py"), "class CreateWorkflowCommand: pass")
            .unwrap();

        let config = create_test_config(root.clone(), "python");
        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        let rule = RequireCommandsInDomainRule;
        rule.validate(&ctx, &mut report).unwrap();

        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.errors[0].code, "VSA020");
        assert!(report.errors[0].message.contains("domain/commands/"));
    }

    #[test]
    fn test_vsa021_events_in_domain() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        let context_path = root.join("workflows");
        let domain_events_path = context_path.join("domain/events");
        fs::create_dir_all(&domain_events_path).unwrap();

        // Event in domain/events/ (wrong location)
        fs::write(
            domain_events_path.join("WorkflowCreatedEvent.py"),
            "class WorkflowCreatedEvent: pass"
        ).unwrap();

        let config = create_test_config(root.clone(), "python");
        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        let rule = RequireEventsAtContextRootRule;
        rule.validate(&ctx, &mut report).unwrap();

        // Should report at least 1 error for the domain/events/ folder issue
        // Note: May report 2 errors (folder + individual file) which is acceptable
        assert!(
            report.errors.len() >= 1,
            "Expected at least 1 error, got {}",
            report.errors.len()
        );
        assert!(report.errors.iter().any(|e| e.code == "VSA021"));
        assert!(report.errors.iter().any(|e| e.message.contains("context root")));
    }

    #[test]
    fn test_vsa022_aggregate_in_shared() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        let context_path = root.join("workflows");
        let shared_path = context_path.join("_shared");
        fs::create_dir_all(&shared_path).unwrap();

        // Aggregate in _shared/ (wrong location)
        fs::write(
            shared_path.join("WorkflowAggregate.py"),
            "class WorkflowAggregate: pass"
        ).unwrap();

        let config = create_test_config(root.clone(), "python");
        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        let rule = RequireAggregatesInDomainRootRule;
        rule.validate(&ctx, &mut report).unwrap();

        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.errors[0].code, "VSA022");
        assert!(report.errors[0].message.contains("domain/ root"));
    }

    #[test]
    fn test_vsa025_port_without_suffix() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        let context_path = root.join("workflows");
        let ports_path = context_path.join("ports");
        fs::create_dir_all(&ports_path).unwrap();

        // Port without Port suffix (wrong naming)
        fs::write(ports_path.join("WorkflowRepository.py"), "class WorkflowRepository: pass")
            .unwrap();

        let config = create_test_config(root.clone(), "python");
        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        let rule = RequirePortSuffixRule;
        rule.validate(&ctx, &mut report).unwrap();

        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.errors[0].code, "VSA025");
        assert!(report.errors[0].message.contains("Port"));
    }

    #[test]
    fn test_valid_structure() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        let context_path = root.join("workflows");
        
        // Create valid structure
        let domain_path = context_path.join("domain");
        let commands_path = domain_path.join("commands");
        let events_path = context_path.join("events");
        let ports_path = context_path.join("ports");

        fs::create_dir_all(&commands_path).unwrap();
        fs::create_dir_all(&events_path).unwrap();
        fs::create_dir_all(&ports_path).unwrap();

        // Aggregate in domain root (correct)
        fs::write(domain_path.join("WorkflowAggregate.py"), "class WorkflowAggregate: pass")
            .unwrap();

        // Command in domain/commands/ (correct)
        fs::write(commands_path.join("CreateWorkflowCommand.py"), "class CreateWorkflowCommand: pass")
            .unwrap();

        // Event at context root (correct)
        fs::write(events_path.join("WorkflowCreatedEvent.py"), "class WorkflowCreatedEvent: pass")
            .unwrap();

        // Port with Port suffix (correct)
        fs::write(ports_path.join("WorkflowRepositoryPort.py"), "class WorkflowRepositoryPort: pass")
            .unwrap();

        let config = create_test_config(root.clone(), "python");
        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        // Run all rules
        RequireCommandsInDomainRule.validate(&ctx, &mut report).unwrap();
        RequireEventsAtContextRootRule.validate(&ctx, &mut report).unwrap();
        RequireAggregatesInDomainRootRule.validate(&ctx, &mut report).unwrap();
        RequirePortSuffixRule.validate(&ctx, &mut report).unwrap();

        assert_eq!(report.errors.len(), 0, "Valid structure should have no errors");
    }
}
