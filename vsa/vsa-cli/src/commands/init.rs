//! Initialize VSA configuration

use anyhow::Result;
use std::fs;
use std::path::PathBuf;

const CONFIG_TEMPLATE: &str = r#"# VSA Configuration
# See: https://github.com/yourusername/vsa for full documentation

version: 1
root: ./src/contexts
language: typescript

# Optional: Event sourcing framework integration
# framework:
#   name: event-sourcing-platform
#   base_types:
#     domain_event:
#       import: "@event-sourcing-platform/typescript"
#       class: "BaseDomainEvent"
#     aggregate:
#       import: "@event-sourcing-platform/typescript"
#       class: "AggregateRoot"

# Validation rules
validation:
  require_tests: true
  require_integration_events_in_shared: true
  max_nesting_depth: 3
  allow_nested_features: true

# Pattern definitions (customize for your naming conventions)
patterns:
  command: "*Command"
  event: "*Event"
  handler: "*Handler"
  query: "*Query"
  integration_event: "*IntegrationEvent"
  test: "*.test"

# Context-specific configuration
# contexts:
#   warehouse:
#     description: "Warehouse management bounded context"
#   sales:
#     description: "Sales and order management bounded context"
"#;

const CONFIG_TEMPLATE_WITH_FRAMEWORK: &str = r#"# VSA Configuration with Event Sourcing Platform Integration
# See: https://github.com/yourusername/vsa for full documentation

version: 1
root: ./src/contexts
language: typescript

# Event sourcing framework integration
framework:
  name: event-sourcing-platform
  base_types:
    domain_event:
      import: "@event-sourcing-platform/typescript"
      class: "BaseDomainEvent"
    aggregate:
      import: "@event-sourcing-platform/typescript"
      class: "AggregateRoot"
    command_handler:
      import: "@event-sourcing-platform/typescript"
      class: "CommandHandler"

# Validation rules
validation:
  require_tests: true
  require_integration_events_in_shared: true
  max_nesting_depth: 3
  allow_nested_features: true

# Pattern definitions
patterns:
  command: "*Command"
  event: "*Event"
  handler: "*Handler"
  query: "*Query"
  integration_event: "*IntegrationEvent"
  test: "*.test"
"#;

const CONFIG_TEMPLATE_PYTHON: &str = r#"# VSA Configuration for Python
# See: https://github.com/yourusername/vsa for full documentation

version: 1
root: ./src/contexts
language: python

# Optional: Event sourcing framework integration
# framework:
#   name: event-sourcing-platform
#   base_types:
#     domain_event:
#       import: "event_sourcing"
#       class: "DomainEvent"
#     aggregate:
#       import: "event_sourcing"
#       class: "AggregateRoot"

# Validation rules
validation:
  require_tests: true
  require_integration_events_in_shared: true
  max_nesting_depth: 3
  allow_nested_features: true

# Pattern definitions (Python conventions)
patterns:
  command: "*Command"
  event: "*Event"
  handler: "*Handler"
  query: "*Query"
  integration_event: "*IntegrationEvent"
  test: "test_*"

# Context-specific configuration
# contexts:
#   warehouse:
#     description: "Warehouse management bounded context"
#   sales:
#     description: "Sales and order management bounded context"
"#;

const CONFIG_TEMPLATE_PYTHON_WITH_FRAMEWORK: &str = r#"# VSA Configuration for Python with Event Sourcing Platform Integration
# See: https://github.com/yourusername/vsa for full documentation

version: 1
root: ./src/contexts
language: python

# Event sourcing framework integration
framework:
  name: event-sourcing-platform
  base_types:
    domain_event:
      import: "event_sourcing"
      class: "DomainEvent"
    aggregate:
      import: "event_sourcing"
      class: "AggregateRoot"
    command_handler:
      import: "event_sourcing"
      class: "CommandHandler"

# Validation rules
validation:
  require_tests: true
  require_integration_events_in_shared: true
  max_nesting_depth: 3
  allow_nested_features: true

# Pattern definitions (Python conventions)
patterns:
  command: "*Command"
  event: "*Event"
  handler: "*Handler"
  query: "*Query"
  integration_event: "*IntegrationEvent"
  test: "test_*"
"#;

pub fn run(root: PathBuf, language: String, with_framework: bool) -> Result<()> {
    let config_path = PathBuf::from("vsa.yaml");

    if config_path.exists() {
        anyhow::bail!("Configuration file already exists: {}", config_path.display());
    }

    // Select template based on language and framework
    let template = match (language.as_str(), with_framework) {
        ("typescript", false) => CONFIG_TEMPLATE,
        ("typescript", true) => CONFIG_TEMPLATE_WITH_FRAMEWORK,
        ("python", false) => CONFIG_TEMPLATE_PYTHON,
        ("python", true) => CONFIG_TEMPLATE_PYTHON_WITH_FRAMEWORK,
        _ => CONFIG_TEMPLATE,
    };

    // Customize template (root path)
    let config = template.replace("./src/contexts", &root.to_string_lossy());

    // Write config
    fs::write(&config_path, config)?;

    println!("✅ Created VSA configuration: {}", config_path.display());
    println!();
    println!("Next steps:");
    println!("  1. Review and customize vsa.yaml");
    println!("  2. Create your first context: mkdir -p {}/your-context", root.display());
    println!("  3. Generate a feature: vsa generate -c your-context -f your-feature");

    // Language-specific next steps
    match language.as_str() {
        "python" => {
            println!("  4. Install event-sourcing SDK: pip install event-sourcing");
            println!("  5. Run tests: pytest");
            println!("  6. Validate structure: vsa validate");
        }
        _ => {
            println!("  4. Validate structure: vsa validate");
        }
    }

    Ok(())
}
