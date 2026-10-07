//! Event definitions and metadata handling
//!
//! # Wire format
//!
//! Events are stored in the cross-language envelope of ADR-026 (see
//! [`crate::wire`]): the payload is a JSON object holding only the event's
//! fields, and the event type and schema version live in metadata. This is
//! what the TypeScript and Python SDKs write, so any SDK can read any stream.
//!
//! # Defining events
//!
//! Give each event its own struct and implement [`EventSchema`] for it, then
//! group an aggregate's events with [`event_enum!`](crate::event_enum):
//!
//! ```
//! use event_sourcing_rust::prelude::*;
//!
//! #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
//! pub struct AccountOpened {
//!     pub account_id: String,
//!     pub owner: String,
//! }
//! impl EventSchema for AccountOpened {
//!     const EVENT_TYPE: &'static str = "AccountOpened";
//! }
//!
//! #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
//! pub struct MoneyDeposited {
//!     pub amount: i64,
//! }
//! impl EventSchema for MoneyDeposited {
//!     const EVENT_TYPE: &'static str = "MoneyDeposited";
//!     const EVENT_VERSION: u32 = 2;
//! }
//!
//! event_sourcing_rust::event_enum! {
//!     #[derive(Debug, Clone, PartialEq)]
//!     pub enum AccountEvent {
//!         Opened(AccountOpened),
//!         Deposited(MoneyDeposited),
//!     }
//! }
//!
//! let event = AccountEvent::from(MoneyDeposited { amount: 5 });
//! assert_eq!(event.event_type(), "MoneyDeposited");
//! assert_eq!(event.event_version(), 2);
//! assert_eq!(event.to_payload().unwrap(), br#"{"amount":5}"#);
//! ```

use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Debug;
use uuid::Uuid;

use crate::error::{Error, Result};

/// A domain event as written to and read from the event store.
///
/// Domain events represent facts that have occurred in the system. Implement
/// this through [`EventSchema`] (one struct) or
/// [`event_enum!`](crate::event_enum) (an aggregate's set of events); a
/// hand-written impl must follow the same rules:
///
/// * [`to_payload`](Self::to_payload) returns a JSON **object** with only the
///   event's fields (no type tag): `{"amount":5}`, never
///   `{"Deposited":{"amount":5}}`.
/// * [`from_payload`](Self::from_payload) dispatches on the stored event type
///   and version and returns [`Error::UnknownEventType`] or
///   [`Error::UnknownEventVersion`] for anything it does not know. It must
///   never guess.
pub trait DomainEvent: Debug + Clone + Send + Sync + Sized {
    /// Stable event type written to `meta.event_type`, e.g.
    /// `"MoneyDeposited"`. Renaming it orphans stored events (use an
    /// upcaster).
    fn event_type(&self) -> &'static str;

    /// Schema version written to `meta.event_version` (starts at 1).
    fn event_version(&self) -> u32 {
        1
    }

    /// Serialize the event body: a JSON object with the event's fields only.
    fn to_payload(&self) -> Result<Vec<u8>>;

    /// Decode a stored event, dispatching on its type and version.
    fn from_payload(event: &SerializedEvent<'_>) -> Result<Self>;

    /// Get optional correlation ID for tracing related events
    fn correlation_id(&self) -> Option<&str> {
        None
    }

    /// Get optional causation ID for event causality tracking
    fn causation_id(&self) -> Option<&str> {
        None
    }
}

/// Schema of one event type at one version: a struct whose serde form is the
/// wire payload.
///
/// Fields are serialized by serde as a flat JSON object. Use
/// `#[serde(rename_all = "camelCase")]` (or per-field renames) when the
/// stream is shared with code that uses other field names. Do not use
/// `#[serde(deny_unknown_fields)]` on events read from TypeScript streams:
/// the TypeScript SDK currently also writes its `eventType` and
/// `schemaVersion` class fields into the payload (ADR-026).
///
/// Every `EventSchema` is a [`DomainEvent`] on its own, which is handy in
/// projections (`recorded.decode::<MoneyDeposited>()`).
pub trait EventSchema: Serialize + DeserializeOwned + Debug + Clone + Send + Sync {
    /// Stable event type, e.g. `"MoneyDeposited"`.
    const EVENT_TYPE: &'static str;
    /// Schema version (starts at 1).
    const EVENT_VERSION: u32 = 1;
}

impl<T: EventSchema> DomainEvent for T {
    fn event_type(&self) -> &'static str {
        const {
            assert!(
                crate::wire::is_valid_event_type(T::EVENT_TYPE),
                "EventSchema::EVENT_TYPE must be non-empty printable ASCII without spaces"
            );
        }
        T::EVENT_TYPE
    }

