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
    EnhancedValidationReport, Severity, Suggestion, ValidationContext, ValidationIssue,
    ValidationRule,
};
use crate::error::Result;
use crate::scanner::Scanner;
use std::path::PathBuf;

// ============================================================================
// VSA027: Domain Purity Rule
// ============================================================================

/// Rule: Domain layer must not import from events, ports, application, infrastructure, or slices
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
        // NOTE: domain/events/ is ALLOWED (events are part of domain in ADR-019 v2)
        // Only forbid events/ at context root (legacy structure)
        let has_forbidden_events = normalized.contains("/events/") && !normalized.contains("/domain/events/")
            || normalized.starts_with("events/") && !normalized.starts_with("domain/events/")
            || (normalized.starts_with("events") && !normalized.starts_with("domain/events") && !normalized.contains('/'));

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
            let domain_config = ctx.config.domain.as_ref().ok_or_else(|| {
                crate::error::VsaError::InvalidConfig(
                    "Domain configuration required for events isolation rule".to_string(),
                )
            })?;
            let events_path = context.path.join(&domain_config.path).join(&domain_config.events.path);
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

            // Check each slice
            for slice_dir in &slice_dirs {
                let slice_name = slice_dir.file_name().unwrap().to_string_lossy();

                // Scan all files in this slice recursively
                for entry in walkdir::WalkDir::new(slice_dir)
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

                    // Check each import for cross-slice dependencies
                    for import in imports {
                        // Check if import targets another slice
                        if let Some(target_slice) = Self::extract_slice_name(&import.module) {
                            if target_slice != slice_name {
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
    fn test_vsa028_events_import_domain() {
        let (_temp_dir, ctx) = create_test_context();
        let context_path = create_context_structure(ctx.root.as_path(), "workflows");

        // Create event file that imports from domain
        let event_file = context_path.join("events/WorkflowCreatedEvent.py");
        std::fs::write(&event_file, "from domain.WorkflowAggregate import WorkflowAggregate\n")
            .unwrap();

        let rule = EventsIsolationRule;
        let mut report = EnhancedValidationReport::default();

        rule.validate(&ctx, &mut report).unwrap();

        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.errors[0].code, "VSA028");
        assert!(report.errors[0].message.contains("pure data structures"));
    }

    #[test]
    fn test_vsa028_events_stdlib_allowed() {
        let (_temp_dir, ctx) = create_test_context();
        let context_path = create_context_structure(ctx.root.as_path(), "workflows");

        // Create event file with only stdlib imports
        let event_file = context_path.join("events/WorkflowCreatedEvent.py");
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
}
