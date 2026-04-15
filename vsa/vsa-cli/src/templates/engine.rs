//! Template rendering engine

use anyhow::Result;
use handlebars::{Context, Handlebars, Helper, HelperResult, JsonRender, Output, RenderContext};
use vsa_core::VsaConfig;

use super::context::TemplateContext;
use super::{python, typescript};

/// Helper to strip "Event" suffix from event names
/// Usage in templates: {{strip_event_suffix this}}
/// Example: "WorkflowCreatedEvent" -> "WorkflowCreated"
fn strip_event_suffix_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let param = h.param(0).map(|v| v.value().render()).unwrap_or_default();
    let stripped = param.strip_suffix("Event").unwrap_or(&param);
    out.write(stripped)?;
    Ok(())
}

/// Template engine for code generation
pub struct TemplateEngine {
    handlebars: Handlebars<'static>,
    config: VsaConfig,
}

impl TemplateEngine {
    /// Create a new template engine
    pub fn new(config: VsaConfig) -> Result<Self> {
        let mut handlebars = Handlebars::new();
        handlebars.set_strict_mode(true);

        // Register custom helpers
        handlebars.register_helper("strip_event_suffix", Box::new(strip_event_suffix_helper));

        // Register TypeScript templates
        handlebars.register_template_string("ts_command", typescript::COMMAND_TEMPLATE)?;
        handlebars.register_template_string("ts_event", typescript::EVENT_TEMPLATE)?;
        handlebars.register_template_string("ts_handler", typescript::HANDLER_TEMPLATE)?;
        handlebars.register_template_string("ts_test", typescript::TEST_TEMPLATE)?;
        handlebars.register_template_string("ts_aggregate", typescript::AGGREGATE_TEMPLATE)?;

        // Register Python templates
        handlebars.register_template_string("py_command", python::COMMAND_TEMPLATE)?;
        handlebars.register_template_string("py_event", python::EVENT_TEMPLATE)?;
        handlebars.register_template_string("py_handler", python::HANDLER_TEMPLATE)?;
        handlebars.register_template_string("py_test", python::TEST_TEMPLATE)?;
        handlebars.register_template_string("py_aggregate", python::AGGREGATE_TEMPLATE)?;

        // Register TypeScript query slice templates
        handlebars.register_template_string("ts_query", typescript::QUERY_TEMPLATE)?;
        handlebars.register_template_string("ts_projection", typescript::PROJECTION_TEMPLATE)?;
        handlebars
            .register_template_string("ts_query_handler", typescript::QUERY_HANDLER_TEMPLATE)?;
        handlebars.register_template_string(
            "ts_query_controller",
            typescript::QUERY_CONTROLLER_TEMPLATE,
        )?;
        handlebars.register_template_string("ts_query_test", typescript::QUERY_TEST_TEMPLATE)?;
        handlebars
            .register_template_string("ts_slice_manifest", typescript::SLICE_MANIFEST_TEMPLATE)?;

        // Register Python query slice templates
        handlebars.register_template_string("py_query", python::QUERY_TEMPLATE)?;
        handlebars.register_template_string("py_projection", python::PROJECTION_TEMPLATE)?;
        handlebars.register_template_string("py_query_handler", python::QUERY_HANDLER_TEMPLATE)?;
        handlebars
            .register_template_string("py_query_controller", python::QUERY_CONTROLLER_TEMPLATE)?;
        handlebars.register_template_string("py_query_test", python::QUERY_TEST_TEMPLATE)?;
        handlebars
            .register_template_string("py_slice_manifest", python::SLICE_MANIFEST_TEMPLATE)?;

        Ok(Self { handlebars, config })
    }

    /// Render command template
    pub fn render_command(&self, ctx: &TemplateContext) -> Result<String> {
        let template_name = match self.config.language.as_str() {
            "typescript" => "ts_command",
            "python" => "py_command",
            _ => anyhow::bail!("Unsupported language: {}", self.config.language),
        };

        Ok(self.handlebars.render(template_name, &ctx)?)
    }

    /// Render event template
    pub fn render_event(&self, ctx: &TemplateContext) -> Result<String> {
        let template_name = match self.config.language.as_str() {
            "typescript" => "ts_event",
            "python" => "py_event",
            _ => anyhow::bail!("Unsupported language: {}", self.config.language),
        };

        Ok(self.handlebars.render(template_name, &ctx)?)
    }

    /// Render handler template
    pub fn render_handler(&self, ctx: &TemplateContext) -> Result<String> {
        let template_name = match self.config.language.as_str() {
            "typescript" => "ts_handler",
            "python" => "py_handler",
            _ => anyhow::bail!("Unsupported language: {}", self.config.language),
        };

        Ok(self.handlebars.render(template_name, &ctx)?)
    }

    /// Render test template
    pub fn render_test(&self, ctx: &TemplateContext) -> Result<String> {
        let template_name = match self.config.language.as_str() {
            "typescript" => "ts_test",
            "python" => "py_test",
            _ => anyhow::bail!("Unsupported language: {}", self.config.language),
        };

        Ok(self.handlebars.render(template_name, &ctx)?)
    }

    /// Render aggregate template
    pub fn render_aggregate(&self, ctx: &TemplateContext) -> Result<String> {
        let template_name = match self.config.language.as_str() {
            "typescript" => "ts_aggregate",
            "python" => "py_aggregate",
            _ => anyhow::bail!("Unsupported language: {}", self.config.language),
        };

        Ok(self.handlebars.render(template_name, &ctx)?)
    }