    fn event_version(&self) -> u32 {
        const {
            assert!(
                T::EVENT_VERSION >= 1,
                "EventSchema::EVENT_VERSION starts at 1"
            )
        }
        T::EVENT_VERSION
    }

    fn to_payload(&self) -> Result<Vec<u8>> {
        encode_body(self)
    }

    fn from_payload(event: &SerializedEvent<'_>) -> Result<Self> {
        if event.event_type != T::EVENT_TYPE {
            return Err(event.unknown_type());
        }
        event.decode_as::<T>()
    }
}

/// Serialize an event body as compact JSON, rejecting anything that is not a
/// JSON object (unit structs, tuple structs, externally tagged enums, ...).
pub fn encode_body<T: Serialize + ?Sized>(body: &T) -> Result<Vec<u8>> {
    let bytes = serde_json::to_vec(body)?;
    if bytes.first() != Some(&b'{') {
        return Err(Error::invalid_event(
            "event payload must serialize to a JSON object (use a struct with named fields, \
             `struct E {}` for an event without data)",
        ));
    }
    Ok(bytes)
}

/// A stored event handed to [`DomainEvent::from_payload`]: type and version
/// from metadata (after upcasting) and the JSON payload.
#[derive(Debug, Clone, Copy)]
pub struct SerializedEvent<'a> {
    /// Event type (`meta.event_type`).
    pub event_type: &'a str,
    /// Schema version (`meta.event_version`; `0` on the wire is read as 1).
    pub event_version: u32,
    /// JSON object payload.
    pub payload: &'a [u8],
}

impl<'a> SerializedEvent<'a> {
    /// A stored event.
    pub fn new(event_type: &'a str, event_version: u32, payload: &'a [u8]) -> Self {
        Self {
            event_type,
            event_version: crate::wire::normalize_version(event_version),
            payload,
        }
    }

    /// Deserialize the payload as `T` without checking type or version.
    pub fn deserialize<T: DeserializeOwned>(&self) -> Result<T> {
        serde_json::from_slice(self.payload).map_err(|source| Error::EventDecode {
            event_type: self.event_type.to_string(),
            event_version: self.event_version,
            source,
        })
    }

    /// Decode as schema `T`: the version must be `T::EVENT_VERSION`
    /// (otherwise [`Error::UnknownEventVersion`]). The caller has matched
    /// the type.
    pub fn decode_as<T: EventSchema>(&self) -> Result<T> {
        if self.event_version != T::EVENT_VERSION {
            return Err(self.unknown_version());
        }
        self.deserialize()
    }

    /// Error for an event type the decoder does not know.
    pub fn unknown_type(&self) -> Error {
        Error::UnknownEventType {
            event_type: self.event_type.to_string(),
            event_version: self.event_version,
        }
    }

    /// Error for a known event type at a version the decoder does not know.
    pub fn unknown_version(&self) -> Error {
        Error::UnknownEventVersion {
            event_type: self.event_type.to_string(),
            event_version: self.event_version,
        }
    }
}

