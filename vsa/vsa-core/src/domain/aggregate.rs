//! Aggregate metadata

use std::path::PathBuf;

/// Metadata for a domain aggregate
#[derive(Debug, Clone, PartialEq)]
pub struct Aggregate {
    /// Name of the aggregate (e.g., "TaskAggregate", "CartAggregate")
    pub name: String,

    /// File path relative to project root
    pub file_path: PathBuf,

    /// Total line count (for metrics)
    pub line_count: usize,

    /// Command handlers defined in this aggregate
    pub command_handlers: Vec<CommandHandler>,

    /// Event sourcing handlers defined in this aggregate
    pub event_handlers: Vec<EventHandler>,
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
#[derive(Debug, Clone, PartialEq)]
pub struct CommandHandler {
    /// Command type this handler processes (e.g., "CreateTaskCommand")
    pub command_type: String,

    /// Method name (e.g., "handle", "handleCreateTask")
    pub method_name: String,

    /// Line number in the file
    pub line_number: usize,
}

/// Metadata for an event sourcing handler method
#[derive(Debug, Clone, PartialEq)]
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
            file_path: PathBuf::from("domain/TaskAggregate.ts"),
            line_count: 150,
            command_handlers: vec![
                CommandHandler {
                    command_type: "CreateTaskCommand".to_string(),
                    method_name: "handle".to_string(),
                    line_number: 25,
                },
                CommandHandler {
                    command_type: "CompleteTaskCommand".to_string(),
                    method_name: "handle".to_string(),
                    line_number: 45,
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
            file_path: PathBuf::from("domain/EmptyAggregate.ts"),
            line_count: 10,
            command_handlers: vec![],
            event_handlers: vec![],
        };

        assert_eq!(aggregate.handler_count(), 0);
        assert!(!aggregate.handles_command("AnyCommand"));
        assert!(!aggregate.handles_event("AnyEvent"));
    }
}
