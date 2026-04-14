//! Dependency validation rules for hexagonal architecture
//!
//! These rules enforce the strict dependency direction from ADR-019:
//! - Domain: Pure, no external dependencies
//! - Events: Pure data, no dependencies
//! - Ports: Only domain and events
//! - Application: No slices or infrastructure
//! - Infrastructure: Can depend on everything above
//! - Slices: Can depend on everything except other slices

use super::import_parser::parse_imports;
use super::{
    is_test_or_conftest, EnhancedValidationReport, Severity, Suggestion, ValidationContext,
    ValidationIssue, ValidationRule,
};
use crate::error::Result;
use crate::scanner::Scanner;
use std::collections::HashSet;
use std::path::PathBuf;

// ============================================================================
// VSA027: Domain Purity Rule
// ============================================================================

/// Rule: Domain layer must not import from ports, application, infrastructure, or slices
/// Domain CAN import from domain/events/ since events are part of the domain layer
pub struct DomainPurityRule;

impl ValidationRule for DomainPurityRule {
    fn name(&self) -> &str {
        "domain-purity"
    }

    fn code(&self) -> &str {
        "VSA027"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        let scanner = Scanner::new(ctx.config.clone(), ctx.root.clone());
        let contexts = scanner.scan_contexts()?;

        for context in contexts {
            let domain_path = context.path.join("domain");
            if !domain_path.exists() {
                continue;
            }

            // Scan all files in domain/ recursively
            for entry in walkdir::WalkDir::new(&domain_path)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
            {
                let file_path = entry.path();

                // Parse imports
                let imports = match parse_imports(file_path) {
                    Ok(imports) => imports,
                    Err(_) => continue,
                };

                // Check each import
                for import in imports {
                    // Detect if import targets forbidden layers
                    let is_forbidden = Self::is_forbidden_import(&import.module);

                    if is_forbidden {
                        let relative_path = file_path.strip_prefix(&ctx.root).unwrap_or(file_path);

                        report.errors.push(ValidationIssue {
                            path: file_path.to_path_buf(),
                            code: self.code().to_string(),
                            severity: Severity::Error,
                            message: format!(
                                "Domain file '{}' imports from forbidden layer: '{}' (line {})\n\
                                 Domain must be pure - no dependencies on ports/, application/, infrastructure/, or slices/\n\
                                 (domain/events/ is allowed - events are part of domain language)",
                                relative_path.display(),
                                import.module,
                                import.line_number
                            ),
                            suggestions: vec![
                                Suggestion::manual(
                                    "Remove this import. Domain should only contain pure business logic with no external dependencies"
                                ),
                                Suggestion::manual(
                                    "If you need external functionality, define an interface in ports/ and inject it"
                                ),
                            ],
                        });
                    }
                }
            }
        }

        Ok(())
    }
}

impl DomainPurityRule {
    /// Check if an import module targets a forbidden layer for domain
    fn is_forbidden_import(module: &str) -> bool {
        let normalized = module.replace("::", "/").replace('.', "/");

        // Check for forbidden layers in import path
        // Per ADR-019: Events should be in domain/events/ (domain cohesion)
        // Context-root events/ is an old pattern and should not be used
        // Logic: Allow domain/events/, forbid context-root events/
        let has_forbidden_events = normalized.contains("/events/")
            && !normalized.contains("/domain/events/")
            || normalized.starts_with("events/") && !normalized.starts_with("domain/events/")
            || (normalized.starts_with("events")
                && !normalized.starts_with("domain/events")
                && !normalized.contains('/'));

        has_forbidden_events
            || normalized.contains("/ports/")
            || normalized.contains("/application/")
            || normalized.contains("/infrastructure/")
            || normalized.contains("/slices/")
            || normalized.starts_with("ports/")
            || normalized.starts_with("application/")
            || normalized.starts_with("infrastructure/")
            || normalized.starts_with("slices/")
            // Also check for module names without path separators (same-level imports)
            || normalized.starts_with("ports")
            || normalized.starts_with("application")
            || normalized.starts_with("infrastructure")
            || normalized.starts_with("slices")
    }
}

// ============================================================================
// VSA028: Events Isolation Rule
// ============================================================================

/// Rule: Events layer must not import anything (pure data structures)
pub struct EventsIsolationRule;

