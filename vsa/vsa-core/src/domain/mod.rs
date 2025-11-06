//! Domain metadata structures
//!
//! This module contains metadata types for domain layer components:
//! - Aggregates
//! - Commands
//! - Queries
//! - Events
//! - Upcasters

pub mod aggregate;
pub mod command;
pub mod event;
pub mod query;
pub mod upcaster;

pub use aggregate::{Aggregate, CommandHandler, EventHandler};
pub use command::{Command, CommandField};
pub use event::{Event, EventField, EventVersion};
pub use query::{Query, QueryField};
pub use upcaster::Upcaster;

use std::path::PathBuf;

/// Complete domain model containing all domain components
#[derive(Debug, Clone)]
pub struct DomainModel {
    pub aggregates: Vec<Aggregate>,
    pub commands: Vec<Command>,
    pub queries: Vec<Query>,
    pub events: Vec<Event>,
    pub upcasters: Vec<Upcaster>,
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
            upcasters: Vec::new(),
            root_path,
        }
    }

    /// Get total count of all domain components
    pub fn component_count(&self) -> usize {
        self.aggregates.len()
            + self.commands.len()
            + self.queries.len()
            + self.events.len()
            + self.upcasters.len()
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
            event_type: "TaskCreated".to_string(),
            version: EventVersion::Simple("v1".to_string()),
            file_path: PathBuf::from("domain/events/_versioned/TaskCreatedEvent.v1.ts"),
            fields: vec![],
            decorator_present: true,
        });

        model.events.push(Event {
            name: "TaskCreatedEvent".to_string(),
            event_type: "TaskCreated".to_string(),
            version: EventVersion::Simple("v2".to_string()),
            file_path: PathBuf::from("domain/events/TaskCreatedEvent.ts"),
            fields: vec![],
            decorator_present: true,
        });

        let versions = model.get_event_versions("TaskCreated");
        assert_eq!(versions, vec!["v1", "v2"]);
    }
}
