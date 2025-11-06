//! Integration event validation rules

use super::{
    EnhancedValidationReport, Severity, Suggestion, ValidationContext, ValidationIssue,
    ValidationRule,
};
use crate::error::Result;
use crate::integration_events::IntegrationEventRegistry;

/// Rule: No duplicate integration events across contexts
pub struct NoDuplicateIntegrationEventsRule;

impl ValidationRule for NoDuplicateIntegrationEventsRule {
    fn name(&self) -> &str {
        "no-duplicate-integration-events"
    }

    fn code(&self) -> &str {
        "VSA100"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        let registry = IntegrationEventRegistry::scan(&ctx.config, &ctx.root)?;
        let duplicates = registry.find_duplicates();

        for (event_name, publishers) in duplicates {
            report.errors.push(ValidationIssue {
                path: ctx.root.clone(),
                code: self.code().to_string(),
                severity: Severity::Error,
                message: format!(
                    "Integration event '{event_name}' is defined in multiple contexts: {publishers:?}"
                ),
                suggestions: vec![Suggestion::manual(format!(
                    "Move '{event_name}' to _shared/integration-events/ and import from there"
                ))],
            });
        }

        Ok(())
    }
}

/// Rule: Integration events should be in _shared folder
pub struct IntegrationEventsLocationRule;

impl ValidationRule for IntegrationEventsLocationRule {
    fn name(&self) -> &str {
        "integration-events-location"
    }

    fn code(&self) -> &str {
        "VSA101"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        let registry = IntegrationEventRegistry::scan(&ctx.config, &ctx.root)?;

        for event in registry.all_events() {
            // Check if event is in a _shared folder
            let in_shared = event.path.components().any(|c| c.as_os_str() == "_shared");

            if !in_shared {
                report.warnings.push(ValidationIssue {
                    path: event.path.clone(),
                    code: self.code().to_string(),
                    severity: Severity::Warning,
                    message: format!(
                        "Integration event '{}' should be in _shared/integration-events/",
                        event.name
                    ),
                    suggestions: vec![Suggestion::manual(
                        "Move integration events to context _shared/integration-events/ directory"
                            .to_string(),
                    )],
                });
            }
        }

        Ok(())
    }
}

/// Rule: Integration event names should end with IntegrationEvent
pub struct IntegrationEventNamingRule;

impl ValidationRule for IntegrationEventNamingRule {
    fn name(&self) -> &str {
        "integration-event-naming"
    }

    fn code(&self) -> &str {
        "VSA102"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        let registry = IntegrationEventRegistry::scan(&ctx.config, &ctx.root)?;

        for event in registry.all_events() {
            if !event.name.ends_with("IntegrationEvent") {
                report.warnings.push(ValidationIssue {
                    path: event.path.clone(),
                    code: self.code().to_string(),
                    severity: Severity::Warning,
                    message: format!(
                        "Integration event '{}' should end with 'IntegrationEvent' suffix",
                        event.name
                    ),
                    suggestions: vec![Suggestion::rename_file(
                        event.path.clone(),
                        event.path.parent().unwrap().join(format!(
                            "{}IntegrationEvent.{}",
                            event.name.trim_end_matches("Event"),
                            ctx.config.file_extension()
                        )),
                        format!(
                            "Rename to {}IntegrationEvent",
                            event.name.trim_end_matches("Event")
                        ),
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
    fn test_no_duplicate_integration_events_rule() {
        let rule = NoDuplicateIntegrationEventsRule;
        assert_eq!(rule.name(), "no-duplicate-integration-events");
        assert_eq!(rule.code(), "VSA100");
    }
}