impl ValidationRule for EventsIsolationRule {
    fn name(&self) -> &str {
        "events-isolation"
    }

    fn code(&self) -> &str {
        "VSA028"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        let scanner = Scanner::new(ctx.config.clone(), ctx.root.clone());
        let contexts = scanner.scan_contexts()?;

        for context in contexts {
            // Use configured events path from domain config (supports domain/events/)
            // Skip validation if domain config is not present (domain is optional)
            let domain_config = match ctx.config.domain.as_ref() {
                Some(config) => config,
                None => continue, // Skip validation for contexts without domain config
            };
            let events_path =
                context.path.join(&domain_config.path).join(&domain_config.events.path);
            if !events_path.exists() {
                continue;
            }

            // Scan all files in events/ recursively
            for entry in walkdir::WalkDir::new(&events_path)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
            {
                let file_path = entry.path();

                // Skip __init__.py files (they need to import from same package for re-exports)
                if file_path.file_name().and_then(|n| n.to_str()) == Some("__init__.py") {
                    continue;
                }

                // Parse imports
                let imports = match parse_imports(file_path) {
                    Ok(imports) => imports,
                    Err(_) => continue,
                };

                // Events should have NO imports from project code (only stdlib/typing)
                for import in imports {
                    // Allow only standard library imports (heuristic)
                    if !Self::is_stdlib_import(&import.module) {
                        let relative_path = file_path.strip_prefix(&ctx.root).unwrap_or(file_path);

                        report.errors.push(ValidationIssue {
                            path: file_path.to_path_buf(),
                            code: self.code().to_string(),
                            severity: Severity::Error,
                            message: format!(
                                "Event file '{}' imports from '{}' (line {})\n\
                                 Events must be pure data structures with no dependencies",
                                relative_path.display(),
                                import.module,
                                import.line_number
                            ),
                            suggestions: vec![
                                Suggestion::manual(
                                    "Remove this import. Events should only be data classes/structures"
                                ),
                                Suggestion::manual(
                                    "Use primitive types or standard library types only"
                                ),
                            ],
                        });
                    }
                }
            }
        }

        Ok(())
    }
}

impl EventsIsolationRule {
    /// Check if import is from standard library or allowed framework (allow these)
    fn is_stdlib_import(module: &str) -> bool {
        // Python stdlib
        if module.starts_with("typing")
            || module.starts_with("dataclasses")
            || module.starts_with("datetime")
            || module.starts_with("uuid")
            || module.starts_with("enum")
            || module.starts_with("decimal")
            || module.starts_with("__future__")
            || module == "typing"
            || module == "dataclasses"
            || module == "datetime"
            || module == "uuid"
            || module == "enum"
            || module == "decimal"
            || module == "__future__"
        {
            return true;
        }

        // Event sourcing framework (required for @event decorator and DomainEvent)
        if module.starts_with("event_sourcing") || module == "event_sourcing" {
            return true;
        }

        // Value objects from same domain (events can use value objects for structured data)
        // Pattern: aef_domain.contexts.{context}._shared.value_objects or domain.value_objects
        if module.contains(".value_objects") || module.contains("ValueObjects") {
            return true;
        }

        // Pydantic (validation library commonly used in events for data validation)
        if module.starts_with("pydantic") || module == "pydantic" {
            return true;
        }

        // Rust std
        if module.starts_with("std::") {
            return true;
        }

        // TypeScript/JavaScript - no imports typically needed for data structures

        false
    }
}

// ============================================================================
// VSA029: Port Isolation Rule
// ============================================================================

/// Rule: Ports layer may only import from domain and events
pub struct PortIsolationRule;

impl ValidationRule for PortIsolationRule {
    fn name(&self) -> &str {
        "port-isolation"
    }

