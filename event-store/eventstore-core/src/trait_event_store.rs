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
}
