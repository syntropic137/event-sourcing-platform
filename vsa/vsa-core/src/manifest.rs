//! Manifest generation

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::config::{ContextType, SliceType, VsaConfig};
use crate::domain::DomainModel;
use crate::error::Result;
use crate::scanner::Scanner;
use crate::scanners::DomainScanner;

/// VSA manifest schema version
/// v2.3.0 - Added aggregate entities, value_objects, and folder_name for relationship visualization
pub const MANIFEST_SCHEMA_VERSION: &str = "2.3.0";

/// VSA manifest
#[derive(Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub version: String,
    pub schema_version: String,
    pub generated_at: String,
    pub bounded_contexts: Vec<ContextManifest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<DomainManifest>,
}

/// Context manifest entry
#[derive(Debug, Serialize, Deserialize)]
pub struct ContextManifest {
    pub name: String,
    pub path: String,
    pub features: Vec<FeatureManifest>,
    /// Context type - bounded_context (has aggregates) or invalid_module (no aggregates)
    /// NEW in v2.2.0 - helps identify orphan projection modules
    #[serde(default)]
    pub context_type: ContextType,
    /// Number of aggregates in this context - NEW in v2.2.0
    /// A valid bounded context has aggregate_count > 0
    #[serde(default)]
    pub aggregate_count: usize,
    /// Infrastructure folders (repositories, buses, etc.) - NEW in v2.0.0
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub infrastructure_folders: Vec<String>,
}

/// Feature manifest entry
#[derive(Debug, Serialize, Deserialize)]
pub struct FeatureManifest {
    pub name: String,
    pub path: String,
    pub files: Vec<String>,
    /// The type of slice (command, query, saga, mixed, or unknown)
    /// Detected from file contents: *Command.* = command, *Query.*/*Projection.* = query
    #[serde(default)]
    pub slice_type: SliceType,
}

/// Domain manifest containing all domain model components
#[derive(Debug, Serialize, Deserialize)]
pub struct DomainManifest {
    pub aggregates: Vec<crate::domain::Aggregate>,
    pub commands: Vec<crate::domain::Command>,
    pub events: Vec<crate::domain::Event>,
    pub queries: Vec<crate::domain::Query>,
    /// Projections that build read models from events - NEW in v2.1.0
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub projections: Vec<crate::domain::Projection>,
    pub upcasters: Vec<crate::domain::Upcaster>,
    /// Value objects in the domain - NEW in v2.0.0
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub value_objects: Vec<crate::domain::ValueObject>,
    pub relationships: Relationships,
}

/// Relationships between domain components
#[derive(Debug, Serialize, Deserialize)]
pub struct Relationships {
    /// Maps command name to aggregate name
    pub command_to_aggregate: HashMap<String, String>,
    /// Maps aggregate name to list of events it emits
    pub aggregate_to_events: HashMap<String, Vec<String>>,
    /// Maps event type to list of aggregates that handle it
    pub event_to_handlers: HashMap<String, Vec<String>>,
    /// Maps event type to list of projections that subscribe to it - NEW in v2.1.0
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub event_to_projections: HashMap<String, Vec<String>>,
    /// Maps projection name to the read model it builds - NEW in v2.1.0
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub projection_to_read_model: HashMap<String, String>,
}

impl Manifest {
    /// Generate manifest from current structure
    pub fn generate(config: &VsaConfig, root: PathBuf) -> Result<Self> {
        Self::generate_with_options(config, root, false)
    }

