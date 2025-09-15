use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("concurrency conflict: {0}")]
    Concurrency(String),
    #[error("invalid argument: {0}")]
    Invalid(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl StoreError {
    pub fn to_status(&self) -> tonic::Status {
        use tonic::Code;
        match self {
            StoreError::NotFound(msg) => tonic::Status::new(Code::NotFound, msg.clone()),
            StoreError::Concurrency(msg) => tonic::Status::new(Code::Aborted, msg.clone()),
            StoreError::Invalid(msg) => tonic::Status::new(Code::InvalidArgument, msg.clone()),
            StoreError::Other(e) => tonic::Status::new(Code::Unknown, e.to_string()),
        }
    }
}
