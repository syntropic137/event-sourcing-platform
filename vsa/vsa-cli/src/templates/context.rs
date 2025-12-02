//! Template context for code generation

use serde::Serialize;
use vsa_core::VsaConfig;

/// Context data for template rendering
#[derive(Debug, Clone, Serialize)]
pub struct TemplateContext {
    /// Feature name (e.g., "create-product")
    pub feature_name: String,

    /// Operation name in PascalCase (e.g., "CreateProduct")
    pub operation_name: String,

    /// Command class name (e.g., "CreateProductCommand")
    pub command_name: String,

    /// Event class name (e.g., "ProductCreatedEvent")
    pub event_name: String,

    /// Handler class name (e.g., "CreateProductHandler")
    pub handler_name: String,

    /// Aggregate class name (e.g., "ProductAggregate")
    pub aggregate_name: Option<String>,

    /// Test file name (e.g., "CreateProduct")
    pub test_name: String,

    /// Fields for the command/event
    pub fields: Vec<FieldInfo>,

    /// Framework integration context
    pub framework: Option<FrameworkContext>,

    /// File extension (ts, py, rs)
    pub extension: String,

    /// Context name (e.g., "warehouse")
    pub context_name: String,

    // =========================================================================
    // QUERY SLICE FIELDS (CQRS Read Side)
    // =========================================================================
    /// Query class name (e.g., "ListProductsQuery")
    pub query_name: Option<String>,

    /// Projection class name (e.g., "ProductListProjection")
    pub projection_name: Option<String>,

    /// Query handler class name (e.g., "ListProductsHandler")
    pub query_handler_name: Option<String>,

    /// Controller class name (e.g., "ListProductsController")
    pub controller_name: Option<String>,

    /// Read model name (e.g., "ProductSummary")
    pub read_model_name: Option<String>,

    /// Slice type (command, query, saga)
    pub slice_type: Option<String>,

    /// Events that the projection subscribes to
    pub subscribed_events: Vec<String>,

    /// Whether this is a list query (returns multiple items)
    pub is_list: bool,
}

/// Field information for templates
#[derive(Debug, Clone, Serialize)]
pub struct FieldInfo {
    /// Field name
    pub name: String,

    /// Field name in PascalCase (for getters)
    pub name_pascal: String,

    /// Field type
    pub field_type: String,

    /// Whether field is required
    pub is_required: bool,

    /// Default value (if any)
    pub default: Option<String>,
}

/// Framework integration context
#[derive(Debug, Clone, Serialize)]
pub struct FrameworkContext {
    /// Framework name
    pub name: String,

    /// Import path for base domain event
    pub domain_event_import: String,

    /// Base domain event class name
    pub domain_event_class: String,

    /// Import path for aggregate
    pub aggregate_import: Option<String>,

    /// Base aggregate class name
    pub aggregate_class: Option<String>,

    /// Import path for command handler
    pub handler_import: Option<String>,

    /// Base handler class name
    pub handler_class: Option<String>,
}

impl TemplateContext {
    /// Create context from feature path
    pub fn from_feature_path(feature_path: &str, context_name: &str, config: &VsaConfig) -> Self {
        let feature_name = feature_path.split('/').next_back().unwrap_or(feature_path).to_string();

        let operation_name = Self::to_pascal_case(&feature_name);
        let command_name = format!("{operation_name}Command");
        let event_name = Self::to_event_name(&operation_name);
        let handler_name = format!("{operation_name}Handler");
        let test_name = operation_name.clone();

        let framework = config.framework.as_ref().map(|fw| FrameworkContext {
            name: fw.name.clone(),
            domain_event_import: fw
                .base_types
                .get("domain_event")
                .map(|bt| bt.import.clone())
                .unwrap_or_default(),
            domain_event_class: fw
                .base_types
                .get("domain_event")
                .map(|bt| bt.class.clone())
                .unwrap_or_else(|| "BaseDomainEvent".to_string()),
            aggregate_import: fw.base_types.get("aggregate").map(|bt| bt.import.clone()),
            aggregate_class: fw.base_types.get("aggregate").map(|bt| bt.class.clone()),
            handler_import: fw.base_types.get("command_handler").map(|bt| bt.import.clone()),
            handler_class: fw.base_types.get("command_handler").map(|bt| bt.class.clone()),
        });

        Self {
            feature_name,
            operation_name,
            command_name,
            event_name,
            handler_name,
            aggregate_name: None,
            test_name,
            fields: Vec::new(),
            framework,
            extension: config.file_extension().to_string(),
            context_name: context_name.to_string(),
            // Query slice fields (default)
            query_name: None,
            projection_name: None,
            query_handler_name: None,
            controller_name: None,
            read_model_name: None,
            slice_type: None,
            subscribed_events: Vec::new(),
            is_list: false,
        }
    }

