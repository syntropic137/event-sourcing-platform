//! Slice location validation rules
//!
//! Validates that slices are located in the correct directory:
//! - VSA015: All command and query slices must be in slices/ directory

use super::{EnhancedValidationReport, Severity, Suggestion, ValidationContext, ValidationIssue};
use crate::error::Result;
use crate::scanner::Scanner;

use super::rules::ValidationRule;
use std::path::Path;

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

        if matches!(
            dir_name,
            "_shared" | "domain" | "ports" | "slices" | "tests" | "fixtures"
        ) {
            return false;
        }

        // Check if directory contains slice-like files (Command, Event, Handler)
        let ext = ctx.config.file_extension();

        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                if let Some(file_name) = entry.file_name().to_str() {
                    // Look for Command, Event, Handler, or Projection files
                    // Note: file extension from config doesn't include the dot
                    if file_name.ends_with(&format!("Command.{}", ext))
                        || file_name.ends_with(&format!("Event.{}", ext))
                        || file_name.ends_with(&format!("Handler.{}", ext))
                        || file_name.ends_with(&format!("Projection.{}", ext))
                        || file_name == format!("handler.{}", ext)
                        || file_name == format!("projection.{}", ext)
                        || file_name == format!("commands.{}", ext) // For factory functions
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
                // Check if this feature is NOT in the slices/ directory
                let relative_path_str = feature.relative_path.to_string_lossy();

                // Skip if already in slices/ directory
                if relative_path_str.starts_with("slices/") || relative_path_str.starts_with("slices\\") {
                    continue;
                }

                // Skip reserved directories
                if matches!(
                    feature.name.as_str(),
                    "_shared" | "domain" | "ports" | "tests" | "fixtures" | "__pycache__"
                ) {
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
                            feature.name, context.name, relative_path_str
                        ),
                        suggestions: vec![Suggestion::manual(
                            format!(
                                "Move this slice to slices/{}/\n\
                                 Command: git mv {} {}",
                                feature.name,
                                feature.path.display(),
                                suggested_path.display()
                            )
                        )],
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
    use std::path::PathBuf;

    #[test]
    fn test_rule_metadata() {
        let rule = RequireSliceLocationRule;
        assert_eq!(rule.name(), "require-slice-location");
        assert_eq!(rule.code(), "VSA015");
    }
}
