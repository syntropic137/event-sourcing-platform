use eventstore_proto::gen::{AppendRequest, EventData, EventMetadata, ReadStreamRequest};
use eventstore_sdk_rs::EventStore;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let addr = std::env::var("EVENTSTORE_ADDR").unwrap_or_else(|_| "localhost:50051".to_string());
    let mut client = EventStore::connect(&addr).await?;

    let stream_id = "Order-RS-1".to_string();

    let ev = EventData {
        meta: Some(EventMetadata {
            aggregate_id: stream_id.clone(),
            aggregate_type: "Order".into(),
            aggregate_nonce: 1, // client proposes the nonce
            event_type: "OrderCreated".into(),
            ..Default::default()
        }),
        payload: b"hello".to_vec(),
    };

    let _ = client
        .append(AppendRequest {
            aggregate_id: stream_id.clone(),
            aggregate_type: "Order".into(),
            expected: Some(
                eventstore_proto::gen::append_request::Expected::ExpectedAny(
                    eventstore_proto::gen::Expected::Any as i32,
                ),
            ),
            events: vec![ev],
        })
        .await?;

    let out = client
        .read_stream(ReadStreamRequest {
            aggregate_id: stream_id.clone(),
            from_aggregate_nonce: 1,
            max_count: 100,
            forward: true,
        })
        .await?;

    println!("read count: {}", out.events.len());

    Ok(())
}
