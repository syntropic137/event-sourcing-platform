//! Manifest generation

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::config::VsaConfig;
use crate::domain::DomainModel;
use crate::error::Result;
use crate::scanner::Scanner;
use crate::scanners::DomainScanner;

/// VSA manifest schema version
pub const MANIFEST_SCHEMA_VERSION: &str = "1.1.0";

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
    /// Infrastructure folders (repositories, buses, etc.) - NEW in v1.1.0
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub infrastructure_folders: Vec<String>,
}

/// Feature manifest entry
#[derive(Debug, Serialize, Deserialize)]
pub struct FeatureManifest {
    pub name: String,
    pub path: String,
    pub files: Vec<String>,
}

/// Domain manifest containing all domain model components
#[derive(Debug, Serialize, Deserialize)]
pub struct DomainManifest {
    pub aggregates: Vec<crate::domain::Aggregate>,
    pub commands: Vec<crate::domain::Command>,
    pub events: Vec<crate::domain::Event>,
    pub queries: Vec<crate::domain::Query>,
    pub upcasters: Vec<crate::domain::Upcaster>,
    /// Value objects in the domain - NEW in v1.1.0
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

            // Detect infrastructure folders
            let infrastructure_folders = Self::detect_infrastructure_folders(&context.path);

            context_manifests.push(ContextManifest {
                name: context.name.clone(),
                path: context.path.to_string_lossy().to_string(),
                features: feature_manifests,
                infrastructure_folders,
            });
        }

        // Optionally scan domain model
        let domain = if include_domain && config.domain.is_some() {
            let domain_config = config.domain.as_ref().unwrap();
            let domain_scanner = DomainScanner::new(domain_config.clone(), root);
            let domain_model = domain_scanner.scan()?;
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

    /// Build domain manifest from domain model
    fn build_domain_manifest(model: &DomainModel) -> DomainManifest {
        let relationships = Self::build_relationships(model);

        DomainManifest {
            aggregates: model.aggregates.clone(),
            commands: model.commands.clone(),
            events: model.events.clone(),
            queries: model.queries.clone(),
            upcasters: model.upcasters.clone(),
            value_objects: model.value_objects.clone(),
            relationships,
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

        Relationships { command_to_aggregate, aggregate_to_events, event_to_handlers }
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
                }],
                infrastructure_folders: vec![],
            }],
            domain: None,
        };

        let json = manifest.to_json().unwrap();
        assert!(json.contains("warehouse"));
        assert!(json.contains("create-product"));
        assert!(json.contains("schema_version"));
    }

    #[test]
    fn test_manifest_with_domain() {
        use crate::domain::*;

        let domain_model = DomainModel {
            aggregates: vec![Aggregate {
                name: "TaskAggregate".to_string(),
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
            }],
            commands: vec![Command {
                name: "CreateTaskCommand".to_string(),
                file_path: std::path::PathBuf::from("domain/commands/CreateTaskCommand.ts"),
                has_aggregate_id: true,
                fields: vec![],
            }],
            events: vec![Event {
                name: "TaskCreatedEvent".to_string(),
                event_type: "TaskCreated".to_string(),
                version: EventVersion::Simple("v1".to_string()),
                file_path: std::path::PathBuf::from("domain/events/TaskCreatedEvent.ts"),
                fields: vec![],
                decorator_present: true,
            }],
            queries: vec![],
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
            }],
            commands: vec![],
            events: vec![],
            queries: vec![],
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
