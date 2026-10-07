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
    /// object). See ADR-027.
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

    /// The event store reported an undecodable stored event (gRPC
    /// `DATA_LOSS` with trailing metadata `esp-undecodable-global-nonce`,
    /// ADR-026). Not retryable: reconnecting fails at the same position.
    /// Runners stop here and never advance a checkpoint past it.
    ///
    /// Operator recovery: repair the row or deploy an event store that
    /// decodes it, or, if unrecoverable, set the checkpoint of every consumer
    /// that has not passed `global_nonce` to `global_nonce` (it resumes at
    /// `global_nonce + 1`). See ADR-026.
    #[error(
        "Undecodable stored event at global_nonce {global_nonce} (gRPC DATA_LOSS); retrying \
         cannot fix this. Repair the row or deploy an event store that decodes it, or, if \
         unrecoverable, set the checkpoint of every consumer that has not passed \
         {global_nonce} to {global_nonce} (ADR-026). Server: {message}"
    )]
    DataLoss {
        /// Position of the undecodable event.
        global_nonce: u64,
        /// Server message (names the position and column, never payload).
        message: String,
    },

    /// A checkpoint commit was refused because the stored checkpoint is
    /// already at or beyond `position`: another runner owns this key, or the
    /// event was already processed. Not retryable.
    #[error(
        "checkpoint for {projection} is at {stored}; refusing to commit {position} \
         (another runner owns this key, or the event was already processed)"
    )]
    CheckpointFenced {
        /// Checkpoint key of the projection.
        projection: String,
        /// Position found in the store.
        stored: u64,
        /// Position the runner tried to commit.
        position: u64,
    },

    /// A [`LiveProcessor`](crate::projection::LiveProcessor) pass panicked.
    /// The runner stops with this error unless configured to restart the
    /// processor ([`ProcessorPanicPolicy::Restart`](crate::projection::ProcessorPanicPolicy)).
    #[error("Live processor of {projection} panicked: {message}")]
    LiveProcessorPanicked {
        /// Checkpoint key of the projection.
        projection: String,
        /// Panic payload, when it was a string.
        message: String,
    },

    /// The event store does not meet a stated capability or version floor
    /// (`EventStoreClient::require_capabilities` / `require_min_version`).
    #[error("{0}")]
    Incompatible(eventstore_sdk_rs::CompatibilityError),

    /// Invalid client configuration (endpoint, TLS material, credentials).
    /// Never contains secret values.
    #[error("{0}")]
    Config(String),

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
            Error::DataLoss { .. } => Some(tonic::Code::DataLoss),
            _ => None,
        }
    }

    /// Position of the undecodable stored event, for [`Error::DataLoss`].
    pub fn data_loss_position(&self) -> Option<u64> {
        match self {
            Error::DataLoss { global_nonce, .. } => Some(*global_nonce),
            _ => None,
        }
    }
}

/// gRPC trailing-metadata key carrying the position of an undecodable
/// stored event on a `DATA_LOSS` status (ADR-026).
pub const UNDECODABLE_GLOBAL_NONCE_KEY: &str = "esp-undecodable-global-nonce";

impl From<tonic::Status> for Error {
    /// `DATA_LOSS` carrying the `esp-undecodable-global-nonce` position
    /// becomes [`Error::DataLoss`]; anything else is [`Error::EventStore`].
    fn from(status: tonic::Status) -> Self {
        if status.code() == tonic::Code::DataLoss {
            let position = status
                .metadata()
                .get(UNDECODABLE_GLOBAL_NONCE_KEY)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse::<u64>().ok());
            if let Some(global_nonce) = position {
                return Error::DataLoss {
                    global_nonce,
                    message: status.message().to_string(),
                };
            }
        }
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

    fn data_loss_status(position: Option<&str>) -> tonic::Status {
        let mut metadata = tonic::metadata::MetadataMap::new();
        if let Some(p) = position {
            metadata.insert(UNDECODABLE_GLOBAL_NONCE_KEY, p.parse().unwrap());
        }
        tonic::Status::with_metadata(tonic::Code::DataLoss, "bad row", metadata)
    }

    #[test]
    fn data_loss_with_position_is_typed_and_not_transient() {
        let err = Error::from(data_loss_status(Some("42")));
        assert!(
            matches!(err, Error::DataLoss { global_nonce: 42, ref message } if message == "bad row"),
            "{err:?}"
        );
        assert_eq!(err.data_loss_position(), Some(42));
        assert_eq!(err.status_code(), Some(tonic::Code::DataLoss));
        assert!(!err.is_transient());
        assert!(err.to_string().contains("ADR-026"), "{err}");
    }

    #[test]
    fn data_loss_without_position_stays_a_non_transient_status() {
        for position in [None, Some("x"), Some("-1")] {
            let err = Error::from(data_loss_status(position));
            assert!(matches!(err, Error::EventStore(_)), "{err:?}");
            assert_eq!(err.status_code(), Some(tonic::Code::DataLoss));
            assert!(!err.is_transient());
            assert_eq!(err.data_loss_position(), None);
        }
    }
}