/// Define an enum of an aggregate's events, one [`EventSchema`] struct per
/// variant, with its [`DomainEvent`] impl and `From` conversions.
///
/// Each variant is written as the bare struct body (no type tag) and decoded
/// by dispatching on the stored `(event_type, event_version)`. Two variants
/// may share an event type at different versions. Duplicate
/// `(type, version)` pairs, invalid type names and version 0 are compile
/// errors.
///
/// ```
/// use event_sourcing_rust::prelude::*;
///
/// #[derive(Debug, Clone, Serialize, Deserialize)]
/// pub struct Opened { pub owner: String }
/// impl EventSchema for Opened { const EVENT_TYPE: &'static str = "AccountOpened"; }
///
/// #[derive(Debug, Clone, Serialize, Deserialize)]
/// pub struct Closed {}
/// impl EventSchema for Closed { const EVENT_TYPE: &'static str = "AccountClosed"; }
///
/// event_sourcing_rust::event_enum! {
///     /// Events of the Account aggregate.
///     #[derive(Debug, Clone)]
///     pub enum AccountEvent {
///         Opened(Opened),
///         Closed(Closed),
///     }
/// }
///
/// let stored = SerializedEvent::new("AccountOpened", 1, br#"{"owner":"alice"}"#);
/// assert!(matches!(AccountEvent::from_payload(&stored), Ok(AccountEvent::Opened(_))));
///
/// let unknown = SerializedEvent::new("AccountFrozen", 1, b"{}");
/// assert!(matches!(
///     AccountEvent::from_payload(&unknown),
///     Err(Error::UnknownEventType { .. })
/// ));
/// ```
#[macro_export]
macro_rules! event_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $( $(#[$vmeta:meta])* $variant:ident($ty:ty) ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        $vis enum $name {
            $( $(#[$vmeta])* $variant($ty), )+
        }

        const _: () = $crate::event::__private::check_schemas(&[
            $( (
                <$ty as $crate::event::EventSchema>::EVENT_TYPE,
                <$ty as $crate::event::EventSchema>::EVENT_VERSION,
            ), )+
        ]);

        impl $crate::event::DomainEvent for $name {
            fn event_type(&self) -> &'static str {
                match self {
                    $( Self::$variant(_) => <$ty as $crate::event::EventSchema>::EVENT_TYPE, )+
                }
            }

            fn event_version(&self) -> u32 {
                match self {
                    $( Self::$variant(_) => <$ty as $crate::event::EventSchema>::EVENT_VERSION, )+
                }
            }

            fn to_payload(&self) -> $crate::error::Result<::std::vec::Vec<u8>> {
                match self {
                    $( Self::$variant(body) => $crate::event::encode_body(body), )+
                }
            }

            fn from_payload(
                event: &$crate::event::SerializedEvent<'_>,
            ) -> $crate::error::Result<Self> {
                let mut known_type = false;
                $(
                    if event.event_type == <$ty as $crate::event::EventSchema>::EVENT_TYPE {
                        if event.event_version
                            == <$ty as $crate::event::EventSchema>::EVENT_VERSION
                        {
                            return event.deserialize::<$ty>().map(Self::$variant);
                        }
                        known_type = true;
                    }
                )+
                if known_type {
                    Err(event.unknown_version())
                } else {
                    Err(event.unknown_type())
                }
            }
        }

        $(
            impl ::std::convert::From<$ty> for $name {
                fn from(body: $ty) -> Self {
                    Self::$variant(body)
                }
            }
        )+
    };
}

#[doc(hidden)]
pub mod __private {
    /// Compile-time checks for [`event_enum!`](crate::event_enum).
    pub const fn check_schemas(schemas: &[(&str, u32)]) {
        let mut i = 0;
        while i < schemas.len() {
            let (ty, version) = schemas[i];
            assert!(
                crate::wire::is_valid_event_type(ty),
                "event_enum!: EVENT_TYPE must be non-empty printable ASCII without spaces"
            );
            assert!(version >= 1, "event_enum!: EVENT_VERSION starts at 1");
            let mut j = i + 1;
            while j < schemas.len() {
                let (other, other_version) = schemas[j];
                assert!(
                    !(str_eq(ty, other) && version == other_version),
                    "event_enum!: two variants have the same EVENT_TYPE and EVENT_VERSION"
                );
                j += 1;
            }
            i += 1;
        }
    }

