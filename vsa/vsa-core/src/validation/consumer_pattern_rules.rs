//! Consumer pattern validation rules for event sourcing
//!
//! These rules enforce the correct use of event consumer patterns:
//! - Projections must be pure (no side-effecting imports)
//! - ProcessManagers must implement required methods
//!
//! See CONSUMER-PATTERNS.md and ADR-025 for the architectural rationale.

use super::import_parser::parse_imports;
use super::{
    is_test_or_conftest, EnhancedValidationReport, Severity, Suggestion, ValidationContext,
    ValidationIssue, ValidationRule,
};
use crate::error::Result;
use crate::scanner::Scanner;
use std::collections::HashSet;

// ============================================================================
// Projection Purity Whitelist
// ============================================================================

/// Default allowed module prefixes for projections.
/// Matches event_sourcing.fitness.projection_purity.PROJECTION_ALLOWED_PREFIXES.
fn default_projection_allowed_prefixes() -> HashSet<&'static str> {
    [
        // Python stdlib (pure, no side effects)
        "__future__",
        "abc",
        "collections",
        "dataclasses",
        "datetime",
        "decimal",
        "enum",
        "functools",
        "logging",
        "math",
        "operator",
        "re",
        "typing",
        "typing_extensions",
        "uuid",
        // ESP framework itself
        "event_sourcing",
    ]
    .into_iter()
    .collect()
}

/// Check if a module name matches any allowed prefix.
fn is_allowed_import(module: &str, allowed: &HashSet<&str>) -> bool {
    for prefix in allowed {
        if module == *prefix || module.starts_with(&format!("{}.", prefix)) {
            return true;
        }
    }
    // Relative imports are always allowed (same-package)
    module.starts_with('.')
}

// ============================================================================
// VSA032: Projection Purity Rule
// ============================================================================

/// Rule: Projection files must only import from whitelisted modules.
///
/// Uses a whitelist approach (CSP-style default-deny). Any runtime import
/// whose top-level module is not in the allowed set is a violation.
/// Imports inside `if TYPE_CHECKING:` blocks are not checked by the
/// line-based parser, so they pass through safely.
///
/// The rule scans for Python files in `slices/` directories that have
/// "projection" in their filename or path, checking their imports against
/// the whitelist.
///
/// Projects can extend the whitelist via `vsa.yaml` configuration
/// (projection_allowed_prefixes).
pub struct ProjectionPurityRule;

impl ValidationRule for ProjectionPurityRule {
    fn name(&self) -> &str {
        "projection-purity"
    }

