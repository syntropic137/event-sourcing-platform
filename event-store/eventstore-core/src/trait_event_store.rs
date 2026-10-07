use async_trait::async_trait;

use crate::errors::StoreError;
use crate::types::{proto, StoreStream};
use proto::{
    AppendRequest, AppendResponse, ReadAllRequest, ReadAllResponse, ReadStreamRequest,
    ReadStreamResponse, SubscribeRequest, SubscribeResponse,
};

#[async_trait]
pub trait EventStore: Send + Sync + 'static {
    async fn append(&self, req: AppendRequest) -> Result<AppendResponse, StoreError>;
    async fn read_stream(&self, req: ReadStreamRequest) -> Result<ReadStreamResponse, StoreError>;
    async fn read_all(&self, req: ReadAllRequest) -> Result<ReadAllResponse, StoreError>;
    fn subscribe(&self, req: SubscribeRequest) -> StoreStream<SubscribeResponse>;

    /// Backend kind reported by `GetServerInfo` (e.g. "memory", "postgres").
    fn backend_kind(&self) -> &'static str {
        "unknown"
    }

    /// Capability flags this backend guarantees (names from
    /// `eventstore_proto::capabilities`). Defaults to none so a backend never
    /// advertises a guarantee it has not explicitly opted into.
    fn capabilities(&self) -> Vec<&'static str> {
        Vec::new()
    }
}