    fn code(&self) -> &str {
        "VSA029"
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
            if !ports_path.exists() {
                continue;
            }

            // Scan all files in ports/ recursively
            for entry in walkdir::WalkDir::new(&ports_path)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
            {
                let file_path = entry.path();

                // Parse imports
                let imports = match parse_imports(file_path) {
                    Ok(imports) => imports,
                    Err(_) => continue,
                };

                // Check each import
                for import in imports {
                    // Ports can only import from domain/ and events/ (and stdlib)
                    if !Self::is_allowed_import(&import.module) {
                        let relative_path = file_path.strip_prefix(&ctx.root).unwrap_or(file_path);

                        report.errors.push(ValidationIssue {
                            path: file_path.to_path_buf(),
                            code: self.code().to_string(),
                            severity: Severity::Error,
                            message: format!(
                                "Port file '{}' imports from forbidden layer: '{}' (line {})\n\
                                 Ports may only import from domain/ and events/",
                                relative_path.display(),
                                import.module,
                                import.line_number
                            ),
                            suggestions: vec![
                                Suggestion::manual(
                                    "Ports are interfaces - they should only reference domain types and events"
                                ),
                            ],
                        });
                    }
                }
            }
        }

        Ok(())
    }
}

impl PortIsolationRule {
    /// Check if import is allowed for ports layer
    fn is_allowed_import(module: &str) -> bool {
        let normalized = module.replace("::", "/").replace('.', "/");

        // Allow domain and events
        if normalized.contains("/domain/")
            || normalized.contains("/events/")
            || normalized.starts_with("domain/")
            || normalized.starts_with("events/")
            || normalized.starts_with("domain")
            || normalized.starts_with("events")
        {
            return true;
        }

        // Allow standard library
        if EventsIsolationRule::is_stdlib_import(module) {
            return true;
        }

        // Forbidden: application, infrastructure, slices
        !normalized.contains("/application/")
            && !normalized.contains("/infrastructure/")
            && !normalized.contains("/slices/")
            && !normalized.starts_with("application")
            && !normalized.starts_with("infrastructure")
            && !normalized.starts_with("slices")
    }
}

// ============================================================================
// VSA030: Application Isolation Rule
// ============================================================================

/// Rule: Application layer must not import from slices or infrastructure
pub struct ApplicationIsolationRule;

impl ValidationRule for ApplicationIsolationRule {
    fn name(&self) -> &str {
        "application-isolation"
    }

    fn code(&self) -> &str {
        "VSA030"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        let scanner = Scanner::new(ctx.config.clone(), ctx.root.clone());
        let contexts = scanner.scan_contexts()?;

        for context in contexts {
            let application_path = context.path.join("application");
            if !application_path.exists() {
                continue;
            }

            // Scan all files in application/ recursively
            for entry in walkdir::WalkDir::new(&application_path)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
            {
                let file_path = entry.path();

                // Parse imports
                let imports = match parse_imports(file_path) {
                    Ok(imports) => imports,
                    Err(_) => continue,
                };

                // Check each import
                for import in imports {
                    // Application cannot import from slices/ or infrastructure/
                    if Self::is_forbidden_import(&import.module) {
                        let relative_path = file_path.strip_prefix(&ctx.root).unwrap_or(file_path);

                        report.errors.push(ValidationIssue {
                            path: file_path.to_path_buf(),
                            code: self.code().to_string(),
                            severity: Severity::Error,
                            message: format!(
                                "Application file '{}' imports from forbidden layer: '{}' (line {})\n\
                                 Application layer must not depend on slices/ or infrastructure/",
                                relative_path.display(),
                                import.module,
                                import.line_number
                            ),
                            suggestions: vec![
                                Suggestion::manual(
                                    "Application layer coordinates use cases via ports - use dependency injection"
                                ),
                                Suggestion::manual(
                                    "Define interfaces in ports/ and inject implementations"
                                ),
                            ],
                        });
                    }
                }
            }
        }

        Ok(())
    }
}

impl ApplicationIsolationRule {
    /// Check if import is forbidden for application layer
    fn is_forbidden_import(module: &str) -> bool {
        let normalized = module.replace("::", "/").replace('.', "/");

        normalized.contains("/infrastructure/")
            || normalized.contains("/slices/")
            || normalized.starts_with("infrastructure/")
            || normalized.starts_with("slices/")
            || normalized.starts_with("infrastructure")
            || normalized.starts_with("slices")
    }
}

// ============================================================================
// VSA031: Slice Isolation Rule
// ============================================================================

/// Rule: Slices must not import from other slices (horizontal coupling forbidden)
pub struct SliceIsolationRule;

impl ValidationRule for SliceIsolationRule {
    fn name(&self) -> &str {
        "slice-isolation"
    }

