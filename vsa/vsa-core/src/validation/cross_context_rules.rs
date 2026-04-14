//! Cross-context boundary validation rules
//!
//! VSA204: Cross-context imports must go through public API (root or ports)
//! VSA205: Each context must have a public API (__init__.py with exports)

use super::{
    is_test_or_conftest, EnhancedValidationReport, Severity, Suggestion, ValidationContext,
    ValidationIssue, ValidationRule,
};
use crate::error::Result;
use crate::scanner::Scanner;
use crate::validation::import_parser::{ImportParser, PythonImportParser};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

// ============================================================================
// VSA205: Context Public API Exists
// ============================================================================

/// Rule: Each bounded context must have a public API (__init__.py with exports)
pub struct ContextPublicApiExistsRule;

impl ValidationRule for ContextPublicApiExistsRule {
    fn name(&self) -> &str {
        "context-public-api-exists"
    }

    fn code(&self) -> &str {
        "VSA205"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        let scanner = Scanner::new(ctx.config.clone(), ctx.root.clone());
        let contexts = scanner.scan_contexts()?;

        for context in &contexts {
            let init_path = context.path.join("__init__.py");

            if !init_path.exists() {
                report.errors.push(ValidationIssue {
                    path: context.path.clone(),
                    code: self.code().to_string(),
                    severity: Severity::Error,
                    message: format!(
                        "Context '{}' is missing __init__.py public API",
                        context.name
                    ),
                    suggestions: vec![Suggestion::create_file(
                        init_path,
                        format!(
                            "Create __init__.py with public exports for context '{}'",
                            context.name
                        ),
                    )],
                });
                continue;
            }

            // Check that __init__.py has at least one export
            let content = std::fs::read_to_string(&init_path).unwrap_or_default();
            let has_exports = content.lines().any(|line| {
                let trimmed = line.trim();
                trimmed.starts_with("from ") && trimmed.contains(" import ")
                    || trimmed.starts_with("__all__")
            });

            if !has_exports {
                report.errors.push(ValidationIssue {
                    path: init_path.clone(),
                    code: self.code().to_string(),
                    severity: Severity::Error,
                    message: format!(
                        "Context '{}' __init__.py has no exports (needs 'from ... import' or '__all__')",
                        context.name
                    ),
                    suggestions: vec![Suggestion::manual(
                        "Add public API exports to __init__.py (e.g., 'from .domain import ...' or '__all__ = [...]')"
                    )],
                });
            }
        }

        Ok(())
    }
}

// ============================================================================
// VSA204: Cross-Context Public API Rule
// ============================================================================

/// Rule: Cross-context imports must go through the public API (root or ports)
pub struct CrossContextPublicApiRule;

impl ValidationRule for CrossContextPublicApiRule {
    fn name(&self) -> &str {
        "cross-context-public-api"
    }

    fn code(&self) -> &str {
        "VSA204"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        let scanner = Scanner::new(ctx.config.clone(), ctx.root.clone());
        let contexts = scanner.scan_contexts()?;
        let context_names: HashSet<String> =
            contexts.iter().map(|c| c.name.clone()).collect();

        let parser = PythonImportParser::new();

        // Walk each context's files
        for context in &contexts {
            let py_files = collect_py_files(&context.path);

            for file_path in &py_files {
                if should_skip_file(file_path, &context.path) {
                    continue;
                }

                let imports = match parser.parse_file(file_path) {
                    Ok(imports) => imports,
                    Err(_) => continue,
                };

                let violations: Vec<String> = imports
                    .iter()
                    .filter(|imp| {
                        is_deep_cross_context_import(
                            &imp.module,
                            &imp.line,
                            Some(&context.name),
                            &context_names,
                        )
                    })
                    .map(|imp| imp.line.trim().to_string())
                    .collect();

                report_violations(
                    ctx,
                    report,
                    file_path,
                    &violations,
                    "VSA204",
                );
            }
        }

        // Walk cross_context_scan_paths (no "own" context)
        for scan_path in &ctx.config.cross_context_scan_paths {
            let resolved = if scan_path.is_absolute() {
                scan_path.clone()
            } else {
                ctx.root.join(scan_path)
            };

            if !resolved.exists() {
                continue;
            }

            let py_files = collect_py_files(&resolved);

            for file_path in &py_files {
                if is_test_or_conftest(file_path) {
                    continue;
                }
                if is_conftest_or_init(file_path) {
                    continue;
                }

                let imports = match parser.parse_file(file_path) {
                    Ok(imports) => imports,
                    Err(_) => continue,
                };

                let violations: Vec<String> = imports
                    .iter()
                    .filter(|imp| {
                        is_deep_cross_context_import(
                            &imp.module,
                            &imp.line,
                            None, // no own context
                            &context_names,
                        )
                    })
                    .map(|imp| imp.line.trim().to_string())
                    .collect();

                report_violations(
                    ctx,
                    report,
                    file_path,
                    &violations,
                    "VSA204",
                );
            }
        }

        Ok(())
    }
}

