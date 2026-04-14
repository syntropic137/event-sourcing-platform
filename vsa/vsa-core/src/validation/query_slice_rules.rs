//! Query slice validation rules
//!
//! Validates that query slices follow CQRS best practices:
//! - VSA007: Query slices require a projection
//! - VSA008: Query slices require a handler
//! - VSA009: Projections should subscribe to events

use super::{EnhancedValidationReport, Severity, Suggestion, ValidationContext, ValidationIssue};
use crate::config::SliceType;
use crate::error::Result;
use crate::scanners::{ProjectionScanner, SliceScanner};

use super::rules::ValidationRule;

/// VSA007: Query slices must have a projection
///
/// In CQRS, query slices read from projections (read models).
/// Without a projection, there's no read model to query.
pub struct RequireProjectionForQueryRule;

impl ValidationRule for RequireProjectionForQueryRule {
    fn name(&self) -> &str {
        "require-projection-for-query"
    }

    fn code(&self) -> &str {
        "VSA007"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        // Get slices config if available
        let slices_config = ctx.config.slices.as_ref();

        // Check if query slices require projections (default: true)
        let require_projection = slices_config
            .and_then(|s| s.query.as_ref())
            .map(|q| q.require_projection)
            .unwrap_or(true);

        if !require_projection {
            return Ok(());
        }

        // Scan for slices
        let scanner = SliceScanner::new(slices_config, &ctx.root);
        let slices = scanner.scan()?;

        for slice in slices {
            if slice.slice_type == SliceType::Query && !slice.has_projection() {
                // Suggest creating a projection
                // Note: to_pascal_case already handles both '-' and '_' delimiters
                let projection_name = format!("{}Projection", to_pascal_case(&slice.name));
                let ext = ctx.config.file_extension();
                let projection_path = slice.path.join(format!("{projection_name}.{ext}"));

                report.errors.push(ValidationIssue {
                        path: slice.path.clone(),
                        code: self.code().to_string(),
                        severity: Severity::Error,
                        message: format!(
                            "Query slice '{}' is missing a projection. Projections build read models from events.",
                            slice.name
                        ),
                        suggestions: vec![Suggestion::create_file(
                            projection_path,
                            format!("Create {projection_name} to build the read model"),
                        )],
                    });
            }
        }

        Ok(())
    }
}

/// VSA008: Query slices must have a handler
///
/// The handler executes the query against the projection's read model.
pub struct RequireHandlerForQueryRule;

impl ValidationRule for RequireHandlerForQueryRule {
    fn name(&self) -> &str {
        "require-handler-for-query"
    }

    fn code(&self) -> &str {
        "VSA008"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        // Get slices config if available
        let slices_config = ctx.config.slices.as_ref();

        // Scan for slices
        let scanner = SliceScanner::new(slices_config, &ctx.root);
        let slices = scanner.scan()?;

        for slice in slices {
            if slice.slice_type == SliceType::Query && !slice.has_handler() {
                // Suggest creating a handler
                // Note: to_pascal_case already handles both '-' and '_' delimiters
                let handler_name = format!("{}Handler", to_pascal_case(&slice.name));
                let ext = ctx.config.file_extension();
                let handler_path = slice.path.join(format!("{handler_name}.{ext}"));

                report.errors.push(ValidationIssue {
                        path: slice.path.clone(),
                        code: self.code().to_string(),
                        severity: Severity::Error,
                        message: format!(
                            "Query slice '{}' is missing a handler. Handlers execute queries against the projection.",
                            slice.name
                        ),
                        suggestions: vec![Suggestion::create_file(
                            handler_path,
                            format!("Create {handler_name} to handle the query"),
                        )],
                    });
            }
        }

        Ok(())
    }
}

/// VSA009: Projections should subscribe to events
///
/// Projections that don't subscribe to any events can't build their read model.
/// This is a warning since some projections may use a different mechanism.
pub struct ProjectionEventSubscriptionRule;

impl ValidationRule for ProjectionEventSubscriptionRule {
    fn name(&self) -> &str {
        "projection-event-subscription"
    }

    fn code(&self) -> &str {
        "VSA009"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        // Get query slice config
        let query_config = ctx.config.slices.as_ref().and_then(|s| s.query.as_ref());

        // Scan for projections
        let scanner = ProjectionScanner::new(query_config, &ctx.root);
        let projections = scanner.scan()?;

        for projection in projections {
            if !projection.has_subscriptions() {
                report.warnings.push(ValidationIssue {
                    path: projection.file_path.clone(),
                    code: self.code().to_string(),
                    severity: Severity::Warning,
                    message: format!(
                        "Projection '{}' has no event subscriptions. Projections should handle events to build read models.",
                        projection.name
                    ),
                    suggestions: vec![Suggestion::manual(
                        "Add event handler methods (e.g., on_WorkflowCreated, @Handles(WorkflowCreatedEvent))"
                    )],
                });
            }
        }

        Ok(())
    }
}

