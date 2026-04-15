//! File system scanning

use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::config::VsaConfig;
use crate::error::Result;

/// File system scanner
#[derive(Debug)]
pub struct Scanner {
    config: VsaConfig,
    root: PathBuf,
}

impl Scanner {
    /// Create a new scanner
    pub fn new(config: VsaConfig, root: PathBuf) -> Self {
        Self { config, root }
    }

    /// Scan for all contexts
    pub fn scan_contexts(&self) -> Result<Vec<ContextInfo>> {
        let mut contexts = Vec::new();

        for entry in WalkDir::new(&self.root)
            .min_depth(1)
            .max_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_dir())
        {
            let name = entry.file_name().to_string_lossy().to_string();

            // Skip _shared directory
            if name.starts_with('_') {
                continue;
            }

            contexts.push(ContextInfo { name, path: entry.path().to_path_buf() });
        }

        Ok(contexts)
    }

    /// Scan for features within a context
    pub fn scan_features(&self, context_path: &Path) -> Result<Vec<FeatureInfo>> {
        let mut features = Vec::new();

        self.scan_features_recursive(context_path, context_path, &mut features)?;

        Ok(features)
    }

    fn scan_features_recursive(
        &self,
        base_path: &Path,
        current_path: &Path,
        features: &mut Vec<FeatureInfo>,
    ) -> Result<()> {
        // Determine domain layer path to exclude from feature scanning.
        // When a DomainConfig is present, the domain directory (e.g. "domain/")
        // contains organizational subdirectories (commands/, events/, aggregates/)
        // that are NOT vertical slice features and should not be scanned as such.
        let domain_path = self
            .config
            .domain
            .as_ref()
            .map(|d| d.path.to_string_lossy().to_string());

        for entry in WalkDir::new(current_path)
            .min_depth(1)
            .max_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_dir())
        {
            let name = entry.file_name().to_string_lossy().to_string();

            // Skip _shared and hidden directories
            if name.starts_with('_') || name.starts_with('.') {
                continue;
            }

            // Skip the domain layer directory when DomainConfig is declared.
            // The domain/ directory tree contains aggregates, commands, events,
            // and queries — these are domain model files, not feature slices.
            if let Some(ref dp) = domain_path {
                if &name == dp {
                    continue;
                }
            }

            let path = entry.path().to_path_buf();
            let relative_path = path.strip_prefix(base_path).unwrap().to_path_buf();

            features.push(FeatureInfo {
                name,
                path: path.clone(),
                relative_path: relative_path.clone(),
            });

            // Recursively scan for nested features if enabled
            if self.config.validation.allow_nested_features {
                self.scan_features_recursive(base_path, &path, features)?;
            }
        }

        Ok(())
    }

    /// Scan for files within a feature
    pub fn scan_feature_files(&self, feature_path: &Path) -> Result<Vec<FileInfo>> {
        let mut files = Vec::new();

        for entry in WalkDir::new(feature_path)
            .max_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let name = entry.file_name().to_string_lossy().to_string();
            let path = entry.path().to_path_buf();

            files.push(FileInfo { name, path });
        }

        Ok(files)
    }
}

/// Context information
#[derive(Debug, Clone)]
pub struct ContextInfo {
    pub name: String,
    pub path: PathBuf,
}

/// Feature information
#[derive(Debug, Clone)]
pub struct FeatureInfo {
    pub name: String,
    pub path: PathBuf,
    pub relative_path: PathBuf,
}

/// File information
#[derive(Debug, Clone)]
pub struct FileInfo {
    pub name: String,
    pub path: PathBuf,
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
            root,
            language: "typescript".to_string(),
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
            layer_separation: None,
        }
    }

    #[test]
    fn test_scan_contexts() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_path_buf();

        // Create test structure
        std::fs::create_dir_all(root.join("warehouse")).unwrap();
        std::fs::create_dir_all(root.join("sales")).unwrap();
        std::fs::create_dir_all(root.join("_shared")).unwrap();

        let config = create_test_config(root.clone());
        let scanner = Scanner::new(config, root);

        let contexts = scanner.scan_contexts().unwrap();

        assert_eq!(contexts.len(), 2);
        assert!(contexts.iter().any(|c| c.name == "warehouse"));
        assert!(contexts.iter().any(|c| c.name == "sales"));
        assert!(!contexts.iter().any(|c| c.name == "_shared"));
    }

    #[test]
    fn test_scan_features_excludes_domain_directory() {
        let temp_dir = TempDir::new().unwrap();
        let context_path = temp_dir.path().to_path_buf();

        // Create DDD-style structure: domain/ + slices/
        std::fs::create_dir_all(context_path.join("domain/commands")).unwrap();
        std::fs::create_dir_all(context_path.join("domain/events")).unwrap();
        std::fs::create_dir_all(context_path.join("domain/aggregate_order")).unwrap();
        std::fs::create_dir_all(context_path.join("slices/place_order")).unwrap();
        std::fs::create_dir_all(context_path.join("slices/list_orders")).unwrap();
        std::fs::create_dir_all(context_path.join("_shared")).unwrap();

        // Config WITH DomainConfig — domain/ should be excluded from features
        let mut config = create_test_config(context_path.clone());
        config.domain = Some(crate::config::DomainConfig::default()); // path = "domain"

        let scanner = Scanner::new(config, context_path.clone());
        let features = scanner.scan_features(&context_path).unwrap();

        let feature_names: Vec<&str> = features.iter().map(|f| f.name.as_str()).collect();

        // domain/ and its children should NOT appear as features
        assert!(!feature_names.contains(&"domain"), "domain/ should be excluded");
        assert!(!feature_names.contains(&"commands"), "domain/commands/ should be excluded");
        assert!(!feature_names.contains(&"events"), "domain/events/ should be excluded");
        assert!(
            !feature_names.contains(&"aggregate_order"),
            "domain/aggregate_order/ should be excluded"
        );

        // slices/ and its children SHOULD appear
        assert!(feature_names.contains(&"slices"), "slices/ should be a feature");
        assert!(feature_names.contains(&"place_order"), "slices/place_order/ should be a feature");
        assert!(feature_names.contains(&"list_orders"), "slices/list_orders/ should be a feature");

        // _shared should be excluded (starts with _)
        assert!(!feature_names.contains(&"_shared"), "_shared/ should be excluded");
    }

    #[test]
    fn test_scan_features_without_domain_config_includes_all() {
        let temp_dir = TempDir::new().unwrap();
        let context_path = temp_dir.path().to_path_buf();

        // Same structure
        std::fs::create_dir_all(context_path.join("domain/commands")).unwrap();
        std::fs::create_dir_all(context_path.join("slices/place_order")).unwrap();

        // Config WITHOUT DomainConfig — domain/ should still be included (legacy behavior)
        let config = create_test_config(context_path.clone());
        assert!(config.domain.is_none());

        let scanner = Scanner::new(config, context_path.clone());
        let features = scanner.scan_features(&context_path).unwrap();

        let feature_names: Vec<&str> = features.iter().map(|f| f.name.as_str()).collect();

        // Without DomainConfig, domain/ IS treated as a feature (legacy behavior)
        assert!(feature_names.contains(&"domain"), "domain/ should be included without DomainConfig");
        assert!(
            feature_names.contains(&"commands"),
            "domain/commands/ should be included without DomainConfig"
        );
    }
}