    /// Generate manifest with optional domain model inclusion
    pub fn generate_with_options(
        config: &VsaConfig,
        root: PathBuf,
        include_domain: bool,
    ) -> Result<Self> {
        let scanner = Scanner::new(config.clone(), root.clone());
        let contexts = scanner.scan_contexts()?;

        // Create domain scanner for context classification (always needed for aggregate counting)
        let domain_scanner = config
            .domain
            .as_ref()
            .map(|domain_config| DomainScanner::new(domain_config.clone(), root.clone()));

        let mut context_manifests = Vec::new();

        for context in &contexts {
            let features = scanner.scan_features(&context.path)?;
            let mut feature_manifests = Vec::new();

            for feature in features {
                let files = scanner.scan_feature_files(&feature.path)?;
                let file_names: Vec<String> = files.iter().map(|f| f.name.clone()).collect();

                // Detect slice type from file names
                let slice_type = Self::detect_slice_type(&file_names);

                feature_manifests.push(FeatureManifest {
                    name: feature.name.clone(),
                    path: feature.relative_path.to_string_lossy().to_string(),
                    files: file_names,
                    slice_type,
                });
            }

            // Detect infrastructure folders
            let infrastructure_folders = Self::detect_infrastructure_folders(&context.path);

            // Count aggregates to classify context type
            let aggregate_count =
                Self::count_context_aggregates(&domain_scanner, &context.path, &context.name);

            // Classify context based on aggregate presence
            let context_type = if aggregate_count > 0 {
                ContextType::BoundedContext
            } else {
                ContextType::InvalidModule
            };

            context_manifests.push(ContextManifest {
                name: context.name.clone(),
                path: context.path.to_string_lossy().to_string(),
                features: feature_manifests,
                context_type,
                aggregate_count,
                infrastructure_folders,
            });
        }

        // Optionally scan domain model
        let domain = if include_domain && config.domain.is_some() {
            let domain_config = config.domain.as_ref().unwrap();
            let domain_scanner = DomainScanner::new(domain_config.clone(), root.clone());

            // Determine if we should use multi-context scanning
            let domain_model = if !contexts.is_empty() {
                // Multi-context architecture: scan each context's domain folder
                Self::scan_multi_context_domain(&domain_scanner, &contexts)?
            } else {
                // Monolithic architecture: scan single domain folder at root
                domain_scanner.scan()?
            };

            Some(Self::build_domain_manifest(&domain_model))
        } else {
            None
        };

        Ok(Self {
            version: crate::VERSION.to_string(),
            schema_version: MANIFEST_SCHEMA_VERSION.to_string(),
            generated_at: chrono::Utc::now().to_rfc3339(),
            bounded_contexts: context_manifests,
            domain,
        })
    }

    /// Count aggregates in a specific context
    ///
    /// Returns the number of aggregates found in the context's domain folder.
    /// Used to classify whether a directory is a valid bounded context.
    fn count_context_aggregates(
        domain_scanner: &Option<DomainScanner>,
        context_path: &std::path::Path,
        context_name: &str,
    ) -> usize {
        if let Some(scanner) = domain_scanner {
            // Try to scan the context's domain folder
            match scanner.scan_context(context_path, context_name) {
                Ok(domain_model) => domain_model.aggregates.len(),
                Err(_) => 0,
            }
        } else {
            // No domain config - use file-based heuristic
            Self::count_aggregates_by_filename(context_path)
        }
    }