    const fn str_eq(a: &str, b: &str) -> bool {
        let (a, b) = (a.as_bytes(), b.as_bytes());
        if a.len() != b.len() {
            return false;
        }
        let mut i = 0;
        while i < a.len() {
            if a[i] != b[i] {
                return false;
            }
            i += 1;
        }
        true
    }
}

/// Event metadata containing system-level information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventMetadata {
    /// Unique identifier for this event
    pub event_id: Uuid,
    /// Type of the event
    pub event_type: String,
    /// Schema version of the event
    pub event_version: u32,
    /// Content type of the payload (e.g., "application/json")
    pub content_type: String,
    /// When the event occurred
    pub timestamp: DateTime<Utc>,
    /// ID of the aggregate that emitted this event
    pub aggregate_id: String,
    /// Type of the aggregate that emitted this event
    pub aggregate_type: String,
    /// Sequence number within the aggregate
    pub aggregate_nonce: u64,
    /// Global sequence number across all events
    pub global_nonce: Option<u64>,
    /// Optional correlation ID for request tracing
    pub correlation_id: Option<String>,
    /// Optional causation ID for event causality
    pub causation_id: Option<String>,
    /// Optional actor/user ID who caused this event
    pub actor_id: Option<String>,
    /// Optional tenant ID for multi-tenant systems
    pub tenant_id: Option<String>,
    /// Additional custom metadata
    pub metadata: HashMap<String, String>,
}

impl EventMetadata {
    /// Create new event metadata
    pub fn new(
        event_type: String,
        event_version: u32,
        aggregate_id: String,
        aggregate_type: String,
        aggregate_nonce: u64,
    ) -> Self {
        Self {
            event_id: Uuid::new_v4(), // TODO: Use v7 with timestamp when available
            event_type,
            event_version,
            content_type: "application/json".to_string(),
            timestamp: Utc::now(),
            aggregate_id,
            aggregate_type,
            aggregate_nonce,
            global_nonce: None,
            correlation_id: None,
            causation_id: None,
            actor_id: None,
            tenant_id: None,
            metadata: HashMap::new(),
        }
    }

    /// Set the correlation ID
    pub fn with_correlation_id(mut self, correlation_id: String) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }

    /// Set the causation ID
    pub fn with_causation_id(mut self, causation_id: String) -> Self {
        self.causation_id = Some(causation_id);
        self
    }

    /// Set the actor ID
    pub fn with_actor_id(mut self, actor_id: String) -> Self {
        self.actor_id = Some(actor_id);
        self
    }

    /// Set the tenant ID
    pub fn with_tenant_id(mut self, tenant_id: String) -> Self {
        self.tenant_id = Some(tenant_id);
        self
    }

    /// Add custom metadata
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    /// Set the global nonce (typically set by the event store)
    pub fn with_global_nonce(mut self, global_nonce: u64) -> Self {
        self.global_nonce = Some(global_nonce);
        self
    }
}

/// An event envelope containing both the event data and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope<E> {
    /// Event metadata
    pub metadata: EventMetadata,
    /// The actual event data
    pub event: E,
}

impl<E> EventEnvelope<E>
where
    E: DomainEvent,
{
    /// Create a new event envelope
    pub fn new(
        event: E,
        aggregate_id: String,
        aggregate_type: String,
        aggregate_nonce: u64,
    ) -> Self {
        let mut metadata = EventMetadata::new(
            event.event_type().to_string(),
            event.event_version(),
            aggregate_id,
            aggregate_type,
            aggregate_nonce,
        );

        // Use correlation/causation from event if available
        if let Some(correlation_id) = event.correlation_id() {
            metadata = metadata.with_correlation_id(correlation_id.to_string());
        }
        if let Some(causation_id) = event.causation_id() {
            metadata = metadata.with_causation_id(causation_id.to_string());
        }

        Self { metadata, event }
    }

    /// Get the event ID
    pub fn event_id(&self) -> Uuid {
        self.metadata.event_id
    }

    /// Get the aggregate ID
    pub fn aggregate_id(&self) -> &str {
        &self.metadata.aggregate_id
    }

    /// Get the aggregate type
    pub fn aggregate_type(&self) -> &str {
        &self.metadata.aggregate_type
    }

    /// Get the aggregate nonce
    pub fn aggregate_nonce(&self) -> u64 {
        self.metadata.aggregate_nonce
    }

    /// Get the global nonce if available
    pub fn global_nonce(&self) -> Option<u64> {
        self.metadata.global_nonce
    }

    /// Get the timestamp
    pub fn timestamp(&self) -> DateTime<Utc> {
        self.metadata.timestamp
    }
}

