//! Event store client integration
//!
//! The high-level SDK talks to the event store through [`EventStorePort`], a
//! small trait mirroring the gRPC surface. [`EventStoreClient`] is the
//! production adapter: it delegates to the low-level Rust client
//! (`eventstore-sdk-rs`) instead of re-implementing gRPC calls, and maps
//! store failures to typed SDK errors. Tests and decorators (fault injection,
//! metrics, auth) can implement the port directly.

use std::pin::Pin;

use async_trait::async_trait;
use prost::Message;
use tokio_stream::{Stream, StreamExt};

use crate::error::{Error, Result};

/// Generated protobuf types of the event store API.
pub use eventstore_proto::gen as proto;

/// Stream of events delivered by a subscription.
///
/// The stream may end or yield `Err` (for example when the server drops the
/// connection or a backend query fails); consumers must treat both as
/// failures, never as "caught up".
pub type EventDataStream = Pin<Box<dyn Stream<Item = Result<proto::EventData>> + Send>>;

/// Port to the event store used by repositories and projection runners.
#[async_trait]
pub trait EventStorePort: Send + Sync {
    /// Append a batch to one aggregate stream with optimistic concurrency.
    ///
    /// A stale `expected_aggregate_nonce` must surface as
    /// [`Error::ConcurrencyConflict`].
    async fn append(&self, req: proto::AppendRequest) -> Result<proto::AppendResponse>;

    /// Read one page of an aggregate stream.
    async fn read_stream(&self, req: proto::ReadStreamRequest)
        -> Result<proto::ReadStreamResponse>;

    /// Read one page of the tenant's global log.
    async fn read_all(&self, req: proto::ReadAllRequest) -> Result<proto::ReadAllResponse>;

    /// Subscribe to the tenant's global log from a position (inclusive).
    async fn subscribe(&self, req: proto::SubscribeRequest) -> Result<EventDataStream>;
}

/// gRPC event store client backed by the low-level `eventstore-sdk-rs` client.
///
/// Cloning is cheap; clones share one HTTP/2 channel.
#[derive(Clone)]
pub struct EventStoreClient {
    inner: eventstore_sdk_rs::EventStore,
}

impl EventStoreClient {
    /// Connect to an event store at `address` (`host:port`, optionally
    /// prefixed with `http://`).
    pub async fn connect(address: impl AsRef<str>) -> Result<Self> {
        let address = address.as_ref();
        let address = address.strip_prefix("http://").unwrap_or(address);
        let inner = eventstore_sdk_rs::EventStore::connect(address)
            .await
            .map_err(map_anyhow)?;
        Ok(Self { inner })
    }

    /// Wrap an already connected low-level client.
    pub fn from_low_level(inner: eventstore_sdk_rs::EventStore) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl EventStorePort for EventStoreClient {
    async fn append(&self, req: proto::AppendRequest) -> Result<proto::AppendResponse> {
        let expected = req.expected_aggregate_nonce;
        let mut inner = self.inner.clone();
        inner
            .append(req)
            .await
            .map_err(|err| match map_anyhow(err) {
                Error::EventStore(status) => map_append_status(*status, expected),
                other => other,
            })
    }

    async fn read_stream(
        &self,
        req: proto::ReadStreamRequest,
    ) -> Result<proto::ReadStreamResponse> {
        let mut inner = self.inner.clone();
        inner.read_stream(req).await.map_err(map_anyhow)
    }

    async fn read_all(&self, req: proto::ReadAllRequest) -> Result<proto::ReadAllResponse> {
        let mut inner = self.inner.clone();
        inner.read_all(req).await.map_err(map_anyhow)
    }

    async fn subscribe(&self, req: proto::SubscribeRequest) -> Result<EventDataStream> {
        let mut inner = self.inner.clone();
        let stream = inner.subscribe(req).await.map_err(map_anyhow)?;
        Ok(Box::pin(stream.map(|item| match item {
            Ok(resp) => resp.event.ok_or_else(|| {
                Error::from(tonic::Status::internal(
                    "subscribe response without an event",
                ))
            }),
            Err(status) => Err(Error::from(status)),
        })))
    }
}

/// Map a low-level client error to an SDK error, preserving the gRPC status.
fn map_anyhow(err: anyhow::Error) -> Error {
    let err = match err.downcast::<tonic::Status>() {
        Ok(status) => return Error::from(status),
        Err(err) => err,
    };
    match err.downcast::<tonic::transport::Error>() {
        Ok(transport) => Error::from(tonic::Status::unavailable(format!(
            "transport error: {transport}"
        ))),
        Err(err) => Error::Repository(err),
    }
}

/// `Aborted` on append is the store's optimistic concurrency failure.
fn map_append_status(status: tonic::Status, expected: u64) -> Error {
    if status.code() != tonic::Code::Aborted {
        return Error::from(status);
    }
    let actual = decode_concurrency_detail(status.details())
        .map(|d| d.actual_last_aggregate_nonce)
        .unwrap_or(0);
    Error::concurrency_conflict(expected, actual)
}

fn decode_concurrency_detail(details: &[u8]) -> Option<proto::ConcurrencyErrorDetail> {
    if details.is_empty() {
        return None;
    }
    let any = prost_types::Any::decode(details).ok()?;
    if !any
        .type_url
        .ends_with("eventstore.v1.ConcurrencyErrorDetail")
    {
        return None;
    }
    proto::ConcurrencyErrorDetail::decode(any.value.as_slice()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aborted_with_detail_maps_to_typed_conflict() {
        let detail = proto::ConcurrencyErrorDetail {
            tenant_id: "t".into(),
            aggregate_id: "a".into(),
            actual_last_aggregate_nonce: 7,
            actual_last_global_nonce: 42,
        };
        let any = prost_types::Any {
            type_url: "type.googleapis.com/eventstore.v1.ConcurrencyErrorDetail".into(),
            value: detail.encode_to_vec(),
        };
        let status = tonic::Status::with_details(
            tonic::Code::Aborted,
            "append precondition failed",
            any.encode_to_vec().into(),
        );
        match map_append_status(status, 3) {
            Error::ConcurrencyConflict { expected, actual } => {
                assert_eq!((expected, actual), (3, 7));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn aborted_without_detail_still_typed() {
        let err = map_append_status(tonic::Status::aborted("x"), 2);
        assert!(matches!(
            err,
            Error::ConcurrencyConflict {
                expected: 2,
                actual: 0
            }
        ));
    }

    #[test]
    fn other_status_is_preserved() {
        let err = map_append_status(tonic::Status::invalid_argument("bad"), 0);
        assert_eq!(err.status_code(), Some(tonic::Code::InvalidArgument));
    }

    #[test]
    fn status_inside_anyhow_is_recovered() {
        let err = map_anyhow(anyhow::Error::from(tonic::Status::unavailable("down")));
        assert!(err.is_transient());
    }

    #[tokio::test]
    async fn connect_to_closed_port_is_transient() {
        let port = portpicker::pick_unused_port().expect("free port");
        let err = EventStoreClient::connect(format!("127.0.0.1:{port}"))
            .await
            .err()
            .expect("connect must fail");
        assert!(err.is_transient(), "{err:?}");
    }
}
