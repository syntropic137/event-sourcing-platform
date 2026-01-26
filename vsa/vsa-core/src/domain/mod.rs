//! Domain metadata structures
//!
//! This module contains metadata types for domain layer components:
//! - Aggregates
//! - Commands
//! - Queries
//! - Events
//! - Upcasters
//! - Projections (read models for CQRS)
//! - Value Objects

pub mod aggregate;
pub mod command;
pub mod event;
pub mod projection;
pub mod query;
pub mod upcaster;
pub mod value_object;

pub use aggregate::{Aggregate, CommandHandler, EventHandler};
pub use command::{Command, CommandField};
pub use event::{Event, EventField, EventVersion};
pub use projection::Projection;
pub use query::{Query, QueryField};
pub use upcaster::Upcaster;
pub use value_object::{ValueObject, ValueObjectField};

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Complete domain model containing all domain components
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainModel {
    pub aggregates: Vec<Aggregate>,
    pub commands: Vec<Command>,
    pub queries: Vec<Query>,
    pub events: Vec<Event>,
    pub projections: Vec<Projection>,
    pub upcasters: Vec<Upcaster>,
    pub value_objects: Vec<ValueObject>,
    pub root_path: PathBuf,
}

impl DomainModel {
    /// Create a new empty domain model
    pub fn new(root_path: PathBuf) -> Self {
        Self {
            aggregates: Vec::new(),
            commands: Vec::new(),
            queries: Vec::new(),
            events: Vec::new(),
            projections: Vec::new(),
            upcasters: Vec::new(),
            value_objects: Vec::new(),
            root_path,
        }
    }

    /// Get total count of all domain components
    pub fn component_count(&self) -> usize {
        self.aggregates.len()
            + self.commands.len()
            + self.queries.len()
            + self.events.len()
            + self.projections.len()
            + self.upcasters.len()
            + self.value_objects.len()
    }

    /// Find an aggregate by name
    pub fn find_aggregate(&self, name: &str) -> Option<&Aggregate> {
        self.aggregates.iter().find(|a| a.name == name)
    }

    /// Find an event by type and version
    pub fn find_event(&self, event_type: &str, version: &str) -> Option<&Event> {
        self.events.iter().find(|e| e.event_type == event_type && e.version.as_str() == version)
    }

    /// Find upcasters for a specific event type
    pub fn find_upcasters_for_event(&self, event_type: &str) -> Vec<&Upcaster> {
        self.upcasters.iter().filter(|u| u.event_type == event_type).collect()
    }

    /// Get all event versions for a specific event type
    pub fn get_event_versions(&self, event_type: &str) -> Vec<&str> {
        let mut versions: Vec<&str> = self
            .events
            .iter()
            .filter(|e| e.event_type == event_type)
            .map(|e| e.version.as_str())
            .collect();
        versions.sort();
        versions
    }

    /// Merge another domain model into this one
    ///
    /// This is useful for multi-context architectures where each bounded context
    /// has its own domain folder. All components from the other model are added
    /// to this model. Duplicates are handled by keeping both (assuming they're
    /// from different contexts).
    pub fn merge(&mut self, other: DomainModel) {
        self.aggregates.extend(other.aggregates);
        self.commands.extend(other.commands);
        self.queries.extend(other.queries);
        self.events.extend(other.events);
        self.projections.extend(other.projections);
        self.upcasters.extend(other.upcasters);
        self.value_objects.extend(other.value_objects);
    }

