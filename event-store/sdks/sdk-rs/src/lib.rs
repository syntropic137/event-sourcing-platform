use anyhow::Result;
use eventstore_proto::gen::event_store_client::EventStoreClient;
use eventstore_proto::gen::{AppendRequest, ReadStreamRequest, SubscribeRequest};
use tonic::transport::Channel;

pub struct EventStore {
    inner: EventStoreClient<Channel>,
}

impl EventStore {
    pub async fn connect(addr: &str) -> Result<Self> {
        let inner = EventStoreClient::connect(format!("http://{addr}")).await?;
        Ok(Self { inner })
    }

    pub async fn append(
        &mut self,
        req: AppendRequest,
    ) -> Result<eventstore_proto::gen::AppendResponse> {
        let resp = self.inner.append(req).await?.into_inner();
        Ok(resp)
    }

    pub async fn read_stream(
        &mut self,
        req: ReadStreamRequest,
    ) -> Result<eventstore_proto::gen::ReadStreamResponse> {
        let resp = self.inner.read_stream(req).await?.into_inner();
        Ok(resp)
    }

    pub async fn subscribe(
        &mut self,
        req: SubscribeRequest,
    ) -> Result<tonic::Streaming<eventstore_proto::gen::SubscribeResponse>> {
        let stream = self.inner.subscribe(req).await?.into_inner();
        Ok(stream)
    }
}
