//! Bounded context validation rules

use super::{
    EnhancedValidationReport, Severity, Suggestion, ValidationContext, ValidationIssue,
    ValidationRule,
};
use crate::error::Result;
use crate::integration_events::IntegrationEventRegistry;
use crate::scanner::Scanner;
use std::collections::{HashMap, HashSet};

/// Rule: Check for circular dependencies between contexts
pub struct NoCircularDependenciesRule;

impl ValidationRule for NoCircularDependenciesRule {
    fn name(&self) -> &str {
        "no-circular-dependencies"
    }

    fn code(&self) -> &str {
        "VSA200"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        // Build dependency graph from integration events
        let registry = IntegrationEventRegistry::scan(&ctx.config, &ctx.root)?;
        let scanner = Scanner::new(ctx.config.clone(), ctx.root.clone());
        let contexts = scanner.scan_contexts()?;

        let mut dependencies: HashMap<String, HashSet<String>> = HashMap::new();

        // For each context, find which events it subscribes to (imports)
        for context in &contexts {
            let deps = dependencies.entry(context.name.clone()).or_default();

            // Find all imports of integration events in this context
            for event in registry.all_events() {
                if event.publisher != context.name {
                    // Check if any files in this context import this event
                    // This is a simplified check - in production, we'd parse imports
                    deps.insert(event.publisher.clone());
                }
            }
        }

        // Check for circular dependencies
        for context_name in dependencies.keys() {
            if let Some(cycle) = self.find_cycle(context_name, &dependencies) {
                report.errors.push(ValidationIssue {
                    path: ctx.root.clone(),
                    code: self.code().to_string(),
                    severity: Severity::Error,
                    message: format!(
                        "Circular dependency detected: {}",
                        cycle.join(" -> ")
                    ),
                    suggestions: vec![Suggestion::manual(
                        "Refactor to remove circular dependencies between contexts. Consider introducing a mediator context or restructuring event flows."
                    )],
                });
            }
        }

        Ok(())
    }
}

impl NoCircularDependenciesRule {
    fn find_cycle(
        &self,
        start: &str,
        dependencies: &HashMap<String, HashSet<String>>,
    ) -> Option<Vec<String>> {
        let mut visited = HashSet::new();
        let mut path = Vec::new();

        if self.dfs(start, start, dependencies, &mut visited, &mut path) {
            path.push(start.to_string());
            Some(path)
        } else {
            None
        }
    }

    #[allow(clippy::only_used_in_recursion)]
    fn dfs(
        &self,
        current: &str,
        target: &str,
        dependencies: &HashMap<String, HashSet<String>>,
        visited: &mut HashSet<String>,
        path: &mut Vec<String>,
    ) -> bool {
        if !path.is_empty() && current == target {
            return true;
        }

        if visited.contains(current) {
            return false;
        }

        visited.insert(current.to_string());
        path.push(current.to_string());

        if let Some(deps) = dependencies.get(current) {
            for dep in deps {
                if self.dfs(dep, target, dependencies, visited, path) {
                    return true;
                }
            }
        }

        path.pop();
        false
    }
}

/// Rule: Contexts should not directly access each other's internals
pub struct ContextBoundariesRule;

impl ValidationRule for ContextBoundariesRule {
    fn name(&self) -> &str {
        "context-boundaries"
    }

    fn code(&self) -> &str {
        "VSA201"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        // This rule would check imports to ensure contexts only import
        // from _shared/integration-events, not from each other's internals
        // For now, this is a placeholder - full implementation would require
        // parsing TypeScript/Python/Rust imports

        let scanner = Scanner::new(ctx.config.clone(), ctx.root.clone());
        let contexts = scanner.scan_contexts()?;

        // Check for suspicious directory structures
        for context in contexts {
            let features = scanner.scan_features(&context.path)?;

            for feature in features {
                // Check if feature path contains another context name
                let feature_path_str = feature.path.to_string_lossy();

                // This is a basic check - production would parse actual imports
                if feature_path_str.contains("../") {
                    report.warnings.push(ValidationIssue {
                        path: feature.path.clone(),
                        code: self.code().to_string(),
                        severity: Severity::Warning,
                        message: format!(
                            "Feature '{}' may be accessing parent directories - ensure it only imports from _shared/integration-events",
                            feature.name
                        ),
                        suggestions: vec![Suggestion::manual(
                            "Review imports to ensure proper context boundaries"
                        )],
                    });
                }
            }
        }

        Ok(())
    }
}

/// Rule: Each context should have a _shared folder
pub struct RequireSharedFolderRule;

impl ValidationRule for RequireSharedFolderRule {
    fn name(&self) -> &str {
        "require-shared-folder"
    }

    fn code(&self) -> &str {
        "VSA202"
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

            if !shared_path.exists() {
                report.warnings.push(ValidationIssue {
                    path: context.path.clone(),
                    code: self.code().to_string(),
                    severity: Severity::Warning,
                    message: format!(
                        "Context '{}' is missing _shared folder for integration events and types",
                        context.name
                    ),
                    suggestions: vec![Suggestion::create_file(
                        shared_path.join(".gitkeep"),
                        format!("Create _shared/ directory in {}", context.name),
                    )],
                });
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{PatternsConfig, ValidationConfig, VsaConfig};
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn create_test_config(root: PathBuf) -> VsaConfig {
        VsaConfig {
            version: 1,
            architecture: crate::config::ArchitectureType::default(),
            root: root.clone(),
            language: "typescript".to_string(),
            domain: None,
            slices: None,
            infrastructure: None,
            framework: None,
            contexts: HashMap::new(),
            validation: ValidationConfig::default(),
            patterns: PatternsConfig::default(),
        }
    }

    #[test]
    fn test_no_circular_dependencies_rule() {
        let rule = NoCircularDependenciesRule;
        assert_eq!(rule.name(), "no-circular-dependencies");
        assert_eq!(rule.code(), "VSA200");
    }

    #[test]
    fn test_find_cycle_simple() {
        let rule = NoCircularDependenciesRule;

        let mut dependencies = HashMap::new();
        dependencies.insert("A".to_string(), HashSet::from(["B".to_string()]));
        dependencies.insert("B".to_string(), HashSet::from(["A".to_string()]));

        let cycle = rule.find_cycle("A", &dependencies);
        assert!(cycle.is_some());
    }

    #[test]
    fn test_find_cycle_none() {
        let rule = NoCircularDependenciesRule;

        let mut dependencies = HashMap::new();
        dependencies.insert("A".to_string(), HashSet::from(["B".to_string()]));
        dependencies.insert("B".to_string(), HashSet::new());

        let cycle = rule.find_cycle("A", &dependencies);
        assert!(cycle.is_none());
    }
}
