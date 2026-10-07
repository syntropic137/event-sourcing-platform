//! Fixture domain shared with `tests/xlang/ts_peer.cjs` and
//! `tests/xlang/py_peer.py` (ADR-027). Keep the three in sync.

use std::path::PathBuf;
use std::sync::Mutex;

use async_trait::async_trait;
use event_sourcing_rust::client::{proto, EventDataStream, EventStorePort};
use event_sourcing_rust::prelude::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountOpened {
    pub account_id: String,
    pub owner: String,
}
impl EventSchema for AccountOpened {
    const EVENT_TYPE: &'static str = "AccountOpened";
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MoneyDeposited {
    pub amount: i64,
    pub note: String,
}
impl EventSchema for MoneyDeposited {
    const EVENT_TYPE: &'static str = "MoneyDeposited";
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountClosed {
    pub reason: String,
    pub tags: Vec<String>,
}
impl EventSchema for AccountClosed {
    const EVENT_TYPE: &'static str = "AccountClosed";
    const EVENT_VERSION: u32 = 2;
}

event_sourcing_rust::event_enum! {
    #[derive(Debug, Clone, PartialEq)]
    pub enum AccountEvent {
        Opened(AccountOpened),
        Deposited(MoneyDeposited),
        Closed(AccountClosed),
    }
}

#[derive(Debug, Clone, Default)]
pub struct Account {
    pub id: Option<String>,
    pub events: Vec<AccountEvent>,
}

impl Aggregate for Account {
    type Event = AccountEvent;
    type Error = Error;
    const AGGREGATE_TYPE: &'static str = "Account";

    fn aggregate_id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    fn version(&self) -> u64 {
        self.events.len() as u64
    }

    fn apply_event(&mut self, event: &AccountEvent) -> Result<()> {
        if let AccountEvent::Opened(e) = event {
            self.id = Some(e.account_id.clone());
        }
        self.events.push(event.clone());
        Ok(())
    }
}

pub const NOTE: &str = "caf\u{e9} \u{2615} \"quoted\"";

/// The events every peer writes for aggregate `id`. Python writes only the
/// first two (it cannot write version 2 yet, see ADR-027).
pub fn fixture_events(id: &str) -> Vec<AccountEvent> {
    vec![
        AccountOpened {
            account_id: id.into(),
            owner: "alice".into(),
        }
        .into(),
        MoneyDeposited {
            amount: 125,
            note: NOTE.into(),
        }
        .into(),
        AccountClosed {
            reason: "done".into(),
            tags: vec!["a".into(), "b".into()],
        }
        .into(),
    ]
}

pub fn xlang_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/xlang")
}

pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/xlang")
}

/// Port that records appends and acknowledges them; reads are empty.
#[derive(Default)]
pub struct CapturePort {
    pub appends: Mutex<Vec<proto::AppendRequest>>,
}

#[async_trait]
impl EventStorePort for CapturePort {
    async fn append(&self, req: proto::AppendRequest) -> Result<proto::AppendResponse> {
        let n = req.expected_aggregate_nonce + req.events.len() as u64;
        self.appends.lock().unwrap().push(req);
        Ok(proto::AppendResponse {
            last_global_nonce: n,
            last_aggregate_nonce: n,
        })
    }

    async fn read_stream(
        &self,
        _req: proto::ReadStreamRequest,
    ) -> Result<proto::ReadStreamResponse> {
        Ok(proto::ReadStreamResponse {
            is_end: true,
            ..Default::default()
        })
    }

    async fn read_all(&self, _req: proto::ReadAllRequest) -> Result<proto::ReadAllResponse> {
        Ok(proto::ReadAllResponse {
            is_end: true,
            ..Default::default()
        })
    }

    async fn subscribe(&self, _req: proto::SubscribeRequest) -> Result<EventDataStream> {
        Err(Error::domain("not supported"))
    }
}

/// Minimal standard base64 decoder (fixtures only).
pub fn base64_decode(input: &str) -> Vec<u8> {
    fn val(c: u8) -> u32 {
        match c {
            b'A'..=b'Z' => (c - b'A') as u32,
            b'a'..=b'z' => (c - b'a' + 26) as u32,
            b'0'..=b'9' => (c - b'0' + 52) as u32,
            b'+' => 62,
            b'/' => 63,
            _ => panic!("invalid base64 byte {c}"),
        }
    }
    let bytes: Vec<u8> = input.bytes().filter(|b| *b != b'=').collect();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        let mut acc = 0u32;
        for (i, c) in chunk.iter().enumerate() {
            acc |= val(*c) << (18 - 6 * i);
        }
        out.push((acc >> 16) as u8);
        if chunk.len() > 2 {
            out.push((acc >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(acc as u8);
        }
    }
    out
}