    /// Create context for a query slice
    pub fn for_query_slice(
        feature_path: &str,
        context_name: &str,
        config: &VsaConfig,
        is_list: bool,
    ) -> Self {
        let feature_name = feature_path.split('/').next_back().unwrap_or(feature_path).to_string();

        let operation_name = Self::to_pascal_case(&feature_name);

        // Query slice naming
        let query_name = format!("{operation_name}Query");
        let projection_name = format!("{operation_name}Projection");
        let query_handler_name = format!("{operation_name}Handler");
        let controller_name = format!("{operation_name}Controller");
        let read_model_name = Self::to_read_model_name(&operation_name);

        let framework = config.framework.as_ref().map(|fw| FrameworkContext {
            name: fw.name.clone(),
            domain_event_import: fw
                .base_types
                .get("domain_event")
                .map(|bt| bt.import.clone())
                .unwrap_or_default(),
            domain_event_class: fw
                .base_types
                .get("domain_event")
                .map(|bt| bt.class.clone())
                .unwrap_or_else(|| "BaseDomainEvent".to_string()),
            aggregate_import: fw.base_types.get("aggregate").map(|bt| bt.import.clone()),
            aggregate_class: fw.base_types.get("aggregate").map(|bt| bt.class.clone()),
            handler_import: fw.base_types.get("command_handler").map(|bt| bt.import.clone()),
            handler_class: fw.base_types.get("command_handler").map(|bt| bt.class.clone()),
        });

        Self {
            feature_name: feature_name.clone(),
            operation_name: operation_name.clone(),
            command_name: String::new(), // Not used for query slices
            event_name: String::new(),   // Not used for query slices
            handler_name: query_handler_name.clone(),
            aggregate_name: None,
            test_name: operation_name,
            fields: Vec::new(),
            framework,
            extension: config.file_extension().to_string(),
            context_name: context_name.to_string(),
            // Query slice specific fields
            query_name: Some(query_name),
            projection_name: Some(projection_name),
            query_handler_name: Some(query_handler_name),
            controller_name: Some(controller_name),
            read_model_name: Some(read_model_name),
            slice_type: Some("query".to_string()),
            subscribed_events: Vec::new(), // To be populated by user
            is_list,
        }
    }

    /// Add a subscribed event to the query slice
    pub fn add_subscribed_event(&mut self, event_name: String) {
        self.subscribed_events.push(event_name);
    }

    /// Generate read model name from operation name
    fn to_read_model_name(operation: &str) -> String {
        // List* -> *Summary, Get* -> *Detail
        // Handle edge cases where prefix stripping results in empty string
        if operation.starts_with("List") || operation.starts_with("GetAll") {
            let rest = operation
                .strip_prefix("List")
                .or_else(|| operation.strip_prefix("GetAll"))
                .unwrap_or(operation)
                .trim();

            if rest.is_empty() {
                "ItemSummary".to_string()
            } else {
                format!("{rest}Summary")
            }
        } else if operation.starts_with("Get") {
            let rest = operation.strip_prefix("Get").unwrap_or(operation).trim();

            if rest.is_empty() {
                "ItemDetail".to_string()
            } else {
                format!("{rest}Detail")
            }
        } else {
            format!("{operation}ReadModel")
        }
    }

    /// Add a field to the context
    pub fn add_field(&mut self, name: String, field_type: String, required: bool) {
        let name_pascal = Self::to_pascal_case(&name);

        // Convert field type based on extension/language
        let converted_type = match self.extension.as_str() {
            "py" => Self::to_python_type(&field_type),
            "rs" => Self::to_rust_type(&field_type),
            _ => field_type.clone(), // TypeScript keeps original type
        };

        self.fields.push(FieldInfo {
            name,
            name_pascal,
            field_type: converted_type,
            is_required: required,
            default: None,
        });
    }

    /// Convert TypeScript types to Python types
    fn to_python_type(ts_type: &str) -> String {
        match ts_type {
            "string" => "str".to_string(),
            "number" => "float".to_string(),
            "boolean" => "bool".to_string(),
            "Date" => "datetime".to_string(),
            "any" => "Any".to_string(),
            // Handle arrays
            t if t.ends_with("[]") => {
                let inner = t.strip_suffix("[]").unwrap();
                format!("list[{}]", Self::to_python_type(inner))
            }
            // Handle optional types (T | null)
            t if t.contains(" | null") || t.contains(" | None") => {
                let inner = t.replace(" | null", "").replace(" | None", "");
                format!("{} | None", Self::to_python_type(&inner))
            }
            // Handle Record types
            t if t.starts_with("Record<") => {
                // Extract key and value types
                let inner =
                    t.strip_prefix("Record<").and_then(|s| s.strip_suffix(">")).unwrap_or("");
                let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
                if parts.len() == 2 {
                    format!(
                        "dict[{}, {}]",
                        Self::to_python_type(parts[0]),
                        Self::to_python_type(parts[1])
                    )
                } else {
                    "dict[str, Any]".to_string()
                }
            }
            // Default: keep as is (for custom types)
            _ => ts_type.to_string(),
        }
    }

