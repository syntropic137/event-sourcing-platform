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
}