/// Builder for creating event context with tracing information
#[derive(Debug, Default)]
pub struct EventContext {
    correlation_id: Option<String>,
    causation_id: Option<String>,
    actor_id: Option<String>,
    tenant_id: Option<String>,
    metadata: HashMap<String, String>,
}

impl EventContext {
    /// Create a new event context
    pub fn new() -> Self {
        Self::default()
    }

    /// Set correlation ID
    pub fn with_correlation_id(mut self, correlation_id: String) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }

    /// Set causation ID
    pub fn with_causation_id(mut self, causation_id: String) -> Self {
        self.causation_id = Some(causation_id);
        self
    }

    /// Set actor ID
    pub fn with_actor_id(mut self, actor_id: String) -> Self {
        self.actor_id = Some(actor_id);
        self
    }

    /// Set tenant ID
    pub fn with_tenant_id(mut self, tenant_id: String) -> Self {
        self.tenant_id = Some(tenant_id);
        self
    }

    /// Add custom metadata
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    /// Apply this context to event metadata
    pub fn apply_to_metadata(&self, metadata: &mut EventMetadata) {
        if let Some(ref correlation_id) = self.correlation_id {
            metadata.correlation_id = Some(correlation_id.clone());
        }
        if let Some(ref causation_id) = self.causation_id {
            metadata.causation_id = Some(causation_id.clone());
        }
        if let Some(ref actor_id) = self.actor_id {
            metadata.actor_id = Some(actor_id.clone());
        }
        if let Some(ref tenant_id) = self.tenant_id {
            metadata.tenant_id = Some(tenant_id.clone());
        }
        for (key, value) in &self.metadata {
            metadata.metadata.insert(key.clone(), value.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct TestEvent {
        message: String,
    }

    impl EventSchema for TestEvent {
        const EVENT_TYPE: &'static str = "TestEvent";
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct Renamed {
        amount: i64,
    }

    impl EventSchema for Renamed {
        const EVENT_TYPE: &'static str = "Renamed";
        const EVENT_VERSION: u32 = 2;
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct RenamedV1 {
        amt: i64,
    }

    impl EventSchema for RenamedV1 {
        const EVENT_TYPE: &'static str = "Renamed";
    }

    crate::event_enum! {
        #[derive(Debug, Clone, PartialEq)]
        enum Both {
            Test(TestEvent),
            Renamed(Renamed),
            RenamedV1(RenamedV1),
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct Unit;

    #[test]
    fn enum_encodes_flat_body_and_metadata_identity() {
        let e = Both::from(Renamed { amount: 3 });
        assert_eq!(e.event_type(), "Renamed");
        assert_eq!(e.event_version(), 2);
        assert_eq!(e.to_payload().unwrap(), br#"{"amount":3}"#);
        let e = Both::from(RenamedV1 { amt: 3 });
        assert_eq!(e.event_version(), 1);
    }

    #[test]
    fn enum_dispatches_on_type_and_version() {
        let v2 = SerializedEvent::new("Renamed", 2, br#"{"amount":3}"#);
        assert_eq!(
            Both::from_payload(&v2).unwrap(),
            Both::Renamed(Renamed { amount: 3 })
        );
        let v1 = SerializedEvent::new("Renamed", 1, br#"{"amt":4}"#);
        assert_eq!(
            Both::from_payload(&v1).unwrap(),
            Both::RenamedV1(RenamedV1 { amt: 4 })
        );
        // Version 0 on the wire means unset, read as 1.
        let v0 = SerializedEvent::new("Renamed", 0, br#"{"amt":4}"#);
        assert!(matches!(Both::from_payload(&v0), Ok(Both::RenamedV1(_))));
    }

    #[test]
    fn unknown_type_and_version_are_typed_errors() {
        let unknown = SerializedEvent::new("Nope", 1, b"{}");
        assert!(matches!(
            Both::from_payload(&unknown),
            Err(Error::UnknownEventType { event_type, event_version: 1 }) if event_type == "Nope"
        ));
        let newer = SerializedEvent::new("Renamed", 3, b"{}");
        assert!(matches!(
            Both::from_payload(&newer),
            Err(Error::UnknownEventVersion {
                event_version: 3,
                ..
            })
        ));
        let bad = SerializedEvent::new("Renamed", 2, br#"{"amount":"x"}"#);
        assert!(matches!(
            Both::from_payload(&bad),
            Err(Error::EventDecode {
                event_version: 2,
                ..
            })
        ));
    }

    #[test]
    fn single_schema_is_a_domain_event() {
        let ok = SerializedEvent::new("TestEvent", 1, br#"{"message":"hi"}"#);
        assert_eq!(TestEvent::from_payload(&ok).unwrap().message, "hi");
        let other = SerializedEvent::new("Renamed", 2, br#"{"message":"hi"}"#);
        assert!(matches!(
            TestEvent::from_payload(&other),
            Err(Error::UnknownEventType { .. })
        ));
        let newer = SerializedEvent::new("TestEvent", 2, br#"{"message":"hi"}"#);
        assert!(matches!(
            TestEvent::from_payload(&newer),
            Err(Error::UnknownEventVersion { .. })
        ));
    }

    #[test]
    fn non_object_bodies_are_rejected() {
        assert!(matches!(
            encode_body(&Unit),
            Err(Error::InvalidEvent { .. })
        ));
        assert!(encode_body(&5).is_err());
        assert_eq!(encode_body(&serde_json::json!({})).unwrap(), b"{}");
    }

    #[test]
    fn test_event_metadata_creation() {
        let metadata = EventMetadata::new(
            "TestEvent".to_string(),
            1,
            "test-123".to_string(),
            "TestAggregate".to_string(),
            5,
        );

        assert_eq!(metadata.event_type, "TestEvent");
        assert_eq!(metadata.event_version, 1);
        assert_eq!(metadata.aggregate_id, "test-123");
        assert_eq!(metadata.aggregate_type, "TestAggregate");
        assert_eq!(metadata.aggregate_nonce, 5);
        assert!(metadata.global_nonce.is_none());
    }

    #[test]
    fn test_event_envelope() {
        let event = TestEvent {
            message: "Hello, World!".to_string(),
        };

        let envelope = EventEnvelope::new(
            event,
            "test-123".to_string(),
            "TestAggregate".to_string(),
            1,
        );

        assert_eq!(envelope.aggregate_id(), "test-123");
        assert_eq!(envelope.aggregate_type(), "TestAggregate");
        assert_eq!(envelope.aggregate_nonce(), 1);
        assert_eq!(envelope.metadata.event_type, "TestEvent");
        assert_eq!(envelope.event.message, "Hello, World!");
    }

    #[test]
    fn test_event_context() {
        let context = EventContext::new()
            .with_correlation_id("corr-123".to_string())
            .with_actor_id("user-456".to_string())
            .with_metadata("custom".to_string(), "value".to_string());

        let mut metadata = EventMetadata::new(
            "TestEvent".to_string(),
            1,
            "test-123".to_string(),
            "TestAggregate".to_string(),
            1,
        );

        context.apply_to_metadata(&mut metadata);

        assert_eq!(metadata.correlation_id, Some("corr-123".to_string()));
        assert_eq!(metadata.actor_id, Some("user-456".to_string()));
        assert_eq!(metadata.metadata.get("custom"), Some(&"value".to_string()));
    }
}