    fn code(&self) -> &str {
        "VSA032"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        let scanner = Scanner::new(ctx.config.clone(), ctx.root.clone());
        let contexts = scanner.scan_contexts()?;

        let mut allowed = default_projection_allowed_prefixes();

        // Add project-specific allowed prefixes from config
        if let Some(ref extra) = ctx.config.projection_allowed_prefixes {
            for prefix in extra {
                // Leak to get 'static lifetime - these live for the duration of validation
                allowed.insert(Box::leak(prefix.clone().into_boxed_str()));
            }
        }

        for context in contexts {
            let slices_path = context.path.join("slices");
            if !slices_path.exists() {
                continue;
            }

            // Find projection files in slices/
            for entry in walkdir::WalkDir::new(&slices_path)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
            {
                let file_path = entry.path();

                // Only check Python files
                if file_path.extension().and_then(|e| e.to_str()) != Some("py") {
                    continue;
                }

                // Skip test files
                if is_test_or_conftest(file_path) {
                    continue;
                }

                // Check if this is a projection file (by name or path)
                if !Self::is_projection_file(file_path) {
                    continue;
                }

                // Parse imports and check against whitelist
                let imports = match parse_imports(file_path) {
                    Ok(imports) => imports,
                    Err(_) => continue,
                };

                for import in imports {
                    if import.is_relative {
                        continue; // Relative imports are always fine
                    }

                    if !is_allowed_import(&import.module, &allowed) {
                        let relative_path =
                            file_path.strip_prefix(&ctx.root).unwrap_or(file_path);

                        report.errors.push(ValidationIssue {
                            path: file_path.to_path_buf(),
                            code: self.code().to_string(),
                            severity: Severity::Error,
                            message: format!(
                                "Projection file '{}' imports non-whitelisted module: '{}' (line {})\n\
                                 Projections must be pure - replaying events must produce zero side effects.\n\
                                 See CONSUMER-PATTERNS.md for allowed imports.",
                                relative_path.display(),
                                import.module,
                                import.line_number,
                            ),
                            suggestions: vec![
                                Suggestion::manual(
                                    "Remove this import. Projections should only use pure stdlib modules and event_sourcing.",
                                ),
                                Suggestion::manual(
                                    "If this import is needed, move the side-effecting logic to a ProcessManager instead.",
                                ),
                                Suggestion::manual(
                                    "If this module is safe (no side effects), add it to projection_allowed_prefixes in vsa.yaml.",
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

impl ProjectionPurityRule {
    /// Check if a file is a projection file based on naming conventions.
    fn is_projection_file(path: &std::path::Path) -> bool {
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();

        // Direct name match
        if file_name.contains("projection") {
            return true;
        }

        // Check parent directory name
        if let Some(parent) = path.parent() {
            let parent_name = parent
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_lowercase();
            if parent_name.contains("projection") {
                return true;
            }
        }

        false
    }
}

// ============================================================================
// VSA033: ProcessManager Structure Rule
// ============================================================================

/// Rule: ProcessManager subclasses must implement required methods.
///
/// Scans Python files that contain `ProcessManager` in their class
/// hierarchy and verifies they define:
/// - `def process_pending(self` - the processor side
/// - `def get_idempotency_key(self` - stable dedup key
///
/// Uses string scanning (consistent with existing VSA parser approach).
pub struct ProcessManagerStructureRule;

impl ValidationRule for ProcessManagerStructureRule {
    fn name(&self) -> &str {
        "process-manager-structure"
    }

    fn code(&self) -> &str {
        "VSA033"
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

            for entry in walkdir::WalkDir::new(&slices_path)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
            {
                let file_path = entry.path();

                // Only check Python files
                if file_path.extension().and_then(|e| e.to_str()) != Some("py") {
                    continue;
                }

                // Skip test files
                if is_test_or_conftest(file_path) {
                    continue;
                }

                let source = match std::fs::read_to_string(file_path) {
                    Ok(s) => s,
                    Err(_) => continue,
                };

                // Check if this file defines a ProcessManager subclass
                if !Self::is_process_manager_file(&source) {
                    continue;
                }

                let relative_path = file_path.strip_prefix(&ctx.root).unwrap_or(file_path);

                // Check for required methods
                if !source.contains("def process_pending(self") {
                    report.errors.push(ValidationIssue {
                        path: file_path.to_path_buf(),
                        code: self.code().to_string(),
                        severity: Severity::Error,
                        message: format!(
                            "ProcessManager in '{}' is missing required method 'process_pending()'\n\
                             ProcessManagers must implement process_pending() for the processor side.\n\
                             See CONSUMER-PATTERNS.md for the To-Do List pattern.",
                            relative_path.display(),
                        ),
                        suggestions: vec![Suggestion::manual(
                            "Add: async def process_pending(self) -> int: ...",
                        )],
                    });
                }

                if !source.contains("def get_idempotency_key(self") {
                    report.errors.push(ValidationIssue {
                        path: file_path.to_path_buf(),
                        code: self.code().to_string(),
                        severity: Severity::Error,
                        message: format!(
                            "ProcessManager in '{}' is missing required method 'get_idempotency_key()'\n\
                             ProcessManagers must provide a stable dedup key for each to-do item.\n\
                             See CONSUMER-PATTERNS.md.",
                            relative_path.display(),
                        ),
                        suggestions: vec![Suggestion::manual(
                            "Add: def get_idempotency_key(self, todo_item: dict[str, object]) -> str: ...",
                        )],
                    });
                }
            }
        }

        Ok(())
    }
}

impl ProcessManagerStructureRule {
    /// Check if source code defines a class that extends ProcessManager.
    fn is_process_manager_file(source: &str) -> bool {
        for line in source.lines() {
            let trimmed = line.trim();
            // Match class definitions that inherit from ProcessManager
            if trimmed.starts_with("class ")
                && trimmed.contains("ProcessManager")
                && trimmed.contains('(')
            {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_allowed_import() {
        let allowed = default_projection_allowed_prefixes();

        // Allowed
        assert!(is_allowed_import("typing", &allowed));
        assert!(is_allowed_import("typing.Optional", &allowed)); // matches "typing" prefix
        assert!(is_allowed_import("logging", &allowed));
        assert!(is_allowed_import("event_sourcing", &allowed));
        assert!(is_allowed_import("event_sourcing.core.checkpoint", &allowed));
        assert!(is_allowed_import("datetime", &allowed));
        assert!(is_allowed_import("uuid", &allowed));
        assert!(is_allowed_import(".relative_import", &allowed)); // relative always ok

        // Not allowed
        assert!(!is_allowed_import("httpx", &allowed));
        assert!(!is_allowed_import("requests", &allowed));
        assert!(!is_allowed_import("docker", &allowed));
        assert!(!is_allowed_import("boto3", &allowed));
        assert!(!is_allowed_import("subprocess", &allowed));
        assert!(!is_allowed_import("aiohttp", &allowed));
    }

    #[test]
    fn test_is_projection_file() {
        use std::path::Path;

        // Match by file name
        assert!(ProjectionPurityRule::is_projection_file(Path::new(
            "slices/orders/projection.py"
        )));
        assert!(ProjectionPurityRule::is_projection_file(Path::new(
            "slices/orders/OrderSummaryProjection.py"
        )));
        assert!(ProjectionPurityRule::is_projection_file(Path::new(
            "slices/orders/order_projection.py"
        )));

        // Match by parent directory
        assert!(ProjectionPurityRule::is_projection_file(Path::new(
            "slices/orders/projection/handler.py"
        )));

        // No match
        assert!(!ProjectionPurityRule::is_projection_file(Path::new(
            "slices/orders/handler.py"
        )));
        assert!(!ProjectionPurityRule::is_projection_file(Path::new(
            "slices/orders/aggregate.py"
        )));
    }

    #[test]
    fn test_is_process_manager_file() {
        assert!(ProcessManagerStructureRule::is_process_manager_file(
            "class WorkflowDispatchManager(ProcessManager):"
        ));
        assert!(ProcessManagerStructureRule::is_process_manager_file(
            "class MyManager(ProcessManager, SomeMixin):"
        ));
        // Not a ProcessManager
        assert!(!ProcessManagerStructureRule::is_process_manager_file(
            "class OrderProjection(CheckpointedProjection):"
        ));
        // Just mentioning ProcessManager in a comment
        assert!(!ProcessManagerStructureRule::is_process_manager_file(
            "# This should use ProcessManager instead"
        ));
    }
}