    /// Convert TypeScript types to Rust types (placeholder for future)
    fn to_rust_type(ts_type: &str) -> String {
        match ts_type {
            "string" => "String".to_string(),
            "number" => "f64".to_string(),
            "boolean" => "bool".to_string(),
            _ => ts_type.to_string(),
        }
    }

    /// Convert kebab-case to PascalCase
    fn to_pascal_case(s: &str) -> String {
        s.split('-')
            .filter(|part| !part.is_empty())
            .map(|part| {
                let mut chars = part.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().chain(chars).collect(),
                }
            })
            .collect()
    }

    /// Generate event name from operation (e.g., "CreateProduct" -> "ProductCreatedEvent")
    fn to_event_name(operation: &str) -> String {
        // Simple heuristic: if starts with Create/Update/Delete, move verb to past tense
        if let Some(rest) = operation.strip_prefix("Create") {
            format!("{rest}CreatedEvent")
        } else if let Some(rest) = operation.strip_prefix("Update") {
            format!("{rest}UpdatedEvent")
        } else if let Some(rest) = operation.strip_prefix("Delete") {
            format!("{rest}DeletedEvent")
        } else {
            format!("{operation}Event")
        }
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
        }
    }

    #[test]
    fn test_to_pascal_case() {
        assert_eq!(TemplateContext::to_pascal_case("create-product"), "CreateProduct");
        assert_eq!(TemplateContext::to_pascal_case("update-inventory"), "UpdateInventory");
        assert_eq!(TemplateContext::to_pascal_case("single"), "Single");
    }

    #[test]
    fn test_to_event_name() {
        assert_eq!(TemplateContext::to_event_name("CreateProduct"), "ProductCreatedEvent");
        assert_eq!(TemplateContext::to_event_name("UpdateInventory"), "InventoryUpdatedEvent");
        assert_eq!(TemplateContext::to_event_name("ProcessOrder"), "ProcessOrderEvent");
    }

    #[test]
    fn test_from_feature_path() {
        let config = create_test_config();
        let ctx = TemplateContext::from_feature_path(
            "warehouse/products/create-product",
            "warehouse",
            &config,
        );

        assert_eq!(ctx.feature_name, "create-product");
        assert_eq!(ctx.operation_name, "CreateProduct");
        assert_eq!(ctx.command_name, "CreateProductCommand");
        assert_eq!(ctx.event_name, "ProductCreatedEvent");
        assert_eq!(ctx.handler_name, "CreateProductHandler");
        assert_eq!(ctx.context_name, "warehouse");
    }

    #[test]
    fn test_python_type_conversion() {
        assert_eq!(TemplateContext::to_python_type("string"), "str");
        assert_eq!(TemplateContext::to_python_type("number"), "float");
        assert_eq!(TemplateContext::to_python_type("boolean"), "bool");
        assert_eq!(TemplateContext::to_python_type("Date"), "datetime");
        assert_eq!(TemplateContext::to_python_type("any"), "Any");
    }

    #[test]
    fn test_python_array_conversion() {
        assert_eq!(TemplateContext::to_python_type("string[]"), "list[str]");
        assert_eq!(TemplateContext::to_python_type("number[]"), "list[float]");
    }

    #[test]
    fn test_python_optional_conversion() {
        assert_eq!(TemplateContext::to_python_type("string | null"), "str | None");
        assert_eq!(TemplateContext::to_python_type("number | null"), "float | None");
    }

    #[test]
    fn test_python_record_conversion() {
        assert_eq!(TemplateContext::to_python_type("Record<string, number>"), "dict[str, float]");
    }

    #[test]
    fn test_to_read_model_name() {
        // Standard cases - implementation strips prefix and appends Summary/Detail
        assert_eq!(TemplateContext::to_read_model_name("ListWorkflows"), "WorkflowsSummary");
        assert_eq!(TemplateContext::to_read_model_name("GetWorkflow"), "WorkflowDetail");
        assert_eq!(TemplateContext::to_read_model_name("GetAllItems"), "ItemsSummary");

        // Edge cases - empty after prefix strip
        assert_eq!(TemplateContext::to_read_model_name("List"), "ItemSummary");
        assert_eq!(TemplateContext::to_read_model_name("Get"), "ItemDetail");
        assert_eq!(TemplateContext::to_read_model_name("GetAll"), "ItemSummary");

        // Fallback case
        assert_eq!(TemplateContext::to_read_model_name("FetchOrders"), "FetchOrdersReadModel");
    }

    #[test]
    fn test_python_field_type_conversion() {
        let mut config = create_test_config();
        config.language = "python".to_string();

        let mut ctx = TemplateContext::from_feature_path("create-product", "warehouse", &config);

        ctx.add_field("name".to_string(), "string".to_string(), true);
        ctx.add_field("price".to_string(), "number".to_string(), true);
        ctx.add_field("available".to_string(), "boolean".to_string(), true);

        assert_eq!(ctx.fields[0].field_type, "str");
        assert_eq!(ctx.fields[1].field_type, "float");
        assert_eq!(ctx.fields[2].field_type, "bool");
    }
}
