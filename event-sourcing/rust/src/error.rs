//! Error types for the event sourcing SDK

use thiserror::Error;

/// Result type for the event sourcing SDK
pub type Result<T> = std::result::Result<T, Error>;

/// Error types that can occur in the event sourcing SDK
#[derive(Error, Debug)]
pub enum Error {
    /// Event store communication errors
    #[error("Event store error: {0}")]
    EventStore(Box<tonic::Status>),

    /// Aggregate not found
    #[error("Aggregate not found: {aggregate_type}:{aggregate_id}")]
    AggregateNotFound {
        aggregate_type: String,
        aggregate_id: String,
    },

    /// Concurrency conflict when saving aggregate
    #[error("Concurrency conflict: expected version {expected}, got {actual}")]
    ConcurrencyConflict { expected: u64, actual: u64 },

    /// Invalid aggregate state for command
    #[error("Invalid aggregate state: {message}")]
    InvalidAggregateState { message: String },

    /// Event deserialization error
    #[error("Event deserialization error: {0}")]
    EventDeserialization(#[from] serde_json::Error),

    /// Invalid command
    #[error("Invalid command: {message}")]
    InvalidCommand { message: String },

    /// Repository error
    #[error("Repository error: {0}")]
    Repository(#[from] anyhow::Error),

    /// Generic domain error
    #[error("Domain error: {message}")]
    Domain { message: String },
}

impl Error {
    /// Create a new aggregate not found error
    pub fn aggregate_not_found(aggregate_type: &str, aggregate_id: &str) -> Self {
        Self::AggregateNotFound {
            aggregate_type: aggregate_type.to_string(),
            aggregate_id: aggregate_id.to_string(),
        }
    }

    /// Create a new concurrency conflict error
    pub fn concurrency_conflict(expected: u64, actual: u64) -> Self {
        Self::ConcurrencyConflict { expected, actual }
    }

    /// Create a new invalid aggregate state error
    pub fn invalid_state(message: impl Into<String>) -> Self {
        Self::InvalidAggregateState {
            message: message.into(),
        }
    }

    /// Create a new invalid command error
    pub fn invalid_command(message: impl Into<String>) -> Self {
        Self::InvalidCommand {
            message: message.into(),
        }
    }

    /// Create a new domain error
    pub fn domain(message: impl Into<String>) -> Self {
        Self::Domain {
            message: message.into(),
        }
    }
}

impl From<tonic::Status> for Error {
    fn from(status: tonic::Status) -> Self {
        Error::EventStore(Box::new(status))
    }
}
