//! Generate new features

use anyhow::Result;
use console::{style, Term};
use dialoguer::{Confirm, Input};
use std::fs;
use std::path::Path;
use vsa_core::VsaConfig;

use crate::templates::{TemplateContext, TemplateEngine};

pub fn run(
    config_path: &Path,
    context: String,
    feature: String,
    _feature_type: Option<String>,
    interactive: bool,
) -> Result<()> {
    let term = Term::stdout();

    term.write_line(&format!(
        "{} Generating feature '{}' in context '{}'...",
        style("🚧").bold(),
        style(&feature).cyan(),
        style(&context).cyan()
    ))?;
    term.write_line("")?;

    // Load configuration
    let config = VsaConfig::from_file(config_path)?;
    let config_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
    let root = config.resolve_root(config_dir);

    // Create template context
    let mut ctx = TemplateContext::from_feature_path(&feature, &context, &config);

    // Interactive mode: prompt for fields
    if interactive {
        term.write_line(&format!("{} Let's configure your feature", style("📋").bold()))?;
        term.write_line("")?;

        // Prompt for fields
        loop {
            let field_name: String = Input::new()
                .with_prompt("Field name (or press Enter to finish)")
                .allow_empty(true)
                .interact_text()?;

            if field_name.is_empty() {
                break;
            }

            let field_type: String = Input::new()
                .with_prompt("Field type")
                .default("string".to_string())
                .interact_text()?;

            let is_required =
                Confirm::new().with_prompt("Is this field required?").default(true).interact()?;

            ctx.add_field(field_name.clone(), field_type.clone(), is_required);
            term.write_line(&format!(
                "  {} Added field: {} ({}, {})",
                style("✓").green(),
                style(&field_name).cyan(),
                field_type,
                if is_required { "required" } else { "optional" }
            ))?;
        }

        // Prompt for aggregate
        let with_aggregate =
            Confirm::new().with_prompt("Include aggregate?").default(false).interact()?;

        if with_aggregate {
            let aggregate_name: String = Input::new()
                .with_prompt("Aggregate name")
                .default(format!("{}Aggregate", ctx.operation_name))
                .interact_text()?;
            ctx.aggregate_name = Some(aggregate_name);
        }

        // Prompt for integration events
        let publishes_integration_events = Confirm::new()
            .with_prompt("Does this feature publish integration events?")
            .default(false)
            .interact()?;

        if publishes_integration_events {
            term.write_line(&format!(
                "{} Integration events will need to be defined in _shared/integration-events/",
                style("ℹ").blue()
            ))?;
        }

        term.write_line("")?;
    } else {
        // Non-interactive mode: use defaults
        ctx.add_field("id".to_string(), "string".to_string(), true);
    }

    // Validate we have at least one field
    if ctx.fields.is_empty() {
        term.write_line(&format!("{} Adding default 'id' field", style("ℹ").blue()))?;
        ctx.add_field("id".to_string(), "string".to_string(), true);
    }

    // Create template engine
    let engine = TemplateEngine::new(config.clone())?;

    // Generate feature directory
    let feature_path = root.join(&context).join(&feature);
    fs::create_dir_all(&feature_path)?;

    // Generate files
    let command_file = feature_path.join(format!("{}.{}", ctx.command_name, ctx.extension));
    let event_file = feature_path.join(format!("{}.{}", ctx.event_name, ctx.extension));
    let handler_file = feature_path.join(format!("{}.{}", ctx.handler_name, ctx.extension));
    let test_file = feature_path.join(format!("{}.test.{}", ctx.test_name, ctx.extension));

    // Render and write templates
    fs::write(&command_file, engine.render_command(&ctx)?)?;
    fs::write(&event_file, engine.render_event(&ctx)?)?;
    fs::write(&handler_file, engine.render_handler(&ctx)?)?;
    fs::write(&test_file, engine.render_test(&ctx)?)?;

    // Optionally generate aggregate
    if let Some(ref aggregate_name) = ctx.aggregate_name {
        let aggregate_file = feature_path.join(format!("{}.{}", aggregate_name, ctx.extension));
        fs::write(&aggregate_file, engine.render_aggregate(&ctx)?)?;

        term.write_line("")?;
        term.write_line(&format!("{}", style("✅ Created feature files:").green().bold()))?;
        term.write_line(&format!("  {} {}", style("├─").dim(), command_file.display()))?;
        term.write_line(&format!("  {} {}", style("├─").dim(), event_file.display()))?;
        term.write_line(&format!("  {} {}", style("├─").dim(), handler_file.display()))?;
        term.write_line(&format!("  {} {}", style("├─").dim(), aggregate_file.display()))?;
        term.write_line(&format!("  {} {}", style("└─").dim(), test_file.display()))?;
    } else {
        term.write_line("")?;
        term.write_line(&format!("{}", style("✅ Created feature files:").green().bold()))?;
        term.write_line(&format!("  {} {}", style("├─").dim(), command_file.display()))?;
        term.write_line(&format!("  {} {}", style("├─").dim(), event_file.display()))?;
        term.write_line(&format!("  {} {}", style("├─").dim(), handler_file.display()))?;
        term.write_line(&format!("  {} {}", style("└─").dim(), test_file.display()))?;
    }

    term.write_line("")?;
    term.write_line(&format!("{}", style("💡 Next steps:").bold()))?;
    term.write_line(&format!("  1. Implement business logic in {}", ctx.handler_name))?;
    term.write_line(&format!("  2. Add tests in {}.test.{}", ctx.test_name, ctx.extension))?;
    term.write_line("  3. Run: vsa validate")?;

    Ok(())
}
