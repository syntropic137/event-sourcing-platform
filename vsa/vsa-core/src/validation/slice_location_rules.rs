//! Slice location validation rules
//!
//! Validates that slices are located in the correct directory:
//! - VSA015: All command and query slices must be in slices/ directory

use super::{EnhancedValidationReport, Severity, Suggestion, ValidationContext, ValidationIssue};
use crate::error::Result;
use crate::scanner::Scanner;

use super::rules::ValidationRule;
use std::path::Path;

/// Reserved directory names that should not be treated as slices
/// These are infrastructure, organizational, or special-purpose directories
const RESERVED_DIRECTORY_NAMES: &[&str] = &[
    "_shared",
    "domain",
    "events",
    "ports",
    "commands",
    "queries",
    "aggregates",
    "application",
    "infrastructure",
    "tests",
    "fixtures",
    "__pycache__",
];

/// VSA015: All slices must be located in slices/ directory
///
/// This rule enforces that command and query slices are organized under
/// the slices/ directory within each bounded context, not at the root level.
/// This is consistent with Vertical Slice Architecture principles where
/// slices are first-class organizational units.
///
/// Invalid structure:
/// ```
/// contexts/sessions/
///   ├── start_session/      # ❌ Command slice at root
///   ├── complete_session/   # ❌ Command slice at root
///   └── slices/
///       └── list_sessions/  # ✅ Query slice in slices/
/// ```
///
/// Valid structure:
/// ```
/// contexts/sessions/
///   └── slices/
///       ├── start_session/      # ✅ Command slice in slices/
///       ├── complete_session/   # ✅ Command slice in slices/
///       └── list_sessions/      # ✅ Query slice in slices/
/// ```
pub struct RequireSliceLocationRule;

impl RequireSliceLocationRule {
    /// Check if a path is a potential slice directory (contains command/handler/event files)
    fn is_potential_slice(&self, path: &Path, ctx: &ValidationContext) -> bool {
        if !path.is_dir() {
            return false;
        }

        // Skip if it's the slices directory itself
        if path.file_name().and_then(|n| n.to_str()) == Some("slices") {
            return false;
        }

        // Skip reserved directories
        let dir_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => return false,
        };

        // Skip slices directory itself and other reserved directories
        if dir_name == "slices" || RESERVED_DIRECTORY_NAMES.contains(&dir_name) {
            return false;
        }

        // Check if directory contains slice-like files (Command, Event, Handler)
        let ext = ctx.config.file_extension();

        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                if let Some(file_name) = entry.file_name().to_str() {
                    // Look for Command, Event, Handler, or Projection files
                    // Note: file extension from config doesn't include the dot
                    if file_name.ends_with(&format!("Command.{ext}"))
                        || file_name.ends_with(&format!("Event.{ext}"))
                        || file_name.ends_with(&format!("Handler.{ext}"))
                        || file_name.ends_with(&format!("Projection.{ext}"))
                        || file_name == format!("handler.{ext}")
                        || file_name == format!("projection.{ext}")
                        || file_name == format!("commands.{ext}")
                    // For factory functions
                    {
                        return true;
                    }
                }
            }
        }

        false
    }
}

impl ValidationRule for RequireSliceLocationRule {
    fn name(&self) -> &str {
        "require-slice-location"
    }

    fn code(&self) -> &str {
        "VSA015"
    }