    // =========================================================================
    // QUERY SLICE RENDERING METHODS
    // =========================================================================

    /// Render query template
    pub fn render_query(&self, ctx: &TemplateContext) -> Result<String> {
        let template_name = match self.config.language.as_str() {
            "typescript" => "ts_query",
            "python" => "py_query",
            _ => anyhow::bail!("Unsupported language: {}", self.config.language),
        };

        Ok(self.handlebars.render(template_name, &ctx)?)
    }

    /// Render projection template
    pub fn render_projection(&self, ctx: &TemplateContext) -> Result<String> {
        let template_name = match self.config.language.as_str() {
            "typescript" => "ts_projection",
            "python" => "py_projection",
            _ => anyhow::bail!("Unsupported language: {}", self.config.language),
        };

        Ok(self.handlebars.render(template_name, &ctx)?)
    }

    /// Render query handler template
    pub fn render_query_handler(&self, ctx: &TemplateContext) -> Result<String> {
        let template_name = match self.config.language.as_str() {
            "typescript" => "ts_query_handler",
            "python" => "py_query_handler",
            _ => anyhow::bail!("Unsupported language: {}", self.config.language),
        };

        Ok(self.handlebars.render(template_name, &ctx)?)
    }

    /// Render query controller template
    pub fn render_query_controller(&self, ctx: &TemplateContext) -> Result<String> {
        let template_name = match self.config.language.as_str() {
            "typescript" => "ts_query_controller",
            "python" => "py_query_controller",
            _ => anyhow::bail!("Unsupported language: {}", self.config.language),
        };

        Ok(self.handlebars.render(template_name, &ctx)?)
    }

    /// Render query test template
    pub fn render_query_test(&self, ctx: &TemplateContext) -> Result<String> {
        let template_name = match self.config.language.as_str() {
            "typescript" => "ts_query_test",
            "python" => "py_query_test",
            _ => anyhow::bail!("Unsupported language: {}", self.config.language),
        };

        Ok(self.handlebars.render(template_name, &ctx)?)
    }

    /// Render slice manifest template
    pub fn render_slice_manifest(&self, ctx: &TemplateContext) -> Result<String> {
        let template_name = match self.config.language.as_str() {
            "typescript" => "ts_slice_manifest",
            "python" => "py_slice_manifest",
            _ => anyhow::bail!("Unsupported language: {}", self.config.language),
        };

        Ok(self.handlebars.render(template_name, &ctx)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use vsa_core::config::{PatternsConfig, ValidationConfig};

    fn create_test_config() -> VsaConfig {
        VsaConfig {
            version: 2,
            architecture: vsa_core::ArchitectureType::HexagonalEventSourcedVsa,
            root: std::path::PathBuf::from("./src/contexts"),
            language: "typescript".to_string(),
            domain: Some(vsa_core::DomainConfig::default()),
            slices: Some(vsa_core::SlicesConfig::default()),
            infrastructure: Some(vsa_core::InfrastructureConfig::default()),
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
    fn test_engine_creation() {
        let config = create_test_config();
        let engine = TemplateEngine::new(config);
        assert!(engine.is_ok());
    }

    #[test]
    fn test_render_command() {
        let config = create_test_config();
        let engine = TemplateEngine::new(config.clone()).unwrap();

        let ctx = TemplateContext::from_feature_path("create-product", "warehouse", &config);

        let result = engine.render_command(&ctx);
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.contains("CreateProductCommand"));
        assert!(output.contains("export class"));
    }

    fn create_python_test_config() -> VsaConfig {
        VsaConfig {
            version: 2,
            architecture: vsa_core::ArchitectureType::HexagonalEventSourcedVsa,
            root: std::path::PathBuf::from("./src/contexts"),
            language: "python".to_string(),
            domain: Some(vsa_core::DomainConfig::default()),
            slices: Some(vsa_core::SlicesConfig::default()),
            infrastructure: Some(vsa_core::InfrastructureConfig::default()),
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
    fn test_python_engine_creation() {
        let config = create_python_test_config();
        let engine = TemplateEngine::new(config);
        assert!(engine.is_ok());
    }

    #[test]
    fn test_render_python_command() {
        let config = create_python_test_config();
        let engine = TemplateEngine::new(config.clone()).unwrap();

        let ctx = TemplateContext::from_feature_path("create-product", "warehouse", &config);

        let result = engine.render_command(&ctx);
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.contains("CreateProductCommand"));
        assert!(output.contains("class CreateProductCommand"));
        assert!(output.contains("BaseModel"));
    }

    #[test]
    fn test_render_python_event() {
        let config = create_python_test_config();
        let engine = TemplateEngine::new(config.clone()).unwrap();

        let ctx = TemplateContext::from_feature_path("create-product", "warehouse", &config);

        let result = engine.render_event(&ctx);
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.contains("ProductCreatedEvent"));
        assert!(output.contains("class ProductCreatedEvent"));
    }

    #[test]
    fn test_render_python_handler() {
        let config = create_python_test_config();
        let engine = TemplateEngine::new(config.clone()).unwrap();

        let ctx = TemplateContext::from_feature_path("create-product", "warehouse", &config);

        let result = engine.render_handler(&ctx);
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.contains("CreateProductHandler"));
        assert!(output.contains("async def handle"));
    }
}