// ============================================================================
// Helper functions
// ============================================================================

/// Collect all .py files under a directory, recursively
fn collect_py_files(root: &Path) -> Vec<PathBuf> {
    WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            e.path()
                .extension()
                .map_or(false, |ext| ext == "py")
        })
        .map(|e| e.path().to_path_buf())
        .collect()
}

/// Check if a file should be skipped for cross-context analysis
fn should_skip_file(path: &Path, _context_root: &Path) -> bool {
    // Skip test files
    if is_test_or_conftest(path) {
        return true;
    }

    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    // Skip __init__.py and conftest.py
    if file_name == "__init__.py" || file_name == "conftest.py" {
        return true;
    }

    // Skip files under _shared/ directories
    for component in path.components() {
        if let std::path::Component::Normal(name) = component {
            if name.to_string_lossy() == "_shared" {
                return true;
            }
        }
    }

    false
}

/// Check if file is conftest.py or __init__.py
fn is_conftest_or_init(path: &Path) -> bool {
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    file_name == "__init__.py" || file_name == "conftest.py"
}

/// Check if an import is a deep cross-context import that violates boundaries.
///
/// A deep import is one that reaches into a foreign context's internals
/// (e.g., `contexts.github.slices.foo`). Allowed imports are:
/// - `contexts.<foreign>` (root import)
/// - `contexts.<foreign>.ports` (ports import)
/// - Any import containing "Projection" (heuristic exemption)
fn is_deep_cross_context_import(
    module: &str,
    line: &str,
    own_context: Option<&str>,
    context_names: &HashSet<String>,
) -> bool {
    // Check for Projection heuristic exemption
    if line.contains("Projection") {
        return false;
    }

    // Find `contexts.<name>.` pattern in the module path
    let parts: Vec<&str> = module.split('.').collect();

    for i in 0..parts.len() {
        if parts[i] != "contexts" {
            continue;
        }

        // Need at least one more part for the context name
        if i + 1 >= parts.len() {
            continue;
        }

        let foreign_ctx = parts[i + 1];

        // Must be a known context
        if !context_names.contains(foreign_ctx) {
            continue;
        }

        // Skip if it's the file's own context
        if let Some(own) = own_context {
            if foreign_ctx == own {
                continue;
            }
        }

        // Check what follows after `contexts.<foreign>`
        if i + 2 >= parts.len() {
            // Root import: `contexts.<foreign>` - allowed
            return false;
        }

        let next_segment = parts[i + 2];
        if next_segment == "ports" {
            // Ports import - allowed
            return false;
        }

        // Any other deep path is a violation
        return true;
    }

    false
}