    fn validate(
        &self,
        ctx: &ValidationContext,
        report: &mut EnhancedValidationReport,
    ) -> Result<()> {
        let scanner = Scanner::new(ctx.config.clone(), ctx.root.clone());
        let contexts = scanner.scan_contexts()?;

        for context in contexts {
            // Use the scanner to find all features
            let features = scanner.scan_features(&context.path)?;

            for feature in features {
                // Check if this feature is in the slices/ directory relative to THIS context
                // Strip context path to get path relative to context root
                let relative_to_context = feature
                    .path
                    .strip_prefix(&context.path)
                    .unwrap_or(&feature.path)
                    .to_string_lossy();

                // Skip if already in slices/ directory (relative to context)
                if relative_to_context.starts_with("slices/")
                    || relative_to_context.starts_with("slices\\")
                {
                    continue;
                }

                // Skip reserved directories
                if RESERVED_DIRECTORY_NAMES.contains(&feature.name.as_str()) {
                    continue;
                }

                // Check if this directory contains slice-like files
                if self.is_potential_slice(&feature.path, ctx) {
                    // Suggest moving to slices/
                    let suggested_path = context.path.join("slices").join(&feature.name);

                    report.errors.push(ValidationIssue {
                        path: feature.path.clone(),
                        code: self.code().to_string(),
                        severity: Severity::Error,
                        message: format!(
                            "Slice '{}' in context '{}' is not located in slices/ directory. \
                             All command and query slices should be organized under slices/ \
                             (found at: {})",
                            feature.name, context.name, relative_to_context
                        ),
                        suggestions: vec![Suggestion::manual(format!(
                            "Move this slice to slices/{}/\n\
                                 Command: git mv {} {}",
                            feature.name,
                            feature.path.display(),
                            suggested_path.display()
                        ))],
                    });
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{SliceType, SlicesConfig, VsaConfig};
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
        }
    }

    #[test]
    fn test_rule_metadata() {
        let rule = RequireSliceLocationRule;
        assert_eq!(rule.name(), "require-slice-location");
        assert_eq!(rule.code(), "VSA015");
    }

    #[test]
    fn test_vsa015_slice_at_root_level() {
        // Test that slices at context root level are detected and reported
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        // Create context (at root level) with misplaced slice
        let context_path = root.join("orders");
        let slice_path = context_path.join("create_order");
        fs::create_dir_all(&slice_path).unwrap();

        // Create slice files that should trigger detection
        fs::write(slice_path.join("CreateOrderCommand.ts"), "export class CreateOrderCommand {}")
            .unwrap();
        fs::write(slice_path.join("CreateOrderHandler.ts"), "export class CreateOrderHandler {}")
            .unwrap();
        fs::write(slice_path.join("OrderCreatedEvent.ts"), "export class OrderCreatedEvent {}")
            .unwrap();

        // Run validation
        let config = create_test_config(root.clone(), "typescript");
        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        let rule = RequireSliceLocationRule;
        rule.validate(&ctx, &mut report).unwrap();

        // Should report 1 error for misplaced slice
        assert_eq!(report.errors.len(), 1, "Expected 1 error, got: {:?}", report.errors);
        assert_eq!(report.errors[0].code, "VSA015");
        assert!(report.errors[0].message.contains("create_order"));
        assert!(report.errors[0].message.contains("slices/"));

        // Check suggestion action contains git mv command
        match &report.errors[0].suggestions[0].action {
            crate::validation::SuggestionAction::Manual { instructions } => {
                assert!(
                    instructions.contains("git mv"),
                    "Expected 'git mv' in instructions, got: {instructions}"
                );
            }
            _ => panic!("Expected Manual suggestion action"),
        }
    }

    #[test]
    fn test_vsa015_slice_in_slices_directory() {
        // Test that slices properly located in slices/ directory pass validation
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        // Create context (at root level) with slice in proper location
        let context_path = root.join("orders");
        let slices_path = context_path.join("slices");
        let slice_path = slices_path.join("create_order");
        fs::create_dir_all(&slice_path).unwrap();

        // Create slice files
        fs::write(slice_path.join("CreateOrderCommand.ts"), "export class CreateOrderCommand {}")
            .unwrap();
        fs::write(slice_path.join("CreateOrderHandler.ts"), "export class CreateOrderHandler {}")
            .unwrap();

        // Run validation
        let config = create_test_config(root.clone(), "typescript");
        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        let rule = RequireSliceLocationRule;
        rule.validate(&ctx, &mut report).unwrap();

        // Should have no errors - compliant structure
        assert_eq!(
            report.errors.len(),
            0,
            "Expected no errors for compliant structure, but got: {:?}",
            report.errors
        );
    }

    #[test]
    fn test_vsa015_reserved_directories_ignored() {
        // Test that reserved directories are not flagged as misplaced slices
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        let context_path = root.join("orders");

        // Create all reserved directories with slice-like files
        let reserved_dirs = ["_shared", "domain", "ports", "tests", "fixtures", "__pycache__"];

        for dir_name in &reserved_dirs {
            let dir_path = context_path.join(dir_name);
            fs::create_dir_all(&dir_path).unwrap();

            // Add slice-like files that should NOT trigger detection
            fs::write(dir_path.join("SomeCommand.ts"), "export class SomeCommand {}").unwrap();
            fs::write(dir_path.join("SomeHandler.ts"), "export class SomeHandler {}").unwrap();
        }

        // Run validation
        let config = create_test_config(root.clone(), "typescript");
        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        let rule = RequireSliceLocationRule;
        rule.validate(&ctx, &mut report).unwrap();

        // Should have no errors - reserved directories are ignored
        assert_eq!(report.errors.len(), 0);
    }

    #[test]
    fn test_vsa015_non_slice_directory_ignored() {
        // Test that directories without slice files are not flagged
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        let context_path = root.join("orders");

        // Create regular directories without slice files
        let utils_path = context_path.join("utils");
        fs::create_dir_all(&utils_path).unwrap();
        fs::write(utils_path.join("helpers.ts"), "export function helper() {}").unwrap();
        fs::write(utils_path.join("constants.ts"), "export const MAX = 100;").unwrap();

        let config_path = context_path.join("config");
        fs::create_dir_all(&config_path).unwrap();
        fs::write(config_path.join("settings.ts"), "export const settings = {};").unwrap();

        // Run validation
        let config = create_test_config(root.clone(), "typescript");
        let ctx = ValidationContext::new(config, root);
        let mut report = EnhancedValidationReport::default();

        let rule = RequireSliceLocationRule;
        rule.validate(&ctx, &mut report).unwrap();

        // Should have no errors - not detected as slices
        assert_eq!(report.errors.len(), 0);
    }

    #[test]
    fn test_vsa015_multiple_file_extensions() {
        // Test detection works for TypeScript, Python, and Rust files
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        // Test TypeScript
        let ts_context = root.join("typescript_ctx");
        let ts_slice = ts_context.join("create_item");
        fs::create_dir_all(&ts_slice).unwrap();
        fs::write(ts_slice.join("CreateItemCommand.ts"), "export class CreateItemCommand {}")
            .unwrap();

        let config = create_test_config(root.clone(), "typescript");
        let ctx = ValidationContext::new(config, root.clone());
        let mut report = EnhancedValidationReport::default();

        let rule = RequireSliceLocationRule;
        rule.validate(&ctx, &mut report).unwrap();
        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.errors[0].code, "VSA015");

        // Test Python
        let py_root = TempDir::new().unwrap().path().to_path_buf();
        let py_context = py_root.join("python_ctx");
        let py_slice = py_context.join("create_item");
        fs::create_dir_all(&py_slice).unwrap();
        fs::write(py_slice.join("CreateItemCommand.py"), "class CreateItemCommand: pass").unwrap();

        let py_config = create_test_config(py_root.clone(), "python");
        let py_ctx = ValidationContext::new(py_config, py_root.clone());
        let mut py_report = EnhancedValidationReport::default();

        rule.validate(&py_ctx, &mut py_report).unwrap();
        assert_eq!(py_report.errors.len(), 1);
        assert_eq!(py_report.errors[0].code, "VSA015");

        // Test Rust
        let rs_root = TempDir::new().unwrap().path().to_path_buf();
        let rs_context = rs_root.join("rust_ctx");
        let rs_slice = rs_context.join("create_item");
        fs::create_dir_all(&rs_slice).unwrap();
        fs::write(rs_slice.join("CreateItemCommand.rs"), "pub struct CreateItemCommand {}")
            .unwrap();

        let rs_config = create_test_config(rs_root.clone(), "rust");
        let rs_ctx = ValidationContext::new(rs_config, rs_root.clone());
        let mut rs_report = EnhancedValidationReport::default();

        rule.validate(&rs_ctx, &mut rs_report).unwrap();
        assert_eq!(rs_report.errors.len(), 1);
        assert_eq!(rs_report.errors[0].code, "VSA015");
    }
}
