use bytes::Bytes;
use prost::Message;
use prost_types::Any;
use thiserror::Error;

use eventstore_proto::gen as proto;

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
}
