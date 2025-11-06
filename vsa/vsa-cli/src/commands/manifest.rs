//! Generate manifest

use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};
use vsa_core::{Manifest, VsaConfig};

pub fn run(config_path: &Path, output: Option<PathBuf>, format: String) -> Result<()> {
    println!("📝 Generating manifest...");

    // Load configuration
    let config = VsaConfig::from_file(config_path)?;
    let config_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
    let root = config.resolve_root(config_dir);

    // Generate manifest
    let manifest = Manifest::generate(&config, root)?;

    // Serialize based on format
    let content = match format.as_str() {
        "json" => manifest.to_json()?,
        "yaml" => manifest.to_yaml()?,
        _ => anyhow::bail!("Unknown format: {format}. Use 'json' or 'yaml'"),
    };

    // Output
    if let Some(output_path) = output {
        fs::write(&output_path, &content)?;
        println!("✅ Manifest written to: {}", output_path.display());
    } else {
        println!("{content}");
    }

    Ok(())
}
