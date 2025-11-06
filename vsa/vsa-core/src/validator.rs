//! Validation logic for VSA structure

use std::path::PathBuf;

use crate::config::VsaConfig;
use crate::error::Result;
use crate::patterns::PatternMatcher;
use crate::scanner::{ContextInfo, FeatureInfo, Scanner};

/// Validator for VSA structure
#[derive(Debug)]
pub struct Validator {
    config: VsaConfig,
    scanner: Scanner,
    pattern_matcher: PatternMatcher,
}

impl Validator {
    /// Create a new validator
    pub fn new(config: VsaConfig, root: PathBuf) -> Self {
        let extension = config.file_extension().to_string();
        let patterns = config.patterns.clone();
        let pattern_matcher = PatternMatcher::new(patterns, extension);
        let scanner = Scanner::new(config.clone(), root);

        Self { config, scanner, pattern_matcher }
    }

    /// Validate the entire structure
    pub fn validate(&self) -> Result<ValidationReport> {
        let mut report = ValidationReport::default();

        // Scan contexts
        let contexts = self.scanner.scan_contexts()?;

        for context in &contexts {
            if let Err(e) = self.validate_context(context, &mut report) {
                report
                    .errors
                    .push(ValidationError { path: context.path.clone(), message: e.to_string() });
            }
        }

        Ok(report)
    }

    fn validate_context(&self, context: &ContextInfo, report: &mut ValidationReport) -> Result<()> {
        // Scan features in context
        let features = self.scanner.scan_features(&context.path)?;

        for feature in &features {
            self.validate_feature(context, feature, report)?;
        }

        Ok(())
    }

    fn validate_feature(
        &self,
        context: &ContextInfo,
        feature: &FeatureInfo,
        report: &mut ValidationReport,
    ) -> Result<()> {
        let files = self.scanner.scan_feature_files(&feature.path)?;

        let mut has_command = false;
        let mut has_handler = false;
        let mut has_test = false;

        for file in &files {
            if self.pattern_matcher.is_command(&file.path) {
                has_command = true;
            } else if self.pattern_matcher.is_handler(&file.path) {
                has_handler = true;
            } else if self.pattern_matcher.is_test(&file.path) {
                has_test = true;
            }
        }

        // Validate command features
        if has_command && !has_handler {
            report.warnings.push(ValidationWarning {
                path: feature.path.clone(),
                message: format!(
                    "Feature '{}' in context '{}' has a command but no handler",
                    feature.name, context.name
                ),
            });
        }

        // Validate tests
        if self.config.validation.require_tests && (has_command || has_handler) && !has_test {
            report.warnings.push(ValidationWarning {
                path: feature.path.clone(),
                message: format!(
                    "Feature '{}' in context '{}' is missing tests",
                    feature.name, context.name
                ),
            });
        }

        Ok(())
    }
}

/// Validation report
#[derive(Debug, Default)]
pub struct ValidationReport {
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationWarning>,
}

impl ValidationReport {
    /// Check if validation passed (no errors)
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    /// Get total issue count
    pub fn total_issues(&self) -> usize {
        self.errors.len() + self.warnings.len()
    }
}

/// Validation error
#[derive(Debug)]
pub struct ValidationError {
    pub path: PathBuf,
    pub message: String,
}

/// Validation warning
#[derive(Debug)]
pub struct ValidationWarning {
    pub path: PathBuf,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{PatternsConfig, ValidationConfig};
    use std::collections::HashMap;
    use tempfile::TempDir;

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
    fn test_validate_empty_structure() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        let config = create_test_config(root.clone());
        let validator = Validator::new(config, root);

        let report = validator.validate().unwrap();
        assert!(report.is_valid());
    }
}
