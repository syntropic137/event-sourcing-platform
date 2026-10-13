//! Validate VSA structure

use anyhow::Result;
use chrono::Local;
use console::{style, Term};
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::mpsc::channel;
use std::time::Duration;
use vsa_core::{ValidationConfig, Validator, VsaConfig};

pub fn run(config_path: &Path, _fix: bool, watch: bool) -> Result<()> {
    if watch {
        run_watch_mode(config_path)
    } else {
        run_once(config_path)
    }
}

fn run_once(config_path: &Path) -> Result<()> {
    let term = Term::stdout();

    term.write_line(&format!("{} Validating VSA structure...", style("🔍").bold()))?;
    term.write_line("")?;

    // Load configuration
    let config = VsaConfig::from_file(config_path)?;
    let config_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
    let root = config.resolve_root(config_dir);

    term.write_line(&format!("📁 Root: {}", root.display()))?;
    term.write_line(&format!("🗣️  Language: {}", config.language))?;
    term.write_line("")?;

    // The exit policy (fail_on_errors, max_warnings) is read before `config`
    // moves into the validator.
    let policy = config.validation.clone();

    // Create validator
    let validator = Validator::new(config, root);

    // Run validation
    let report = validator.validate()?;

    // Print results
    print_validation_report(&term, &report, &policy)?;

    match report.gate_failure(&policy) {
        Some(reason) => anyhow::bail!(reason),
        None => Ok(()),
    }
}

fn run_watch_mode(config_path: &Path) -> Result<()> {
    let term = Term::stdout();

    term.write_line(&format!(
        "{} Watch mode enabled - monitoring for changes...",
        style("👁️").bold()
    ))?;
    term.write_line(&format!("{} Press Ctrl+C to stop", style("ℹ").blue()))?;
    term.write_line("")?;

    // Load configuration
    let config = VsaConfig::from_file(config_path)?;
    let config_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
    let root = config.resolve_root(config_dir);

    // Run initial validation
    run_validation(&term, &config, &root)?;

    // Setup file watcher
    let (tx, rx) = channel();
    let mut watcher = RecommendedWatcher::new(tx, Config::default())?;

    // Watch the root directory
    watcher.watch(&root, RecursiveMode::Recursive)?;

    term.write_line("")?;
    term.write_line(&format!("{} Watching {} for changes...", style("👀").bold(), root.display()))?;
    term.write_line("")?;

    // Watch loop with debouncing
    let mut last_validation = std::time::Instant::now();
    let debounce_duration = Duration::from_millis(500);

    for res in rx {
        match res {
            Ok(event) => {
                if should_trigger_validation(&event) {
                    let now = std::time::Instant::now();
                    if now.duration_since(last_validation) > debounce_duration {
                        last_validation = now;

                        // Clear screen and re-run validation
                        term.clear_screen()?;
                        term.write_line(&format!(
                            "{} File changed at {}",
                            style("🔄").cyan(),
                            Local::now().format("%H:%M:%S")
                        ))?;
                        term.write_line("")?;

                        if let Err(e) = run_validation(&term, &config, &root) {
                            term.write_line(&format!(
                                "{} Validation error: {}",
                                style("❌").red(),
                                e
                            ))?;
                        }

                        term.write_line("")?;
                        term.write_line(&format!("{} Watching for changes...", style("👀").dim()))?;
                    }
                }
            }
            Err(e) => {
                term.write_line(&format!("{} Watch error: {}", style("⚠️").yellow(), e))?;
            }
        }
    }

    Ok(())
}

fn run_validation(term: &Term, config: &VsaConfig, root: &Path) -> Result<()> {
    term.write_line(&format!("{} Validating...", style("🔍").bold()))?;
    term.write_line("")?;

    // Create validator
    let validator = Validator::new(config.clone(), root.to_path_buf());

    // Run validation
    let report = validator.validate()?;

    // Print results
    print_validation_report(term, &report, &config.validation)?;

    Ok(())
}

fn print_validation_report(
    term: &Term,
    report: &vsa_core::validator::ValidationReport,
    policy: &ValidationConfig,
) -> Result<()> {
    // Print results
    if report.errors.is_empty() && report.warnings.is_empty() {
        term.write_line(&format!("{}", style("✅ All checks passed!").green().bold()))?;
        return Ok(());
    }

    // Print errors
    if !report.errors.is_empty() {
        term.write_line(&format!(
            "{}",
            style(format!("❌ {} Error(s)", report.errors.len())).red().bold()
        ))?;
        for error in &report.errors {
            term.write_line(&format!("  {} {}", style("×").red(), error.message))?;
            term.write_line(&format!("    at: {}", error.path.display()))?;
        }
        term.write_line("")?;
    }

    // Print warnings
    if !report.warnings.is_empty() {
        term.write_line(&format!(
            "{}",
            style(format!("⚠️  {} Warning(s)", report.warnings.len())).yellow().bold()
        ))?;
        for warning in &report.warnings {
            term.write_line(&format!("  {} {}", style("!").yellow(), warning.message))?;
            term.write_line(&format!("    at: {}", warning.path.display()))?;
        }
        term.write_line("")?;
    }

    match report.gate_failure(policy) {
        Some(reason) => term.write_line(&format!("{}", style(format!("❌ {reason}")).red()))?,
        None => {
            term.write_line(&format!("{}", style("✅ Validation passed with warnings").green()))?
        }
    }

    Ok(())
}

fn should_trigger_validation(event: &Event) -> bool {
    // Only trigger on modify and create events for relevant file types
    use notify::EventKind;
    match event.kind {
        EventKind::Create(_) | EventKind::Modify(_) => {
            // Check if any of the paths are relevant (ts, py, rs, yaml files)
            event.paths.iter().any(|p| {
                if let Some(ext) = p.extension() {
                    matches!(
                        ext.to_str(),
                        Some("ts") | Some("py") | Some("rs") | Some("yaml") | Some("yml")
                    )
                } else {
                    false
                }
            })
        }
        _ => false,
    }
}