/// Convert snake_case or kebab-case to PascalCase
fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .flat_map(|part| part.split('-'))
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{SliceType, SlicesConfig, VsaConfig};
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn create_test_config(root: PathBuf) -> VsaConfig {
        VsaConfig {
            version: 2,
            architecture: crate::config::ArchitectureType::HexagonalEventSourcedVsa,
            root,
            language: "typescript".to_string(),
            domain: None,
            slices: Some(SlicesConfig {
                path: PathBuf::from("slices"),
                types: vec![SliceType::Command, SliceType::Query, SliceType::Saga],
                metadata_file: "slice.yaml".to_string(),
                command: None,
                query: None,
                saga: None,
            }),
            infrastructure: None,
            framework: None,
            contexts: HashMap::new(),
            validation: crate::config::ValidationConfig::default(),
            patterns: crate::config::PatternsConfig::default(),
            projection_allowed_prefixes: None,
            cross_context_scan_paths: Vec::new(),
            exceptions: Vec::new(),
        }
    }

    #[test]
    fn test_to_pascal_case() {
        assert_eq!(to_pascal_case("list_workflows"), "ListWorkflows");
        assert_eq!(to_pascal_case("get-workflow-detail"), "GetWorkflowDetail");
        assert_eq!(to_pascal_case("create_order"), "CreateOrder");
        assert_eq!(to_pascal_case("simple"), "Simple");
    }

    #[test]
    fn test_vsa007_query_without_projection() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        // Create query slice WITHOUT projection
        let slice_path = root.join("slices/list_orders");
        fs::create_dir_all(&slice_path).unwrap();
        fs::write(slice_path.join("ListOrdersQuery.ts"), "export class ListOrdersQuery {}")
            .unwrap();
        fs::write(slice_path.join("ListOrdersHandler.ts"), "export class ListOrdersHandler {}")
            .unwrap();
        // Note: No projection file

        let config = create_test_config(root.clone());
        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        let rule = RequireProjectionForQueryRule;
        rule.validate(&ctx, &mut report).unwrap();

        // Should have 1 error
        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.errors[0].code, "VSA007");
        assert!(report.errors[0].message.contains("missing a projection"));
    }

    #[test]
    fn test_vsa007_query_with_projection() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        // Create query slice WITH projection
        let slice_path = root.join("slices/list_orders");
        fs::create_dir_all(&slice_path).unwrap();
        fs::write(slice_path.join("ListOrdersQuery.ts"), "export class ListOrdersQuery {}")
            .unwrap();
        fs::write(slice_path.join("OrderListProjection.ts"), "export class OrderListProjection {}")
            .unwrap();
        fs::write(slice_path.join("ListOrdersHandler.ts"), "export class ListOrdersHandler {}")
            .unwrap();

        let config = create_test_config(root.clone());
        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        let rule = RequireProjectionForQueryRule;
        rule.validate(&ctx, &mut report).unwrap();

        // Should have no errors
        assert_eq!(report.errors.len(), 0);
    }

    #[test]
    fn test_vsa008_query_without_handler() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        // Create query slice WITHOUT handler
        let slice_path = root.join("slices/list_orders");
        fs::create_dir_all(&slice_path).unwrap();
        fs::write(slice_path.join("ListOrdersQuery.ts"), "export class ListOrdersQuery {}")
            .unwrap();
        fs::write(slice_path.join("OrderListProjection.ts"), "export class OrderListProjection {}")
            .unwrap();
        // Note: No handler file

        let config = create_test_config(root.clone());
        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        let rule = RequireHandlerForQueryRule;
        rule.validate(&ctx, &mut report).unwrap();

        // Should have 1 error
        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.errors[0].code, "VSA008");
        assert!(report.errors[0].message.contains("missing a handler"));
    }

    #[test]
    fn test_vsa009_projection_without_events() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        // Create projection without event subscriptions
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("EmptyProjection.ts"),
            r#"
export class EmptyProjection {
    // No event handlers
}
"#,
        )
        .unwrap();

        let config = create_test_config(root.clone());
        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        let rule = ProjectionEventSubscriptionRule;
        rule.validate(&ctx, &mut report).unwrap();

        // Should have 1 warning
        assert_eq!(report.warnings.len(), 1);
        assert_eq!(report.warnings[0].code, "VSA009");
        assert!(report.warnings[0].message.contains("no event subscriptions"));
    }

    #[test]
    fn test_vsa009_projection_with_events() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        // Create projection WITH event subscriptions
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("WorkflowListProjection.ts"),
            r#"
export class WorkflowListProjection {
    @Handles(WorkflowCreatedEvent)
    onWorkflowCreated(event: WorkflowCreatedEvent) {}
}
"#,
        )
        .unwrap();

        let config = create_test_config(root.clone());
        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        let rule = ProjectionEventSubscriptionRule;
        rule.validate(&ctx, &mut report).unwrap();

        // Should have no warnings
        assert_eq!(report.warnings.len(), 0);
    }

    #[test]
    fn test_command_slice_not_affected() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        // Create COMMAND slice (not query)
        let slice_path = root.join("slices/create_order");
        fs::create_dir_all(&slice_path).unwrap();
        fs::write(slice_path.join("CreateOrderCommand.ts"), "export class CreateOrderCommand {}")
            .unwrap();
        fs::write(slice_path.join("CreateOrderHandler.ts"), "export class CreateOrderHandler {}")
            .unwrap();
        // Note: No projection (and that's fine for command slices)

        let config = create_test_config(root.clone());
        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        let rule = RequireProjectionForQueryRule;
        rule.validate(&ctx, &mut report).unwrap();

        // Should have no errors (command slices don't need projections)
        assert_eq!(report.errors.len(), 0);
    }
}
