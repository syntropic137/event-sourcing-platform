use bytes::Bytes;
use prost::Message;
use prost_types::Any;
use thiserror::Error;

use eventstore_proto::gen as proto;

/// gRPC trailing-metadata key carrying the `global_nonce` of an undecodable
/// stored event on a `DATA_LOSS` status (decimal ASCII).
pub const UNDECODABLE_GLOBAL_NONCE_KEY: &str = "esp-undecodable-global-nonce";

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("concurrency conflict: {message}")]
    Concurrency {
        message: String,
        detail: Option<proto::ConcurrencyErrorDetail>,
    },
    #[error("invalid argument: {0}")]
    Invalid(String),
    #[error("already exists: {0}")]
    AlreadyExists(String),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("unauthenticated: {0}")]
    Unauthenticated(String),
    #[error("resource exhausted: {0}")]
    ResourceExhausted(String),
    /// A dependency (e.g. the database) failed while serving the request.
    /// Retryable. For a subscription, the stream ends after this error and the
    /// consumer reconnects from its own last checkpoint (at-least-once).
    #[error("unavailable: {0}")]
    Unavailable(String),
    /// A stored event cannot be decoded (data integrity). Not retryable until
    /// an operator repairs the row or the reader. `reason` never contains
    /// payload or header values. Subscriptions stop here; see ADR-026.
    #[error(
        "data integrity: stored event at global_nonce {global_nonce} cannot be decoded: {reason}"
    )]
    UndecodableEvent { global_nonce: u64, reason: String },
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl StoreError {
    fn encode_concurrency_detail(detail: &proto::ConcurrencyErrorDetail) -> Bytes {
        let any = Any {
            type_url: "type.googleapis.com/eventstore.v1.ConcurrencyErrorDetail".to_string(),
            value: detail.encode_to_vec(),
        };
        Bytes::from(any.encode_to_vec())
    }

    pub fn to_status(&self) -> tonic::Status {
        use tonic::Code;
        match self {
            StoreError::NotFound(msg) => tonic::Status::new(Code::NotFound, msg.clone()),
            StoreError::Concurrency { message, detail } => {
                if let Some(detail) = detail {
                    tonic::Status::with_details(
                        Code::Aborted,
                        message.clone(),
                        Self::encode_concurrency_detail(detail),
                    )
                } else {
                    tonic::Status::new(Code::Aborted, message.clone())
                }
            }
            StoreError::Invalid(msg) => tonic::Status::new(Code::InvalidArgument, msg.clone()),
            StoreError::AlreadyExists(msg) => tonic::Status::new(Code::AlreadyExists, msg.clone()),
            StoreError::PermissionDenied(msg) => {
                tonic::Status::new(Code::PermissionDenied, msg.clone())
            }
            StoreError::Unauthenticated(msg) => {
                tonic::Status::new(Code::Unauthenticated, msg.clone())
            }
            StoreError::ResourceExhausted(msg) => {
                tonic::Status::new(Code::ResourceExhausted, msg.clone())
            }
            StoreError::Unavailable(msg) => tonic::Status::new(Code::Unavailable, msg.clone()),
            StoreError::UndecodableEvent { global_nonce, .. } => {
                // The position is also sent as metadata so clients need not
                // parse the message (e.g. to find the head, or to skip it).
                let mut metadata = tonic::metadata::MetadataMap::new();
                metadata.insert(UNDECODABLE_GLOBAL_NONCE_KEY, (*global_nonce).into());
                tonic::Status::with_metadata(Code::DataLoss, self.to_string(), metadata)
            }
            StoreError::Internal(err) => tonic::Status::new(Code::Internal, err.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_maps_to_grpc_unavailable() {
        let status = StoreError::Unavailable("db down".into()).to_status();
        assert_eq!(status.code(), tonic::Code::Unavailable);
        assert_eq!(status.message(), "db down");
    }

    #[test]
    fn undecodable_event_maps_to_grpc_data_loss_with_position() {
        let status = StoreError::UndecodableEvent {
            global_nonce: 42,
            reason: "column 'headers'".into(),
        }
        .to_status();
        assert_eq!(status.code(), tonic::Code::DataLoss);
        assert!(status.message().contains("global_nonce 42"), "{status:?}");
        let nonce = status
            .metadata()
            .get(UNDECODABLE_GLOBAL_NONCE_KEY)
            .expect("position metadata")
            .to_str()
            .unwrap();
        assert_eq!(nonce, "42");
    }
}
