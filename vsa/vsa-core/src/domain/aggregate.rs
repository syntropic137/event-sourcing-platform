//! Aggregate metadata

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Metadata for a domain aggregate
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Aggregate {
    /// Name of the aggregate (e.g., "TaskAggregate", "CartAggregate")
    pub name: String,

    /// Bounded context this aggregate belongs to (optional for monolithic projects)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,

    /// File path relative to project root
    pub file_path: PathBuf,

    /// Total line count (for metrics)
    pub line_count: usize,

    /// Command handlers defined in this aggregate
    pub command_handlers: Vec<CommandHandler>,

    /// Event sourcing handlers defined in this aggregate
    pub event_handlers: Vec<EventHandler>,

    /// Entities within this aggregate (objects with identity)
    /// NEW in v2.3.0 - Aggregate relationship visualization
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub entities: Vec<AggregateEntity>,

    /// Value objects within this aggregate (immutable, no identity)
    /// NEW in v2.3.0 - Aggregate relationship visualization
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub value_objects: Vec<AggregateValueObject>,

    /// Whether this aggregate was found in an aggregate_* folder
    /// This helps validate proper DDD folder structure
    #[serde(skip_serializing_if = "Option::is_none")]
    pub folder_name: Option<String>,
}

/// An entity within an aggregate (has identity)
/// Entities are objects with a unique identity that persists through their lifecycle.
/// They are accessed only through the aggregate root.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AggregateEntity {
    /// Name of the entity class (e.g., "IsolationHandle", "PhaseExecution")
    pub name: String,

    /// The identity field that gives this entity its identity (e.g., "isolation_id", "phase_id")
    pub identity_field: Option<String>,

    /// File path relative to aggregate folder
    pub file_path: PathBuf,

    /// Line count for metrics
    pub line_count: usize,
}

/// A value object within an aggregate (no identity, immutable)
/// Value objects are defined by their attributes, not by an identity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AggregateValueObject {
    /// Name of the value object class (e.g., "SecurityPolicy", "ExecutionStatus")
    pub name: String,

    /// File path relative to aggregate folder
    pub file_path: PathBuf,

    /// Whether the value object is immutable (frozen=True, readonly, etc.)
    pub is_immutable: bool,

    /// Line count for metrics
    pub line_count: usize,
}

impl Aggregate {
    /// Check if this aggregate handles a specific command
    pub fn handles_command(&self, command_type: &str) -> bool {
        self.command_handlers.iter().any(|h| h.command_type == command_type)
    }

    /// Check if this aggregate handles a specific event
    pub fn handles_event(&self, event_type: &str) -> bool {
        self.event_handlers.iter().any(|h| h.event_type == event_type)
    }

    /// Get total number of handlers
    pub fn handler_count(&self) -> usize {
        self.command_handlers.len() + self.event_handlers.len()
    }
}

/// Metadata for a command handler method
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandHandler {
    /// Command type this handler processes (e.g., "CreateTaskCommand")
    pub command_type: String,

    /// Method name (e.g., "handle", "handleCreateTask")
    pub method_name: String,

    /// Line number in the file
    pub line_number: usize,

    /// Events emitted by this command handler
    #[serde(default)]
    pub emits_events: Vec<String>,
}

/// Metadata for an event sourcing handler method
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventHandler {
    /// Event type this handler processes (e.g., "TaskCreatedEvent")
    pub event_type: String,

    /// Method name (e.g., "on", "onTaskCreated")
    pub method_name: String,

    /// Line number in the file
    pub line_number: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_aggregate() -> Aggregate {
        Aggregate {
            name: "TaskAggregate".to_string(),
            context: None,
            file_path: PathBuf::from("domain/TaskAggregate.ts"),
            line_count: 150,
            command_handlers: vec![
                CommandHandler {
                    command_type: "CreateTaskCommand".to_string(),
                    method_name: "handle".to_string(),
                    line_number: 25,
                    emits_events: vec!["TaskCreatedEvent".to_string()],
                },
                CommandHandler {
                    command_type: "CompleteTaskCommand".to_string(),
                    method_name: "handle".to_string(),
                    line_number: 45,
                    emits_events: vec!["TaskCompletedEvent".to_string()],
                },
            ],
            event_handlers: vec![
                EventHandler {
                    event_type: "TaskCreatedEvent".to_string(),
                    method_name: "on".to_string(),
                    line_number: 70,
                },
                EventHandler {
                    event_type: "TaskCompletedEvent".to_string(),
                    method_name: "on".to_string(),
                    line_number: 85,
                },
            ],
            entities: vec![],
            value_objects: vec![],
            folder_name: None,
        }
    }

    #[test]
    fn test_handles_command() {
        let aggregate = create_test_aggregate();

        assert!(aggregate.handles_command("CreateTaskCommand"));
        assert!(aggregate.handles_command("CompleteTaskCommand"));
        assert!(!aggregate.handles_command("DeleteTaskCommand"));
    }

    #[test]
    fn test_handles_event() {
        let aggregate = create_test_aggregate();

        assert!(aggregate.handles_event("TaskCreatedEvent"));
        assert!(aggregate.handles_event("TaskCompletedEvent"));
        assert!(!aggregate.handles_event("TaskDeletedEvent"));
    }

    #[test]
    fn test_handler_count() {
        let aggregate = create_test_aggregate();
        assert_eq!(aggregate.handler_count(), 4); // 2 command + 2 event
    }

    #[test]
    fn test_empty_aggregate() {
        let aggregate = Aggregate {
            name: "EmptyAggregate".to_string(),
            context: None,
            file_path: PathBuf::from("domain/EmptyAggregate.ts"),
            line_count: 10,
            command_handlers: vec![],
            event_handlers: vec![],
            entities: vec![],
            value_objects: vec![],
            folder_name: None,
        };

        assert_eq!(aggregate.handler_count(), 0);
        assert!(!aggregate.handles_command("AnyCommand"));
        assert!(!aggregate.handles_event("AnyEvent"));
    }

    #[test]
    fn test_aggregate_with_entities_and_value_objects() {
        let aggregate = Aggregate {
            name: "WorkspaceAggregate".to_string(),
            context: Some("orchestration".to_string()),
            file_path: PathBuf::from("domain/aggregate_workspace/WorkspaceAggregate.py"),
            line_count: 200,
            command_handlers: vec![],
            event_handlers: vec![],
            entities: vec![AggregateEntity {
                name: "IsolationHandle".to_string(),
                identity_field: Some("isolation_id".to_string()),
                file_path: PathBuf::from("domain/aggregate_workspace/IsolationHandle.py"),
                line_count: 50,
            }],
            value_objects: vec![
                AggregateValueObject {
                    name: "SecurityPolicy".to_string(),
                    file_path: PathBuf::from("domain/aggregate_workspace/SecurityPolicy.py"),
                    is_immutable: true,
                    line_count: 30,
                },
                AggregateValueObject {
                    name: "ExecutionResult".to_string(),
                    file_path: PathBuf::from("domain/aggregate_workspace/ExecutionResult.py"),
                    is_immutable: true,
                    line_count: 25,
                },
            ],
            folder_name: Some("aggregate_workspace".to_string()),
        };

        assert_eq!(aggregate.entities.len(), 1);
        assert_eq!(aggregate.value_objects.len(), 2);
        assert_eq!(aggregate.folder_name, Some("aggregate_workspace".to_string()));
        assert_eq!(aggregate.entities[0].identity_field, Some("isolation_id".to_string()));
    }
}
