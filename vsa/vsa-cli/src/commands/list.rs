//! List contexts and features

use anyhow::Result;
use console::style;
use std::path::Path;
use vsa_core::{Scanner, VsaConfig};

pub fn run(
    config_path: &Path,
    contexts_only: bool,
    context_filter: Option<String>,
    format: String,
) -> Result<()> {
    // Load configuration
    let config = VsaConfig::from_file(config_path)?;
    let config_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
    let root = config.resolve_root(config_dir);

    let scanner = Scanner::new(config, root);
    let contexts = scanner.scan_contexts()?;

    if contexts.is_empty() {
        println!("No contexts found");
        return Ok(());
    }

    match format.as_str() {
        "tree" => {
            println!("{}", style("📦 Contexts").bold());
            for context in &contexts {
                // Filter by context if specified
                if let Some(ref filter) = context_filter {
                    if context.name != *filter {
                        continue;
                    }
                }

                println!("  {} {}", style("├─").dim(), style(&context.name).cyan());

                if !contexts_only {
                    let features = scanner.scan_features(&context.path)?;
                    for (i, feature) in features.iter().enumerate() {
                        let is_last = i == features.len() - 1;
                        let prefix = if is_last { "└─" } else { "├─" };
                        println!("    {} {}", style(prefix).dim(), feature.relative_path.display());
                    }
                }
            }
        }
        "json" => {
            // TODO: Implement JSON output
            println!("JSON output not yet implemented");
        }
        _ => {
            anyhow::bail!("Unknown format: {format}");
        }
    }

    Ok(())
}