    /// Get unique contexts present in the domain model
    pub fn get_contexts(&self) -> Vec<String> {
        let mut contexts = std::collections::HashSet::new();

        for agg in &self.aggregates {
            if let Some(ctx) = &agg.context {
                contexts.insert(ctx.clone());
            }
        }
        for cmd in &self.commands {
            if let Some(ctx) = &cmd.context {
                contexts.insert(ctx.clone());
            }
        }
        for evt in &self.events {
            if let Some(ctx) = &evt.context {
                contexts.insert(ctx.clone());
            }
        }

        let mut result: Vec<String> = contexts.into_iter().collect();
        result.sort();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_model_creation() {
        let model = DomainModel::new(PathBuf::from("/test"));
        assert_eq!(model.component_count(), 0);
        assert_eq!(model.root_path, PathBuf::from("/test"));
    }

    #[test]
    fn test_find_aggregate() {
        let mut model = DomainModel::new(PathBuf::from("/test"));
        model.aggregates.push(Aggregate {
            name: "TaskAggregate".to_string(),
            context: None,
            file_path: PathBuf::from("domain/TaskAggregate.ts"),
            line_count: 100,
            command_handlers: vec![],
            event_handlers: vec![],
        });

        assert!(model.find_aggregate("TaskAggregate").is_some());
        assert!(model.find_aggregate("NonExistent").is_none());
    }

    #[test]
    fn test_find_event() {
        let mut model = DomainModel::new(PathBuf::from("/test"));
        model.events.push(Event {
            name: "TaskCreatedEvent".to_string(),
            context: None,
            event_type: "TaskCreated".to_string(),
            version: EventVersion::Simple("v1".to_string()),
            file_path: PathBuf::from("domain/events/TaskCreatedEvent.ts"),
            fields: vec![],
            decorator_present: true,
        });

        assert!(model.find_event("TaskCreated", "v1").is_some());
        assert!(model.find_event("TaskCreated", "v2").is_none());
        assert!(model.find_event("NonExistent", "v1").is_none());
    }

    #[test]
    fn test_get_event_versions() {
        let mut model = DomainModel::new(PathBuf::from("/test"));

        // Add multiple versions of the same event
        model.events.push(Event {
            name: "TaskCreatedEvent".to_string(),
            context: None,
            event_type: "TaskCreated".to_string(),
            version: EventVersion::Simple("v1".to_string()),
            file_path: PathBuf::from("domain/events/_versioned/TaskCreatedEvent.v1.ts"),
            fields: vec![],
            decorator_present: true,
        });

        model.events.push(Event {
            name: "TaskCreatedEvent".to_string(),
            context: None,
            event_type: "TaskCreated".to_string(),
            version: EventVersion::Simple("v2".to_string()),
            file_path: PathBuf::from("domain/events/TaskCreatedEvent.ts"),
            fields: vec![],
            decorator_present: true,
        });

        let versions = model.get_event_versions("TaskCreated");
        assert_eq!(versions, vec!["v1", "v2"]);
    }

    #[test]
    fn test_merge_empty_models() {
        let mut model1 = DomainModel::new(PathBuf::from("/test"));
        let model2 = DomainModel::new(PathBuf::from("/test"));

        model1.merge(model2);
        assert_eq!(model1.component_count(), 0);
    }

    #[test]
    fn test_merge_non_overlapping_models() {
        let mut model1 = DomainModel::new(PathBuf::from("/test"));
        model1.aggregates.push(Aggregate {
            name: "TaskAggregate".to_string(),
            context: Some("tasks".to_string()),
            file_path: PathBuf::from("contexts/tasks/domain/TaskAggregate.ts"),
            line_count: 100,
            command_handlers: vec![],
            event_handlers: vec![],
        });

        let mut model2 = DomainModel::new(PathBuf::from("/test"));
        model2.aggregates.push(Aggregate {
            name: "OrderAggregate".to_string(),
            context: Some("orders".to_string()),
            file_path: PathBuf::from("contexts/orders/domain/OrderAggregate.ts"),
            line_count: 150,
            command_handlers: vec![],
            event_handlers: vec![],
        });

        model1.merge(model2);
        assert_eq!(model1.aggregates.len(), 2);
        assert!(model1.find_aggregate("TaskAggregate").is_some());
        assert!(model1.find_aggregate("OrderAggregate").is_some());
    }

    #[test]
    fn test_merge_with_duplicate_names_different_contexts() {
        // It's valid to have same-named aggregates in different contexts
        let mut model1 = DomainModel::new(PathBuf::from("/test"));
        model1.aggregates.push(Aggregate {
            name: "ItemAggregate".to_string(),
            context: Some("catalog".to_string()),
            file_path: PathBuf::from("contexts/catalog/domain/ItemAggregate.ts"),
            line_count: 100,
            command_handlers: vec![],
            event_handlers: vec![],
        });

        let mut model2 = DomainModel::new(PathBuf::from("/test"));
        model2.aggregates.push(Aggregate {
            name: "ItemAggregate".to_string(),
            context: Some("inventory".to_string()),
            file_path: PathBuf::from("contexts/inventory/domain/ItemAggregate.ts"),
            line_count: 120,
            command_handlers: vec![],
            event_handlers: vec![],
        });

        model1.merge(model2);
        assert_eq!(model1.aggregates.len(), 2);

        // Both should exist with their respective contexts
        let catalog_item =
            model1.aggregates.iter().find(|a| a.context == Some("catalog".to_string()));
        let inventory_item =
            model1.aggregates.iter().find(|a| a.context == Some("inventory".to_string()));

        assert!(catalog_item.is_some());
        assert!(inventory_item.is_some());
    }

    #[test]
    fn test_merge_preserves_context_info() {
        let mut model1 = DomainModel::new(PathBuf::from("/test"));
        let mut model2 = DomainModel::new(PathBuf::from("/test"));

        model2.aggregates.push(Aggregate {
            name: "WorkspaceAggregate".to_string(),
            context: Some("workspaces".to_string()),
            file_path: PathBuf::from("contexts/workspaces/domain/WorkspaceAggregate.py"),
            line_count: 200,
            command_handlers: vec![],
            event_handlers: vec![],
        });
        model2.commands.push(Command {
            name: "CreateWorkspaceCommand".to_string(),
            context: Some("workspaces".to_string()),
            file_path: PathBuf::from(
                "contexts/workspaces/domain/commands/CreateWorkspaceCommand.py",
            ),
            has_aggregate_id: true,
            fields: vec![],
        });
        model2.events.push(Event {
            name: "WorkspaceCreatedEvent".to_string(),
            context: Some("workspaces".to_string()),
            event_type: "WorkspaceCreated".to_string(),
            version: EventVersion::Simple("v1".to_string()),
            file_path: PathBuf::from("contexts/workspaces/domain/events/WorkspaceCreatedEvent.py"),
            fields: vec![],
            decorator_present: true,
        });

        model1.merge(model2);

        assert_eq!(model1.aggregates.len(), 1);
        assert_eq!(model1.commands.len(), 1);
        assert_eq!(model1.events.len(), 1);

        assert_eq!(model1.aggregates[0].context, Some("workspaces".to_string()));
        assert_eq!(model1.commands[0].context, Some("workspaces".to_string()));
        assert_eq!(model1.events[0].context, Some("workspaces".to_string()));
    }

    #[test]
    fn test_get_contexts() {
        let mut model = DomainModel::new(PathBuf::from("/test"));

        model.aggregates.push(Aggregate {
            name: "TaskAggregate".to_string(),
            context: Some("tasks".to_string()),
            file_path: PathBuf::from("contexts/tasks/domain/TaskAggregate.ts"),
            line_count: 100,
            command_handlers: vec![],
            event_handlers: vec![],
        });

        model.aggregates.push(Aggregate {
            name: "OrderAggregate".to_string(),
            context: Some("orders".to_string()),
            file_path: PathBuf::from("contexts/orders/domain/OrderAggregate.ts"),
            line_count: 150,
            command_handlers: vec![],
            event_handlers: vec![],
        });

        model.commands.push(Command {
            name: "CreatePaymentCommand".to_string(),
            context: Some("payments".to_string()),
            file_path: PathBuf::from("contexts/payments/domain/commands/CreatePaymentCommand.ts"),
            has_aggregate_id: true,
            fields: vec![],
        });

        let contexts = model.get_contexts();
        assert_eq!(contexts, vec!["orders", "payments", "tasks"]);
    }

    #[test]
    fn test_get_contexts_with_none() {
        let mut model = DomainModel::new(PathBuf::from("/test"));

        // Mix of aggregates with and without context
        model.aggregates.push(Aggregate {
            name: "TaskAggregate".to_string(),
            context: Some("tasks".to_string()),
            file_path: PathBuf::from("domain/TaskAggregate.ts"),
            line_count: 100,
            command_handlers: vec![],
            event_handlers: vec![],
        });

        model.aggregates.push(Aggregate {
            name: "LegacyAggregate".to_string(),
            context: None,
            file_path: PathBuf::from("domain/LegacyAggregate.ts"),
            line_count: 50,
            command_handlers: vec![],
            event_handlers: vec![],
        });

        let contexts = model.get_contexts();
        assert_eq!(contexts, vec!["tasks"]);
    }
}