/// Report violations for a file, respecting exception budgets
fn report_violations(
    ctx: &ValidationContext,
    report: &mut EnhancedValidationReport,
    file_path: &Path,
    violations: &[String],
    rule_code: &str,
) {
    if violations.is_empty() {
        return;
    }

    // Check exception budget for this file+rule
    let file_str = file_path.to_string_lossy();
    let budget = ctx
        .config
        .exceptions
        .iter()
        .filter(|e| e.rule == rule_code && file_str.ends_with(&e.file))
        .map(|e| e.budget)
        .sum::<usize>();

    let excess = if violations.len() > budget {
        violations.len() - budget
    } else {
        0
    };

    if excess > 0 {
        // Report the excess violations
        let violation_lines = violations
            .iter()
            .skip(budget)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n  ");

        report.errors.push(ValidationIssue {
            path: file_path.to_path_buf(),
            code: rule_code.to_string(),
            severity: Severity::Error,
            message: format!(
                "{} cross-context deep import violation(s) ({} total, {} budgeted):\n  {}",
                excess,
                violations.len(),
                budget,
                violation_lines
            ),
            suggestions: vec![Suggestion::manual(
                "Import from the context's public API (contexts.<ctx> or contexts.<ctx>.ports) instead of reaching into internals"
            )],
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{PatternsConfig, ValidationConfig, VsaConfig, ExceptionBudget};
    use crate::validation::{EnhancedValidationReport, ValidationContext};
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn create_test_config(root: PathBuf) -> VsaConfig {
        VsaConfig {
            version: 1,
            architecture: crate::config::ArchitectureType::default(),
            root: root.clone(),
            language: "python".to_string(),
            domain: None,
            slices: None,
            infrastructure: None,
            framework: None,
            contexts: HashMap::new(),
            validation: ValidationConfig::default(),
            patterns: PatternsConfig::default(),
            projection_allowed_prefixes: None,
            cross_context_scan_paths: Vec::new(),
            exceptions: Vec::new(),
        }
    }

    fn setup_context_structure(temp_dir: &TempDir) -> PathBuf {
        let root = temp_dir.path().to_path_buf();

        // Create two contexts: github and orchestration
        std::fs::create_dir_all(root.join("github")).unwrap();
        std::fs::create_dir_all(root.join("github/domain")).unwrap();
        std::fs::create_dir_all(root.join("github/slices")).unwrap();
        std::fs::create_dir_all(root.join("github/ports")).unwrap();

        std::fs::create_dir_all(root.join("orchestration")).unwrap();
        std::fs::create_dir_all(root.join("orchestration/domain")).unwrap();
        std::fs::create_dir_all(root.join("orchestration/slices")).unwrap();
        std::fs::create_dir_all(root.join("orchestration/ports")).unwrap();

        root
    }

    // ========================================================================
    // VSA205: Context Public API Exists
    // ========================================================================

    #[test]
    fn test_vsa205_missing_init_py() {
        let temp_dir = TempDir::new().unwrap();
        let root = setup_context_structure(&temp_dir);

        // No __init__.py in either context
        let config = create_test_config(root.clone());
        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        let rule = ContextPublicApiExistsRule;
        rule.validate(&ctx, &mut report).unwrap();

        // Should have 2 errors (one per context)
        assert_eq!(report.errors.len(), 2);
        assert!(report.errors.iter().all(|e| e.code == "VSA205"));
    }

    #[test]
    fn test_vsa205_empty_init_py() {
        let temp_dir = TempDir::new().unwrap();
        let root = setup_context_structure(&temp_dir);

        // Create empty __init__.py
        std::fs::write(root.join("github/__init__.py"), "").unwrap();
        std::fs::write(root.join("orchestration/__init__.py"), "# just a comment\n").unwrap();

        let config = create_test_config(root.clone());
        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        let rule = ContextPublicApiExistsRule;
        rule.validate(&ctx, &mut report).unwrap();

        // Should have 2 errors for empty exports
        assert_eq!(report.errors.len(), 2);
        assert!(report.errors.iter().all(|e| e.message.contains("no exports")));
    }

    #[test]
    fn test_vsa205_valid_init_with_from_import() {
        let temp_dir = TempDir::new().unwrap();
        let root = setup_context_structure(&temp_dir);

        std::fs::write(
            root.join("github/__init__.py"),
            "from .domain import InstallationAggregate\n",
        )
        .unwrap();
        std::fs::write(
            root.join("orchestration/__init__.py"),
            "__all__ = ['WorkspaceService']\n",
        )
        .unwrap();

        let config = create_test_config(root.clone());
        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        let rule = ContextPublicApiExistsRule;
        rule.validate(&ctx, &mut report).unwrap();

        assert_eq!(report.errors.len(), 0);
    }

    // ========================================================================
    // VSA204: Cross-Context Public API Rule
    // ========================================================================

    #[test]
    fn test_vsa204_deep_import_detected() {
        let temp_dir = TempDir::new().unwrap();
        let root = setup_context_structure(&temp_dir);

        // File in orchestration imports github internals
        std::fs::write(
            root.join("orchestration/service.py"),
            "from contexts.github.domain.InstallationAggregate import InstallationAggregate\n",
        )
        .unwrap();

        let config = create_test_config(root.clone());
        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        let rule = CrossContextPublicApiRule;
        rule.validate(&ctx, &mut report).unwrap();

        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].code == "VSA204");
    }

    #[test]
    fn test_vsa204_public_api_import_allowed() {
        let temp_dir = TempDir::new().unwrap();
        let root = setup_context_structure(&temp_dir);

        // File in orchestration imports github through public API
        std::fs::write(
            root.join("orchestration/service.py"),
            "from contexts.github import InstallationAggregate\n",
        )
        .unwrap();

        let config = create_test_config(root.clone());
        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        let rule = CrossContextPublicApiRule;
        rule.validate(&ctx, &mut report).unwrap();

        assert_eq!(report.errors.len(), 0);
    }

    #[test]
    fn test_vsa204_ports_import_allowed() {
        let temp_dir = TempDir::new().unwrap();
        let root = setup_context_structure(&temp_dir);

        // File in orchestration imports github.ports
        std::fs::write(
            root.join("orchestration/service.py"),
            "from contexts.github.ports import GitHubPort\n",
        )
        .unwrap();

        let config = create_test_config(root.clone());
        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        let rule = CrossContextPublicApiRule;
        rule.validate(&ctx, &mut report).unwrap();

        assert_eq!(report.errors.len(), 0);
    }

    #[test]
    fn test_vsa204_test_files_exempt() {
        let temp_dir = TempDir::new().unwrap();
        let root = setup_context_structure(&temp_dir);

        // Test file with deep import - should be exempt
        std::fs::write(
            root.join("orchestration/test_service.py"),
            "from contexts.github.domain.InstallationAggregate import InstallationAggregate\n",
        )
        .unwrap();

        let config = create_test_config(root.clone());
        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        let rule = CrossContextPublicApiRule;
        rule.validate(&ctx, &mut report).unwrap();

        assert_eq!(report.errors.len(), 0);
    }

    #[test]
    fn test_vsa204_shared_dir_exempt() {
        let temp_dir = TempDir::new().unwrap();
        let root = setup_context_structure(&temp_dir);

        // File under _shared with deep import - should be exempt
        std::fs::create_dir_all(root.join("orchestration/_shared")).unwrap();
        std::fs::write(
            root.join("orchestration/_shared/helpers.py"),
            "from contexts.github.domain.InstallationAggregate import InstallationAggregate\n",
        )
        .unwrap();

        let config = create_test_config(root.clone());
        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        let rule = CrossContextPublicApiRule;
        rule.validate(&ctx, &mut report).unwrap();

        assert_eq!(report.errors.len(), 0);
    }

    #[test]
    fn test_vsa204_init_py_exempt() {
        let temp_dir = TempDir::new().unwrap();
        let root = setup_context_structure(&temp_dir);

        // __init__.py with deep import - should be exempt
        std::fs::write(
            root.join("orchestration/__init__.py"),
            "from contexts.github.domain.InstallationAggregate import InstallationAggregate\n",
        )
        .unwrap();

        let config = create_test_config(root.clone());
        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        let rule = CrossContextPublicApiRule;
        rule.validate(&ctx, &mut report).unwrap();

        assert_eq!(report.errors.len(), 0);
    }

    #[test]
    fn test_vsa204_exception_budget_respected() {
        let temp_dir = TempDir::new().unwrap();
        let root = setup_context_structure(&temp_dir);

        // File with 2 deep imports
        std::fs::write(
            root.join("orchestration/service.py"),
            concat!(
                "from contexts.github.domain.InstallationAggregate import InstallationAggregate\n",
                "from contexts.github.slices.foo import bar\n",
            ),
        )
        .unwrap();

        // Budget of 2 - should suppress all violations
        let mut config = create_test_config(root.clone());
        config.exceptions = vec![ExceptionBudget {
            file: "orchestration/service.py".to_string(),
            rule: "VSA204".to_string(),
            budget: 2,
            issue: Some("#123".to_string()),
        }];

        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        let rule = CrossContextPublicApiRule;
        rule.validate(&ctx, &mut report).unwrap();

        assert_eq!(report.errors.len(), 0);
    }

    #[test]
    fn test_vsa204_exception_budget_partial() {
        let temp_dir = TempDir::new().unwrap();
        let root = setup_context_structure(&temp_dir);

        // File with 3 deep imports
        std::fs::write(
            root.join("orchestration/service.py"),
            concat!(
                "from contexts.github.domain.Agg import Agg\n",
                "from contexts.github.slices.foo import bar\n",
                "from contexts.github.domain.Other import Other\n",
            ),
        )
        .unwrap();

        // Budget of 1 - should report 2 excess
        let mut config = create_test_config(root.clone());
        config.exceptions = vec![ExceptionBudget {
            file: "orchestration/service.py".to_string(),
            rule: "VSA204".to_string(),
            budget: 1,
            issue: None,
        }];

        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        let rule = CrossContextPublicApiRule;
        rule.validate(&ctx, &mut report).unwrap();

        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].message.contains("2 cross-context"));
    }

    #[test]
    fn test_vsa204_projection_heuristic_exemption() {
        let temp_dir = TempDir::new().unwrap();
        let root = setup_context_structure(&temp_dir);

        // Import with Projection in the line - should be exempt
        std::fs::write(
            root.join("orchestration/service.py"),
            "from contexts.github.slices.some_projection.internal.GitHubProjection import GitHubProjection\n",
        )
        .unwrap();

        let config = create_test_config(root.clone());
        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        let rule = CrossContextPublicApiRule;
        rule.validate(&ctx, &mut report).unwrap();

        assert_eq!(report.errors.len(), 0);
    }

    #[test]
    fn test_vsa204_cross_context_scan_paths() {
        let temp_dir = TempDir::new().unwrap();
        // Put contexts in a subdirectory so we can have external paths alongside
        let project_root = temp_dir.path().to_path_buf();
        let contexts_root = project_root.join("contexts");
        std::fs::create_dir_all(contexts_root.join("github/domain")).unwrap();
        std::fs::create_dir_all(contexts_root.join("orchestration/domain")).unwrap();

        // Create an external scan path outside the contexts root
        let external = project_root.join("apps/api");
        std::fs::create_dir_all(&external).unwrap();
        std::fs::write(
            external.join("handler.py"),
            "from contexts.github.domain.Agg import Agg\n",
        )
        .unwrap();

        let mut config = create_test_config(contexts_root.clone());
        config.cross_context_scan_paths = vec![external.clone()];

        let ctx = ValidationContext::new(config, contexts_root);
        let mut report = EnhancedValidationReport::default();

        let rule = CrossContextPublicApiRule;
        rule.validate(&ctx, &mut report).unwrap();

        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].code == "VSA204");
    }

    #[test]
    fn test_is_deep_cross_context_import_deep() {
        let mut names = HashSet::new();
        names.insert("github".to_string());
        names.insert("orchestration".to_string());

        assert!(is_deep_cross_context_import(
            "contexts.github.domain.Agg",
            "from contexts.github.domain.Agg import Agg",
            Some("orchestration"),
            &names,
        ));
    }

    #[test]
    fn test_is_deep_cross_context_import_root_allowed() {
        let mut names = HashSet::new();
        names.insert("github".to_string());

        assert!(!is_deep_cross_context_import(
            "contexts.github",
            "from contexts.github import Agg",
            Some("orchestration"),
            &names,
        ));
    }

    #[test]
    fn test_is_deep_cross_context_import_ports_allowed() {
        let mut names = HashSet::new();
        names.insert("github".to_string());

        assert!(!is_deep_cross_context_import(
            "contexts.github.ports",
            "from contexts.github.ports import GitHubPort",
            Some("orchestration"),
            &names,
        ));
    }

    #[test]
    fn test_is_deep_cross_context_import_own_context_ignored() {
        let mut names = HashSet::new();
        names.insert("github".to_string());

        // Importing own context's internals is fine
        assert!(!is_deep_cross_context_import(
            "contexts.github.domain.Agg",
            "from contexts.github.domain.Agg import Agg",
            Some("github"),
            &names,
        ));
    }

    #[test]
    fn test_is_deep_cross_context_import_projection_exempt() {
        let mut names = HashSet::new();
        names.insert("github".to_string());

        assert!(!is_deep_cross_context_import(
            "contexts.github.slices.some_proj.internal.GitHubProjection",
            "from contexts.github.slices.some_proj.internal.GitHubProjection import GitHubProjection",
            Some("orchestration"),
            &names,
        ));
    }
}