    /// Count aggregates by scanning for *Aggregate.* files (fallback when no domain config)
    fn count_aggregates_by_filename(context_path: &Path) -> usize {
        let domain_path = context_path.join("domain");
        if !domain_path.exists() {
            return 0;
        }

        let mut count = 0;
        // Walk the domain directory looking for *Aggregate.* files
        for entry in WalkDir::new(&domain_path)
            .max_depth(3) // Check up to 3 levels deep (domain/aggregate_*/*)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.contains("Aggregate.")
                        && (name.ends_with(".py") || name.ends_with(".ts") || name.ends_with(".rs"))
                    {
                        count += 1;
                    }
                }
            }
        }
        count
    }

    /// Scan domain models across multiple bounded contexts and merge them
    fn scan_multi_context_domain(
        domain_scanner: &DomainScanner,
        contexts: &[crate::scanner::ContextInfo],
    ) -> Result<DomainModel> {
        // Try scanning root domain first (backward compatible)
        let root_domain_path = domain_scanner.root.join(&domain_scanner.config.path);
        let mut merged_model = if root_domain_path.exists() {
            domain_scanner.scan()?
        } else {
            DomainModel::new(root_domain_path)
        };

        // Scan each bounded context's domain folder
        for context in contexts {
            let context_domain_path = context.path.join(&domain_scanner.config.path);

            // Only scan if the context has a domain folder
            if context_domain_path.exists() {
                let context_model = domain_scanner.scan_context(&context.path, &context.name)?;

                // Only merge if the context model has components
                if context_model.component_count() > 0 {
                    merged_model.merge(context_model);
                }
            }
        }

        Ok(merged_model)
    }

    /// Build domain manifest from domain model
    fn build_domain_manifest(model: &DomainModel) -> DomainManifest {
        let relationships = Self::build_relationships(model);

        DomainManifest {
            aggregates: model.aggregates.clone(),
            commands: model.commands.clone(),
            events: model.events.clone(),
            queries: model.queries.clone(),
            projections: model.projections.clone(),
            upcasters: model.upcasters.clone(),
            value_objects: model.value_objects.clone(),
            relationships,
        }
    }

    /// Detect slice type from file names
    ///
    /// Detection logic:
    /// - Files ending with "Command.*" indicate a command slice
    /// - Files ending with "Query.*" or "Projection.*" or named "projection.*" indicate a query slice
    /// - Files ending with "Saga.*" indicate a saga slice
    /// - If both command and query indicators found → Mixed
    /// - If none found → Unknown
    fn detect_slice_type(file_names: &[String]) -> SliceType {
        let mut has_command = false;
        let mut has_query = false;
        let mut has_saga = false;

        for file_name in file_names {
            // Remove extension for checking
            let base = file_name
                .strip_suffix(".ts")
                .or_else(|| file_name.strip_suffix(".tsx"))
                .or_else(|| file_name.strip_suffix(".py"))
                .or_else(|| file_name.strip_suffix(".rs"))
                .or_else(|| file_name.strip_suffix(".js"))
                .or_else(|| file_name.strip_suffix(".jsx"))
                .unwrap_or(file_name);

            // Skip test files
            if file_name.contains(".test.")
                || file_name.contains("_test.")
                || file_name.starts_with("test_")
            {
                continue;
            }

            // Check for command files
            if base.ends_with("Command") {
                has_command = true;
            }

            // Check for query/projection files
            if base.ends_with("Query") || base.ends_with("Projection") || base == "projection" {
                has_query = true;
            }

            // Check for saga files
            if base.ends_with("Saga") {
                has_saga = true;
            }
        }

        // Determine slice type based on detected files
        // Priority: Saga > Mixed > Query > Command > Unknown
        if has_saga {
            SliceType::Saga
        } else if has_command && has_query {
            SliceType::Mixed
        } else if has_query {
            SliceType::Query
        } else if has_command {
            SliceType::Command
        } else {
            SliceType::Unknown
        }
    }

    /// Detect infrastructure folders in a context
    fn detect_infrastructure_folders(context_path: &Path) -> Vec<String> {
        let mut infrastructure = Vec::new();

        // Common infrastructure folders
        let infra_candidates =
            vec!["infrastructure", "repositories", "adapters", "services", "buses"];

        for candidate in infra_candidates {
            let candidate_path = context_path.join(candidate);
            if candidate_path.exists() && candidate_path.is_dir() {
                infrastructure.push(candidate.to_string());
            }
        }

        // Also check infrastructure subfolders
        let infrastructure_path = context_path.join("infrastructure");
        if infrastructure_path.exists() && infrastructure_path.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&infrastructure_path) {
                for entry in entries.flatten() {
                    if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                        if let Some(name) = entry.file_name().to_str() {
                            infrastructure.push(format!("infrastructure/{name}"));
                        }
                    }
                }
            }
        }

        infrastructure
    }

    /// Build relationships between domain components
    fn build_relationships(model: &DomainModel) -> Relationships {
        let mut command_to_aggregate = HashMap::new();
        let mut aggregate_to_events = HashMap::new();
        let mut event_to_handlers = HashMap::new();
        let mut event_to_projections = HashMap::new();
        let mut projection_to_read_model = HashMap::new();

        // Build command -> aggregate mapping
        for aggregate in &model.aggregates {
            let mut events_emitted = Vec::new();

            for handler in &aggregate.command_handlers {
                command_to_aggregate.insert(handler.command_type.clone(), aggregate.name.clone());

                // Collect events emitted by this command handler
                for event in &handler.emits_events {
                    if !events_emitted.contains(event) {
                        events_emitted.push(event.clone());
                    }
                }
            }

            // Store aggregate -> events mapping
            if !events_emitted.is_empty() {
                aggregate_to_events.insert(aggregate.name.clone(), events_emitted);
            }

            // Build event -> handler mapping
            for handler in &aggregate.event_handlers {
                event_to_handlers
                    .entry(handler.event_type.clone())
                    .or_insert_with(Vec::new)
                    .push(aggregate.name.clone());
            }
        }

        // Build projection relationships
        for projection in &model.projections {
            // Map events to projections
            for event_name in &projection.subscribed_events {
                event_to_projections
                    .entry(event_name.clone())
                    .or_insert_with(Vec::new)
                    .push(projection.name.clone());
            }

            // Map projection to read model
            if let Some(ref read_model) = projection.read_model {
                projection_to_read_model.insert(projection.name.clone(), read_model.clone());
            }
        }

        Relationships {
            command_to_aggregate,
            aggregate_to_events,
            event_to_handlers,
            event_to_projections,
            projection_to_read_model,
        }
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
            schema_version: MANIFEST_SCHEMA_VERSION.to_string(),
            generated_at: "2025-11-05T00:00:00Z".to_string(),
            bounded_contexts: vec![ContextManifest {
                name: "warehouse".to_string(),
                path: "/path/to/warehouse".to_string(),
                features: vec![FeatureManifest {
                    name: "create-product".to_string(),
                    path: "products/create-product".to_string(),
                    files: vec![
                        "CreateProductCommand.ts".to_string(),
                        "ProductCreatedEvent.ts".to_string(),
                    ],
                    slice_type: SliceType::Command,
                }],
                context_type: ContextType::BoundedContext,
                aggregate_count: 2,
                infrastructure_folders: vec![],
            }],
            domain: None,
        };

        let json = manifest.to_json().unwrap();
        assert!(json.contains("warehouse"));
        assert!(json.contains("create-product"));
        assert!(json.contains("schema_version"));
        assert!(json.contains("\"slice_type\": \"command\""));
        assert!(json.contains("\"context_type\": \"bounded_context\""));
        assert!(json.contains("\"aggregate_count\": 2"));
    }

    #[test]
    fn test_detect_slice_type_command() {
        let files = vec![
            "CreateOrderCommand.ts".to_string(),
            "CreateOrderHandler.ts".to_string(),
            "OrderCreatedEvent.ts".to_string(),
        ];
        assert_eq!(Manifest::detect_slice_type(&files), SliceType::Command);
    }

    #[test]
    fn test_detect_slice_type_query() {
        let files = vec![
            "ListOrdersQuery.ts".to_string(),
            "OrderListProjection.ts".to_string(),
            "ListOrdersHandler.ts".to_string(),
        ];
        assert_eq!(Manifest::detect_slice_type(&files), SliceType::Query);
    }

    #[test]
    fn test_detect_slice_type_query_projection_only() {
        // Query slice with only projection file
        let files = vec!["OrderListProjection.py".to_string(), "handler.py".to_string()];
        assert_eq!(Manifest::detect_slice_type(&files), SliceType::Query);
    }

    #[test]
    fn test_detect_slice_type_saga() {
        let files =
            vec!["OrderProcessingSaga.ts".to_string(), "OrderProcessingSagaHandler.ts".to_string()];
        assert_eq!(Manifest::detect_slice_type(&files), SliceType::Saga);
    }

    #[test]
    fn test_detect_slice_type_mixed() {
        let files = vec![
            "CreateOrderCommand.ts".to_string(),
            "GetOrderQuery.ts".to_string(),
            "OrderHandler.ts".to_string(),
        ];
        assert_eq!(Manifest::detect_slice_type(&files), SliceType::Mixed);
    }

    #[test]
    fn test_detect_slice_type_unknown() {
        let files = vec!["utils.ts".to_string(), "helpers.ts".to_string(), "index.ts".to_string()];
        assert_eq!(Manifest::detect_slice_type(&files), SliceType::Unknown);
    }

    #[test]
    fn test_detect_slice_type_ignores_test_files() {
        // Test files should not affect slice type detection
        let files = vec![
            "CreateOrderCommand.test.ts".to_string(),
            "test_create_order.py".to_string(),
            "utils.ts".to_string(),
        ];
        // Even though test files mention Command, they should be ignored
        assert_eq!(Manifest::detect_slice_type(&files), SliceType::Unknown);
    }

    #[test]
    fn test_detect_slice_type_python_files() {
        let files =
            vec!["CreateOrderCommand.py".to_string(), "create_order_handler.py".to_string()];
        assert_eq!(Manifest::detect_slice_type(&files), SliceType::Command);
    }

    #[test]
    fn test_detect_slice_type_rust_files() {
        let files = vec![
            "list_orders_query.rs".to_string(), // Note: doesn't end with Query
            "OrderProjection.rs".to_string(),
        ];
        assert_eq!(Manifest::detect_slice_type(&files), SliceType::Query);
    }

    #[test]
    fn test_manifest_with_domain() {
        use crate::domain::*;

        let domain_model = DomainModel {
            aggregates: vec![Aggregate {
                name: "TaskAggregate".to_string(),
                context: None,
                file_path: std::path::PathBuf::from("domain/TaskAggregate.ts"),
                line_count: 100,
                command_handlers: vec![CommandHandler {
                    command_type: "CreateTaskCommand".to_string(),
                    method_name: "handle".to_string(),
                    line_number: 10,
                    emits_events: vec!["TaskCreatedEvent".to_string()],
                }],
                event_handlers: vec![EventHandler {
                    event_type: "TaskCreatedEvent".to_string(),
                    method_name: "onCreated".to_string(),
                    line_number: 20,
                }],
                entities: vec![],
                value_objects: vec![],
                folder_name: None,
            }],
            commands: vec![Command {
                name: "CreateTaskCommand".to_string(),
                context: None,
                file_path: std::path::PathBuf::from("domain/commands/CreateTaskCommand.ts"),
                has_aggregate_id: true,
                fields: vec![],
            }],
            events: vec![Event {
                name: "TaskCreatedEvent".to_string(),
                context: None,
                event_type: "TaskCreated".to_string(),
                version: EventVersion::Simple("v1".to_string()),
                file_path: std::path::PathBuf::from("domain/events/TaskCreatedEvent.ts"),
                fields: vec![],
                decorator_present: true,
            }],
            queries: vec![],
            projections: vec![],
            upcasters: vec![],
            value_objects: vec![],
            root_path: std::path::PathBuf::from("/test"),
        };

        let domain_manifest = Manifest::build_domain_manifest(&domain_model);

        // Check that aggregates were included
        assert_eq!(domain_manifest.aggregates.len(), 1);
        assert_eq!(domain_manifest.aggregates[0].name, "TaskAggregate");

        // Check that commands were included
        assert_eq!(domain_manifest.commands.len(), 1);
        assert_eq!(domain_manifest.commands[0].name, "CreateTaskCommand");

        // Check that events were included
        assert_eq!(domain_manifest.events.len(), 1);
        assert_eq!(domain_manifest.events[0].name, "TaskCreatedEvent");

        // Check relationships
        assert_eq!(
            domain_manifest.relationships.command_to_aggregate.get("CreateTaskCommand"),
            Some(&"TaskAggregate".to_string())
        );
        assert_eq!(
            domain_manifest.relationships.aggregate_to_events.get("TaskAggregate"),
            Some(&vec!["TaskCreatedEvent".to_string()])
        );
        assert_eq!(
            domain_manifest.relationships.event_to_handlers.get("TaskCreatedEvent"),
            Some(&vec!["TaskAggregate".to_string()])
        );
    }

    #[test]
    fn test_build_relationships() {
        use crate::domain::*;

        let domain_model = DomainModel {
            aggregates: vec![Aggregate {
                name: "CartAggregate".to_string(),
                context: None,
                file_path: std::path::PathBuf::from("domain/CartAggregate.ts"),
                line_count: 200,
                command_handlers: vec![
                    CommandHandler {
                        command_type: "AddItemCommand".to_string(),
                        method_name: "addItem".to_string(),
                        line_number: 10,
                        emits_events: vec![
                            "ItemAddedEvent".to_string(),
                            "CartCreatedEvent".to_string(),
                        ],
                    },
                    CommandHandler {
                        command_type: "RemoveItemCommand".to_string(),
                        method_name: "removeItem".to_string(),
                        line_number: 30,
                        emits_events: vec!["ItemRemovedEvent".to_string()],
                    },
                ],
                event_handlers: vec![
                    EventHandler {
                        event_type: "ItemAddedEvent".to_string(),
                        method_name: "onItemAdded".to_string(),
                        line_number: 50,
                    },
                    EventHandler {
                        event_type: "ItemRemovedEvent".to_string(),
                        method_name: "onItemRemoved".to_string(),
                        line_number: 60,
                    },
                ],
                entities: vec![],
                value_objects: vec![],
                folder_name: None,
            }],
            commands: vec![],
            events: vec![],
            queries: vec![],
            projections: vec![],
            upcasters: vec![],
            value_objects: vec![],
            root_path: std::path::PathBuf::from("/test"),
        };

        let relationships = Manifest::build_relationships(&domain_model);

        // Check command -> aggregate mapping
        assert_eq!(
            relationships.command_to_aggregate.get("AddItemCommand"),
            Some(&"CartAggregate".to_string())
        );
        assert_eq!(
            relationships.command_to_aggregate.get("RemoveItemCommand"),
            Some(&"CartAggregate".to_string())
        );

        // Check aggregate -> events mapping
        let cart_events = relationships.aggregate_to_events.get("CartAggregate").unwrap();
        assert_eq!(cart_events.len(), 3);
        assert!(cart_events.contains(&"ItemAddedEvent".to_string()));
        assert!(cart_events.contains(&"CartCreatedEvent".to_string()));
        assert!(cart_events.contains(&"ItemRemovedEvent".to_string()));

        // Check event -> handlers mapping
        assert_eq!(
            relationships.event_to_handlers.get("ItemAddedEvent"),
            Some(&vec!["CartAggregate".to_string()])
        );
        assert_eq!(
            relationships.event_to_handlers.get("ItemRemovedEvent"),
            Some(&vec!["CartAggregate".to_string()])
        );
    }
}
