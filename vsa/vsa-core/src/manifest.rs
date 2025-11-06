//! Manifest generation

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::config::VsaConfig;
use crate::error::Result;
use crate::scanner::Scanner;

/// VSA manifest
#[derive(Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub version: String,
    pub generated_at: String,
    pub contexts: Vec<ContextManifest>,
}

/// Context manifest entry
#[derive(Debug, Serialize, Deserialize)]
pub struct ContextManifest {
    pub name: String,
    pub path: String,
    pub features: Vec<FeatureManifest>,
}

/// Feature manifest entry
#[derive(Debug, Serialize, Deserialize)]
pub struct FeatureManifest {
    pub name: String,
    pub path: String,
    pub files: Vec<String>,
}

impl Manifest {
    /// Generate manifest from current structure
    pub fn generate(config: &VsaConfig, root: PathBuf) -> Result<Self> {
        let scanner = Scanner::new(config.clone(), root);
        let contexts = scanner.scan_contexts()?;

        let mut context_manifests = Vec::new();

        for context in contexts {
            let features = scanner.scan_features(&context.path)?;
            let mut feature_manifests = Vec::new();

            for feature in features {
                let files = scanner.scan_feature_files(&feature.path)?;
                let file_names: Vec<String> = files.iter().map(|f| f.name.clone()).collect();

                feature_manifests.push(FeatureManifest {
                    name: feature.name.clone(),
                    path: feature.relative_path.to_string_lossy().to_string(),
                    files: file_names,
                });
            }

            context_manifests.push(ContextManifest {
                name: context.name.clone(),
                path: context.path.to_string_lossy().to_string(),
                features: feature_manifests,
            });
        }

        Ok(Self {
            version: crate::VERSION.to_string(),
            generated_at: chrono::Utc::now().to_rfc3339(),
            contexts: context_manifests,
        })
    }

    /// Export manifest as JSON
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Export manifest as YAML
    pub fn to_yaml(&self) -> Result<String> {
        Ok(serde_yaml::to_string(self)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_serialization() {
        let manifest = Manifest {
            version: "0.1.0".to_string(),
            generated_at: "2025-11-05T00:00:00Z".to_string(),
            contexts: vec![ContextManifest {
                name: "warehouse".to_string(),
                path: "/path/to/warehouse".to_string(),
                features: vec![FeatureManifest {
                    name: "create-product".to_string(),
                    path: "products/create-product".to_string(),
                    files: vec![
                        "CreateProductCommand.ts".to_string(),
                        "ProductCreatedEvent.ts".to_string(),
                    ],
                }],
            }],
        };

        let json = manifest.to_json().unwrap();
        assert!(json.contains("warehouse"));
        assert!(json.contains("create-product"));
    }
}