    fn code(&self) -> &str {
        "VSA031"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        let scanner = Scanner::new(ctx.config.clone(), ctx.root.clone());
        let contexts = scanner.scan_contexts()?;

        // Slices excluded from isolation checks (shared read models)
        let excluded: HashSet<&str> = ctx
            .config
            .validation
            .exclude_from_isolation
            .iter()
            .map(|s| s.as_str())
            .collect();

        for context in contexts {
            let slices_path = context.path.join("slices");
            if !slices_path.exists() {
                continue;
            }

            // Get all slice directories
            let slice_dirs: Vec<PathBuf> = std::fs::read_dir(&slices_path)
                .ok()
                .into_iter()
                .flatten()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                .map(|e| e.path())
                .collect();

            // Collect real slice names for target validation
            let real_slice_names: HashSet<String> = slice_dirs
                .iter()
                .filter_map(|d| d.file_name())
                .filter_map(|n| n.to_str())
                .map(|s| s.to_string())
                .collect();

            // Check each slice
            for slice_dir in &slice_dirs {
                let slice_name = slice_dir.file_name().unwrap().to_string_lossy();

                // Scan all files in this slice recursively, skipping test files and conftest
                for entry in walkdir::WalkDir::new(slice_dir)
                    .into_iter()
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_type().is_file())
                    .filter(|e| !is_test_or_conftest(e.path()))
                {
                    let file_path = entry.path();

                    // Parse imports
                    let imports = match parse_imports(file_path) {
                        Ok(imports) => imports,
                        Err(_) => continue,
                    };

                    // Check each import for cross-slice dependencies
                    for import in imports {
                        // Check if import targets another slice
                        if let Some(target_slice) = Self::extract_slice_name(&import.module) {
                            // Only flag if target is a different REAL slice and not excluded
                            if target_slice != *slice_name
                                && real_slice_names.contains(&target_slice)
                                && !excluded.contains(target_slice.as_str())
                            {
                                report.errors.push(ValidationIssue {
                                    path: file_path.to_path_buf(),
                                    code: self.code().to_string(),
                                    severity: Severity::Error,
                                    message: format!(
                                        "Slice '{}' imports from slice '{}' (line {})\n\
                                         Cross-slice imports are forbidden - slices must be isolated",
                                        slice_name,
                                        target_slice,
                                        import.line_number
                                    ),
                                    suggestions: vec![
                                        Suggestion::manual(
                                            "Slices communicate via events through EventBus - use publish/subscribe"
                                        ),
                                        Suggestion::manual(
                                            "If code is shared, move it to domain/ or infrastructure/"
                                        ),
                                    ],
                                });
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

impl SliceIsolationRule {
    /// Extract slice name from import path if it targets slices/
    fn extract_slice_name(module: &str) -> Option<String> {
        let normalized = module.replace("::", "/").replace('.', "/");

        // Check if path contains slices/
        if let Some(slices_pos) = normalized.find("/slices/") {
            let after_slices = &normalized[slices_pos + 8..];
            if let Some(next_slash) = after_slices.find('/') {
                return Some(after_slices[..next_slash].to_string());
            } else {
                return Some(after_slices.to_string());
            }
        }

        // Check if starts with slices/
        if let Some(after_slices) = normalized.strip_prefix("slices/") {
            if let Some(next_slash) = after_slices.find('/') {
                return Some(after_slices[..next_slash].to_string());
            } else {
                return Some(after_slices.to_string());
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::VsaConfig;
    use std::collections::HashMap;
    use std::path::Path;
    use tempfile::TempDir;

    fn create_test_context() -> (TempDir, ValidationContext) {
        let temp_dir = TempDir::new().unwrap();
        let config = VsaConfig {
            version: 2,
            architecture: crate::config::ArchitectureType::HexagonalEventSourcedVsa,
            root: temp_dir.path().to_path_buf(),
            language: "python".to_string(),
            domain: Some(crate::config::DomainConfig::default()),
            slices: Some(crate::config::SlicesConfig::default()),
            infrastructure: Some(crate::config::InfrastructureConfig::default()),
            framework: None,
            contexts: HashMap::new(),
            validation: crate::config::ValidationConfig::default(),
            patterns: crate::config::PatternsConfig::default(),
            projection_allowed_prefixes: None,
            cross_context_scan_paths: Vec::new(),
            exceptions: Vec::new(),
        };
        let ctx = ValidationContext::new(config, temp_dir.path().to_path_buf());
        (temp_dir, ctx)
    }

    fn create_context_structure(temp_dir: &Path, context_name: &str) -> PathBuf {
        let context_path = temp_dir.join(context_name);
        std::fs::create_dir_all(context_path.join("domain")).unwrap();
        std::fs::create_dir_all(context_path.join("events")).unwrap();
        std::fs::create_dir_all(context_path.join("ports")).unwrap();
        std::fs::create_dir_all(context_path.join("application")).unwrap();
        std::fs::create_dir_all(context_path.join("infrastructure")).unwrap();
        std::fs::create_dir_all(context_path.join("slices/slice1")).unwrap();
        std::fs::create_dir_all(context_path.join("slices/slice2")).unwrap();
        context_path
    }

    // ========================================================================
    // VSA027: DOMAIN PURITY TESTS
    // ========================================================================

    #[test]
    fn test_vsa027_domain_imports_from_ports() {
        let (_temp_dir, ctx) = create_test_context();
        let context_path = create_context_structure(ctx.root.as_path(), "workflows");

        // Create domain file that imports from ports
        let domain_file = context_path.join("domain/WorkflowAggregate.py");
        std::fs::write(
            &domain_file,
            "from ports.WorkflowRepositoryPort import WorkflowRepositoryPort\n",
        )
        .unwrap();

        let rule = DomainPurityRule;
        let mut report = EnhancedValidationReport::default();

        rule.validate(&ctx, &mut report).unwrap();

        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.errors[0].code, "VSA027");
        assert!(report.errors[0].message.contains("Domain file"));
        assert!(report.errors[0].message.contains("forbidden layer"));
    }

    #[test]
    fn test_vsa027_domain_imports_from_infrastructure() {
        let (_temp_dir, ctx) = create_test_context();
        let context_path = create_context_structure(ctx.root.as_path(), "workflows");

        // Create domain file that imports from infrastructure
        let domain_file = context_path.join("domain/WorkflowAggregate.py");
        std::fs::write(
            &domain_file,
            "from infrastructure.PostgresWorkflowRepository import PostgresWorkflowRepository\n",
        )
        .unwrap();

        let rule = DomainPurityRule;
        let mut report = EnhancedValidationReport::default();

        rule.validate(&ctx, &mut report).unwrap();

        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.errors[0].code, "VSA027");
    }

    #[test]
    fn test_vsa027_domain_pure_allowed() {
        let (_temp_dir, ctx) = create_test_context();
        let context_path = create_context_structure(ctx.root.as_path(), "workflows");

        // Create pure domain file (no forbidden imports)
        let domain_file = context_path.join("domain/WorkflowAggregate.py");
        std::fs::write(
            &domain_file,
            "from typing import Optional\nfrom dataclasses import dataclass\n",
        )
        .unwrap();

        let rule = DomainPurityRule;
        let mut report = EnhancedValidationReport::default();

        rule.validate(&ctx, &mut report).unwrap();

        assert_eq!(report.errors.len(), 0);
    }

    // ========================================================================
    // VSA028: EVENTS ISOLATION TESTS
    // ========================================================================

    #[test]
    fn test_vsa028_events_import_value_objects_allowed() {
        let (_temp_dir, ctx) = create_test_context();
        let context_path = create_context_structure(ctx.root.as_path(), "workflows");

        // Create domain/events/ structure per ADR-019 v2
        let domain_events_path = context_path.join("domain/events");
        std::fs::create_dir_all(&domain_events_path).unwrap();

        // Events CAN import value objects (for structured data in event fields)
        let event_file = domain_events_path.join("WorkflowCreatedEvent.py");
        std::fs::write(&event_file, "from aef_domain.contexts.workflows._shared.value_objects.WorkflowId import WorkflowId\nfrom aef_domain.contexts.workflows._shared.value_objects.WorkflowStatus import WorkflowStatus\n")
            .unwrap();

        let rule = EventsIsolationRule;
        let mut report = EnhancedValidationReport::default();

        rule.validate(&ctx, &mut report).unwrap();

        // Should have 0 errors - events can import value objects per ADR-019 v2
        assert_eq!(
            report.errors.len(),
            0,
            "Events should be allowed to import value objects for structured data"
        );
    }

    #[test]
    fn test_vsa028_events_stdlib_allowed() {
        let (_temp_dir, ctx) = create_test_context();
        let context_path = create_context_structure(ctx.root.as_path(), "workflows");

        // Create domain/events/ structure per ADR-019 and event file with only stdlib imports
        let domain_events_path = context_path.join("domain/events");
        std::fs::create_dir_all(&domain_events_path).unwrap();
        let event_file = domain_events_path.join("WorkflowCreatedEvent.py");
        std::fs::write(
            &event_file,
            "from typing import Optional\nfrom dataclasses import dataclass\nfrom datetime import datetime\n",
        )
        .unwrap();

        let rule = EventsIsolationRule;
        let mut report = EnhancedValidationReport::default();

        rule.validate(&ctx, &mut report).unwrap();

        assert_eq!(report.errors.len(), 0);
    }

    // ========================================================================
    // VSA029: PORT ISOLATION TESTS
    // ========================================================================

    #[test]
    fn test_vsa029_ports_import_infrastructure() {
        let (_temp_dir, ctx) = create_test_context();
        let context_path = create_context_structure(ctx.root.as_path(), "workflows");

        // Create port file that imports from infrastructure
        let port_file = context_path.join("ports/WorkflowRepositoryPort.py");
        std::fs::write(
            &port_file,
            "from infrastructure.PostgresWorkflowRepository import PostgresWorkflowRepository\n",
        )
        .unwrap();

        let rule = PortIsolationRule;
        let mut report = EnhancedValidationReport::default();

        rule.validate(&ctx, &mut report).unwrap();

        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.errors[0].code, "VSA029");
    }

    #[test]
    fn test_vsa029_ports_import_domain_allowed() {
        let (_temp_dir, ctx) = create_test_context();
        let context_path = create_context_structure(ctx.root.as_path(), "workflows");

        // Create port file that imports from domain (allowed)
        let port_file = context_path.join("ports/WorkflowRepositoryPort.py");
        std::fs::write(
            &port_file,
            "from domain.WorkflowAggregate import WorkflowAggregate\nfrom events.WorkflowCreatedEvent import WorkflowCreatedEvent\n",
        )
        .unwrap();

        let rule = PortIsolationRule;
        let mut report = EnhancedValidationReport::default();

        rule.validate(&ctx, &mut report).unwrap();

        assert_eq!(report.errors.len(), 0);
    }

    // ========================================================================
    // VSA030: APPLICATION ISOLATION TESTS
    // ========================================================================

    #[test]
    fn test_vsa030_application_imports_infrastructure() {
        let (_temp_dir, ctx) = create_test_context();
        let context_path = create_context_structure(ctx.root.as_path(), "workflows");

        // Create application file that imports from infrastructure
        let app_file = context_path.join("application/WorkflowOrchestrator.py");
        std::fs::write(
            &app_file,
            "from infrastructure.PostgresWorkflowRepository import PostgresWorkflowRepository\n",
        )
        .unwrap();

        let rule = ApplicationIsolationRule;
        let mut report = EnhancedValidationReport::default();

        rule.validate(&ctx, &mut report).unwrap();

        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.errors[0].code, "VSA030");
    }

    #[test]
    fn test_vsa030_application_imports_ports_allowed() {
        let (_temp_dir, ctx) = create_test_context();
        let context_path = create_context_structure(ctx.root.as_path(), "workflows");

        // Create application file that imports from ports (allowed)
        let app_file = context_path.join("application/WorkflowOrchestrator.py");
        std::fs::write(
            &app_file,
            "from ports.WorkflowRepositoryPort import WorkflowRepositoryPort\nfrom domain.WorkflowAggregate import WorkflowAggregate\n",
        )
        .unwrap();

        let rule = ApplicationIsolationRule;
        let mut report = EnhancedValidationReport::default();

        rule.validate(&ctx, &mut report).unwrap();

        assert_eq!(report.errors.len(), 0);
    }

    // ========================================================================
    // VSA031: SLICE ISOLATION TESTS
    // ========================================================================

    #[test]
    fn test_vsa031_slice_imports_other_slice() {
        let (_temp_dir, ctx) = create_test_context();
        let context_path = create_context_structure(ctx.root.as_path(), "workflows");

        // Create slice file that imports from another slice
        let slice1_file = context_path.join("slices/slice1/handler.py");
        std::fs::write(&slice1_file, "from slices.slice2.handler import Slice2Handler\n").unwrap();

        let rule = SliceIsolationRule;
        let mut report = EnhancedValidationReport::default();

        rule.validate(&ctx, &mut report).unwrap();

        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.errors[0].code, "VSA031");
        assert!(report.errors[0].message.contains("Cross-slice imports are forbidden"));
    }

    #[test]
    fn test_vsa031_slice_imports_domain_allowed() {
        let (_temp_dir, ctx) = create_test_context();
        let context_path = create_context_structure(ctx.root.as_path(), "workflows");

        // Create slice file that imports from domain (allowed)
        let slice1_file = context_path.join("slices/slice1/handler.py");
        std::fs::write(
            &slice1_file,
            "from domain.WorkflowAggregate import WorkflowAggregate\nfrom events.WorkflowCreatedEvent import WorkflowCreatedEvent\n",
        )
        .unwrap();

        let rule = SliceIsolationRule;
        let mut report = EnhancedValidationReport::default();

        rule.validate(&ctx, &mut report).unwrap();

        assert_eq!(report.errors.len(), 0);
    }

    #[test]
    fn test_vsa031_test_files_skipped_in_cross_slice_check() {
        let (_temp_dir, ctx) = create_test_context();
        let context_path = create_context_structure(ctx.root.as_path(), "workflows");

        // Test file imports from another slice — should NOT be flagged
        let test_file = context_path.join("slices/slice1/test_integration.py");
        std::fs::write(
            &test_file,
            "from slices.slice2.handler import Slice2Handler\n",
        )
        .unwrap();

        let rule = SliceIsolationRule;
        let mut report = EnhancedValidationReport::default();

        rule.validate(&ctx, &mut report).unwrap();

        assert_eq!(report.errors.len(), 0, "Test files should be skipped in cross-slice checks");
    }

    #[test]
    fn test_vsa031_conftest_skipped_in_cross_slice_check() {
        let (_temp_dir, ctx) = create_test_context();
        let context_path = create_context_structure(ctx.root.as_path(), "workflows");

        // conftest.py imports from another slice — should NOT be flagged
        let conftest_file = context_path.join("slices/slice1/conftest.py");
        std::fs::write(
            &conftest_file,
            "from slices.slice2.handler import Slice2Handler\n",
        )
        .unwrap();

        let rule = SliceIsolationRule;
        let mut report = EnhancedValidationReport::default();

        rule.validate(&ctx, &mut report).unwrap();

        assert_eq!(report.errors.len(), 0, "conftest.py should be skipped in cross-slice checks");
    }

    #[test]
    fn test_vsa031_conftest_as_target_not_flagged() {
        let (_temp_dir, ctx) = create_test_context();
        let context_path = create_context_structure(ctx.root.as_path(), "workflows");

        // Source file imports from slices.conftest — "conftest" is not a real slice
        let handler_file = context_path.join("slices/slice1/handler.py");
        std::fs::write(
            &handler_file,
            "from slices.conftest import some_fixture\n",
        )
        .unwrap();

        let rule = SliceIsolationRule;
        let mut report = EnhancedValidationReport::default();

        rule.validate(&ctx, &mut report).unwrap();

        assert_eq!(
            report.errors.len(),
            0,
            "Import from slices.conftest should not be flagged — 'conftest' is not a real slice"
        );
    }

    #[test]
    fn test_vsa031_excluded_slices_not_flagged() {
        let temp_dir = TempDir::new().unwrap();
        let mut config = VsaConfig {
            version: 2,
            architecture: crate::config::ArchitectureType::HexagonalEventSourcedVsa,
            root: temp_dir.path().to_path_buf(),
            language: "python".to_string(),
            domain: Some(crate::config::DomainConfig::default()),
            slices: Some(crate::config::SlicesConfig::default()),
            infrastructure: Some(crate::config::InfrastructureConfig::default()),
            framework: None,
            contexts: HashMap::new(),
            validation: crate::config::ValidationConfig::default(),
            patterns: crate::config::PatternsConfig::default(),
            projection_allowed_prefixes: None,
            cross_context_scan_paths: Vec::new(),
            exceptions: Vec::new(),
        };
        config.validation.exclude_from_isolation =
            vec!["list_repos".to_string()];
        let ctx = ValidationContext::new(config, temp_dir.path().to_path_buf());

        // Create context with slices including the excluded one
        let context_path = temp_dir.path().join("org");
        std::fs::create_dir_all(context_path.join("domain")).unwrap();
        std::fs::create_dir_all(context_path.join("slices/get_repo_detail")).unwrap();
        std::fs::create_dir_all(context_path.join("slices/list_repos")).unwrap();

        // get_repo_detail imports from list_repos (excluded) — should NOT be flagged
        let handler_file = context_path.join("slices/get_repo_detail/handler.py");
        std::fs::write(
            &handler_file,
            "from slices.list_repos.projection import RepoListProjection\n",
        )
        .unwrap();

        let rule = SliceIsolationRule;
        let mut report = EnhancedValidationReport::default();

        rule.validate(&ctx, &mut report).unwrap();

        assert_eq!(
            report.errors.len(),
            0,
            "Imports from excluded slices should not be flagged"
        );
    }

    #[test]
    fn test_vsa031_real_cross_slice_still_caught() {
        let temp_dir = TempDir::new().unwrap();
        let mut config = VsaConfig {
            version: 2,
            architecture: crate::config::ArchitectureType::HexagonalEventSourcedVsa,
            root: temp_dir.path().to_path_buf(),
            language: "python".to_string(),
            domain: Some(crate::config::DomainConfig::default()),
            slices: Some(crate::config::SlicesConfig::default()),
            infrastructure: Some(crate::config::InfrastructureConfig::default()),
            framework: None,
            contexts: HashMap::new(),
            validation: crate::config::ValidationConfig::default(),
            patterns: crate::config::PatternsConfig::default(),
            projection_allowed_prefixes: None,
            cross_context_scan_paths: Vec::new(),
            exceptions: Vec::new(),
        };
        // Exclude list_repos but NOT other_slice
        config.validation.exclude_from_isolation =
            vec!["list_repos".to_string()];
        let ctx = ValidationContext::new(config, temp_dir.path().to_path_buf());

        let context_path = temp_dir.path().join("org");
        std::fs::create_dir_all(context_path.join("domain")).unwrap();
        std::fs::create_dir_all(context_path.join("slices/slice_a")).unwrap();
        std::fs::create_dir_all(context_path.join("slices/slice_b")).unwrap();

        // slice_a imports from slice_b (NOT excluded) — SHOULD be flagged
        let handler_file = context_path.join("slices/slice_a/handler.py");
        std::fs::write(
            &handler_file,
            "from slices.slice_b.handler import SliceBHandler\n",
        )
        .unwrap();

        let rule = SliceIsolationRule;
        let mut report = EnhancedValidationReport::default();

        rule.validate(&ctx, &mut report).unwrap();

        assert_eq!(
            report.errors.len(),
            1,
            "Real cross-slice violations (non-excluded) must still be caught"
        );
        assert_eq!(report.errors[0].code, "VSA031");
    }

    #[test]
    fn test_is_test_or_conftest() {
        // Test files
        assert!(is_test_or_conftest(Path::new("test_something.py")));
        assert!(is_test_or_conftest(Path::new("dir/test_integration.py")));
        assert!(is_test_or_conftest(Path::new("something_test.py")));
        assert!(is_test_or_conftest(Path::new("something_test.rs")));
        assert!(is_test_or_conftest(Path::new("Component.test.ts")));
        assert!(is_test_or_conftest(Path::new("Component.test.tsx")));
        assert!(is_test_or_conftest(Path::new("Component.spec.ts")));
        assert!(is_test_or_conftest(Path::new("Component.spec.tsx")));
        assert!(is_test_or_conftest(Path::new("conftest.py")));

        // Non-test files
        assert!(!is_test_or_conftest(Path::new("handler.py")));
        assert!(!is_test_or_conftest(Path::new("projection.py")));
        assert!(!is_test_or_conftest(Path::new("utils.py")));
        // test_utils.py starts with test_ — correctly classified as test infrastructure
        assert!(is_test_or_conftest(Path::new("test_utils.py")));
    }

    #[test]
    fn test_extract_slice_name_conftest() {
        // extract_slice_name returns "conftest" for slices.conftest imports
        // but our validation now checks if target is a real slice directory
        let result = SliceIsolationRule::extract_slice_name("slices.conftest");
        assert_eq!(result, Some("conftest".to_string()));
    }
}
