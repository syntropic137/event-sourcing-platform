//! Error types for the event sourcing SDK

use thiserror::Error;

/// Result type for the event sourcing SDK
pub type Result<T> = std::result::Result<T, Error>;

/// Error types that can occur in the event sourcing SDK
#[derive(Error, Debug)]
pub enum Error {
    /// Event store communication errors (gRPC status from the store or transport).
    ///
    /// Use [`Error::is_transient`] to decide whether the call may be retried and
    /// whether the outcome of a write is unknown.
    #[error("Event store error: {0}")]
    EventStore(Box<tonic::Status>),

    /// Aggregate not found
    #[error("Aggregate not found: {aggregate_type}:{aggregate_id}")]
    AggregateNotFound {
        aggregate_type: String,
        aggregate_id: String,
    },

    /// Optimistic concurrency conflict when saving an aggregate.
    ///
    /// `expected` is the stream revision the writer based its decision on;
    /// `actual` is the store's current revision (0 when the store did not
    /// report it). The writer's state is stale: reload and re-run the command.
    #[error("Concurrency conflict: expected version {expected}, got {actual}")]
    ConcurrencyConflict { expected: u64, actual: u64 },

    /// Invalid aggregate state for command
    #[error("Invalid aggregate state: {message}")]
    InvalidAggregateState { message: String },

    /// Event deserialization error
    #[error("Event deserialization error: {0}")]
    EventDeserialization(#[from] serde_json::Error),

    /// No decoder for this `event_type`. Never skipped silently: register the
    /// type with the event enum, or an upcaster that maps it to a known one.
    #[error("Unknown event type '{event_type}' (version {event_version})")]
    UnknownEventType {
        event_type: String,
        event_version: u32,
    },

    /// The `event_type` is known but not at this `event_version`, and no
    /// upcaster maps it to a known version (for example an event written by
    /// newer code).
    #[error("Unsupported version {event_version} of event type '{event_type}'")]
    UnknownEventVersion {
        event_type: String,
        event_version: u32,
    },

    /// The payload does not match the schema registered for its
    /// `event_type` and `event_version`.
    #[error("Cannot decode event '{event_type}' v{event_version}: {source}")]
    EventDecode {
        event_type: String,
        event_version: u32,
        source: serde_json::Error,
    },

    /// The stored payload is not JSON (`content_type` is set and is not
    /// `application/json`).
    #[error("Unsupported content type '{content_type}' for event '{event_type}'")]
    UnsupportedContentType {
        event_type: String,
        content_type: String,
    },

    /// An upcaster failed, cycled, or produced a non-object payload.
    #[error("Upcasting event '{event_type}' v{event_version} failed: {reason}")]
    Upcast {
        event_type: String,
        event_version: u32,
        reason: String,
    },

    /// An event cannot be written in the cross-language envelope (invalid
    /// `event_type`, `event_version` 0, or a payload that is not a JSON
    /// object). See ADR-026.
    #[error("Invalid event: {message}")]
    InvalidEvent { message: String },

    /// Invalid command
    #[error("Invalid command: {message}")]
    InvalidCommand { message: String },

    /// A projection handler failed; its checkpoint was not advanced.
    #[error("Projection {projection} failed at global nonce {global_nonce}: {source}")]
    ProjectionFailed {
        /// Checkpoint key of the projection (`tenant/name@vN[feed]`).
        projection: String,
        /// Position of the event that failed.
        global_nonce: u64,
        /// Handler error.
        source: Box<Error>,
    },

    /// A subscription delivered a live event below the last applied position
    /// that cannot be proven to be a duplicate. The event store must deliver
    /// live events in global-nonce commit order (`commit_ordered_global_nonce`,
    /// see #366); skipping it could lose data, so the runner stops.
    #[error(
        "Projection {projection} received live event {received} after {last_applied}: \
         out-of-order delivery"
    )]
    OutOfOrderDelivery {
        /// Checkpoint key of the projection.
        projection: String,
        /// Last applied global nonce.
        last_applied: u64,
        /// Global nonce of the out-of-order event.
        received: u64,
    },

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

    /// Create a new invalid event error
    pub fn invalid_event(message: impl Into<String>) -> Self {
        Self::InvalidEvent {
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

    /// True for an optimistic concurrency conflict (stale writer).
    pub fn is_concurrency_conflict(&self) -> bool {
        matches!(self, Error::ConcurrencyConflict { .. })
    }

    /// True when the failure is transient and the call may be retried.
    ///
    /// For writes this also means the outcome is **unknown**: the store may
    /// have committed the batch before the acknowledgment was lost. Retrying
    /// a repository save is safe because each pending batch carries stable
    /// event IDs and an idempotency key (see
    /// [`EventStoreRepository`](crate::repository::EventStoreRepository)).
    pub fn is_transient(&self) -> bool {
        use tonic::Code;
        match self {
            Error::EventStore(status) => matches!(
                status.code(),
                Code::Unavailable
                    | Code::DeadlineExceeded
                    | Code::Unknown
                    | Code::Cancelled
                    | Code::Internal
                    | Code::ResourceExhausted
            ),
            _ => false,
        }
    }

    /// The gRPC status code, when this error came from the event store.
    pub fn status_code(&self) -> Option<tonic::Code> {
        match self {
            Error::EventStore(status) => Some(status.code()),
            _ => None,
        }
    }
}

impl From<tonic::Status> for Error {
    fn from(status: tonic::Status) -> Self {
        Error::EventStore(Box::new(status))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_classification() {
        assert!(Error::from(tonic::Status::unavailable("x")).is_transient());
        assert!(Error::from(tonic::Status::deadline_exceeded("x")).is_transient());
        assert!(!Error::from(tonic::Status::invalid_argument("x")).is_transient());
        assert!(!Error::concurrency_conflict(1, 2).is_transient());
        assert!(Error::concurrency_conflict(1, 2).is_concurrency_conflict());
    }
}
