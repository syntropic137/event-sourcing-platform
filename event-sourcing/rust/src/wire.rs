//! The cross-language event envelope (ADR-027).
//!
//! Every SDK (TypeScript, Python, Rust) writes and reads events in one
//! encoding, so any SDK can read any stream:
//!
//! | `EventData` field        | Value                                                  |
//! |--------------------------|--------------------------------------------------------|
//! | `payload`                | UTF-8 JSON **object** holding only the event's fields  |
//! | `meta.event_type`        | stable event name, e.g. `"MoneyDeposited"`             |
//! | `meta.event_version`     | schema version, `>= 1` (`0` is read as `1`)            |
//! | `meta.content_type`      | `"application/json"` (`""` is read as JSON)            |
//! | `meta.aggregate_type`    | stable aggregate name, e.g. `"Account"` (no `-`)       |
//! | `meta.event_id`          | lowercase hyphenated UUID                              |
//! | `meta.timestamp_unix_ms` | client event time, Unix milliseconds                   |
//! | `meta.tenant_id`         | tenant of the stream                                   |
//! | `meta.correlation_id`, `causation_id`, `actor_id` | `""` when absent             |
//! | `meta.headers`           | free-form string map                                   |
//!
//! The event type and version live **only** in metadata; the payload is never
//! wrapped in a type tag. Readers dispatch on `(event_type, event_version)`,
//! run [`Upcasters`](crate::upcast::Upcasters) first, and fail with a typed
//! error for anything they cannot decode.

use crate::client::proto;
use crate::error::{Error, Result};

/// Content type of every payload written by the SDKs.
pub const CONTENT_TYPE_JSON: &str = "application/json";

/// True for a valid aggregate type: an ASCII letter followed by ASCII
/// letters, digits, `_` or `.` (for example `"Account"`, `"billing.Invoice"`).
///
/// The TypeScript and Python SDKs address streams as `"{type}-{id}"` and split
/// on the first `-`, so a type must never contain one.
pub const fn is_valid_aggregate_type(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_alphabetic() {
        return false;
    }
    let mut i = 1;
    while i < bytes.len() {
        let b = bytes[i];
        if !(b.is_ascii_alphanumeric() || b == b'_' || b == b'.') {
            return false;
        }
        i += 1;
    }
    true
}

/// True for a valid event type: non-empty printable ASCII without spaces
/// (for example `"MoneyDeposited"` or `"billing.InvoiceIssued"`).
pub const fn is_valid_event_type(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let mut i = 0;
    while i < bytes.len() {
        if !bytes[i].is_ascii_graphic() {
            return false;
        }
        i += 1;
    }
    true
}

/// Event version as read from the wire: proto3 encodes "unset" as `0`, which
/// every SDK treats as version 1.
pub fn normalize_version(event_version: u32) -> u32 {
    event_version.max(1)
}

/// True if `content_type` is JSON. Empty (unset) counts as JSON; media-type
/// parameters (`; charset=utf-8`) and case are ignored.
pub fn is_json_content_type(content_type: &str) -> bool {
    let essence = content_type.split(';').next().unwrap_or("").trim();
    essence.is_empty() || essence.eq_ignore_ascii_case(CONTENT_TYPE_JSON)
}

/// Fail unless the stored payload is JSON.
pub fn check_content_type(event_type: &str, content_type: &str) -> Result<()> {
    if is_json_content_type(content_type) {
        Ok(())
    } else {
        Err(Error::UnsupportedContentType {
            event_type: event_type.to_string(),
            content_type: content_type.to_string(),
        })
    }
}

/// Validate an event before it is written.
pub fn check_outgoing(event_type: &str, event_version: u32, payload: &[u8]) -> Result<()> {
    if !is_valid_event_type(event_type) {
        return Err(Error::invalid_event(format!(
            "event type '{event_type}' must be non-empty printable ASCII without spaces"
        )));
    }
    if event_version == 0 {
        return Err(Error::invalid_event(format!(
            "event '{event_type}' has version 0; versions start at 1"
        )));
    }
    if !is_json_object(payload) {
        return Err(Error::invalid_event(format!(
            "payload of '{event_type}' must be a JSON object (a struct with named fields), \
             not a tagged enum, unit or scalar"
        )));
    }
    Ok(())
}

/// Cheap check that serialized JSON is an object.
pub(crate) fn is_json_object(payload: &[u8]) -> bool {
    let trimmed = payload.trim_ascii();
    trimmed.first() == Some(&b'{') && trimmed.last() == Some(&b'}')
}

/// True if `body` serializes as a plain JSON object: a struct with named
/// fields or a map (including untagged and internally tagged enums, and
/// `#[serde(flatten)]`). False for externally tagged enum variants
/// (`{"Variant":{..}}`, a type tag inside the payload), unit and tuple
/// structs, sequences and scalars. Only the top level is inspected; field
/// values are not serialized.
pub(crate) fn serializes_as_object<T: serde::Serialize + ?Sized>(body: &T) -> bool {
    body.serialize(shape::Probe).is_ok()
}

mod shape {
    use serde::ser::{self, Impossible, Serialize};
    use std::fmt;

    #[derive(Debug)]
    pub struct NotObject;
    impl fmt::Display for NotObject {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("not a JSON object")
        }
    }
    impl std::error::Error for NotObject {}
    impl ser::Error for NotObject {
        fn custom<T: fmt::Display>(_: T) -> Self {
            NotObject
        }
    }

    /// Accepts fields without serializing them.
    pub struct Fields;
    impl ser::SerializeStruct for Fields {
        type Ok = ();
        type Error = NotObject;
        fn serialize_field<T: ?Sized + Serialize>(
            &mut self,
            _: &'static str,
            _: &T,
        ) -> Result<(), NotObject> {
            Ok(())
        }
        fn end(self) -> Result<(), NotObject> {
            Ok(())
        }
    }
    impl ser::SerializeMap for Fields {
        type Ok = ();
        type Error = NotObject;
        fn serialize_key<T: ?Sized + Serialize>(&mut self, _: &T) -> Result<(), NotObject> {
            Ok(())
        }
        fn serialize_value<T: ?Sized + Serialize>(&mut self, _: &T) -> Result<(), NotObject> {
            Ok(())
        }
        fn end(self) -> Result<(), NotObject> {
            Ok(())
        }
    }

    pub struct Probe;

    macro_rules! reject {
        ($($name:ident($($arg:ty),*);)*) => {
            $(fn $name(self, $(_: $arg),*) -> Result<(), NotObject> { Err(NotObject) })*
        };
    }

    impl ser::Serializer for Probe {
        type Ok = ();
        type Error = NotObject;
        type SerializeSeq = Impossible<(), NotObject>;
        type SerializeTuple = Impossible<(), NotObject>;
        type SerializeTupleStruct = Impossible<(), NotObject>;
        type SerializeTupleVariant = Impossible<(), NotObject>;
        type SerializeMap = Fields;
        type SerializeStruct = Fields;
        type SerializeStructVariant = Impossible<(), NotObject>;

        reject! {
            serialize_bool(bool); serialize_i8(i8); serialize_i16(i16); serialize_i32(i32);
            serialize_i64(i64); serialize_u8(u8); serialize_u16(u16); serialize_u32(u32);
            serialize_u64(u64); serialize_f32(f32); serialize_f64(f64); serialize_char(char);
            serialize_str(&str); serialize_bytes(&[u8]); serialize_none(); serialize_unit();
            serialize_unit_struct(&'static str);
            serialize_unit_variant(&'static str, u32, &'static str);
        }

        fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> Result<(), NotObject> {
            value.serialize(self)
        }
        fn serialize_newtype_struct<T: ?Sized + Serialize>(
            self,
            _: &'static str,
            value: &T,
        ) -> Result<(), NotObject> {
            value.serialize(self)
        }
        fn serialize_newtype_variant<T: ?Sized + Serialize>(
            self,
            _: &'static str,
            _: u32,
            _: &'static str,
            _: &T,
        ) -> Result<(), NotObject> {
            Err(NotObject)
        }
        fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, NotObject> {
            Err(NotObject)
        }
        fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, NotObject> {
            Err(NotObject)
        }
        fn serialize_tuple_struct(
            self,
            _: &'static str,
            _: usize,
        ) -> Result<Self::SerializeTupleStruct, NotObject> {
            Err(NotObject)
        }
        fn serialize_tuple_variant(
            self,
            _: &'static str,
            _: u32,
            _: &'static str,
            _: usize,
        ) -> Result<Self::SerializeTupleVariant, NotObject> {
            Err(NotObject)
        }
        fn serialize_map(self, _: Option<usize>) -> Result<Fields, NotObject> {
            Ok(Fields)
        }
        fn serialize_struct(self, _: &'static str, _: usize) -> Result<Fields, NotObject> {
            Ok(Fields)
        }
        fn serialize_struct_variant(
            self,
            _: &'static str,
            _: u32,
            _: &'static str,
            _: usize,
        ) -> Result<Self::SerializeStructVariant, NotObject> {
            Err(NotObject)
        }
    }
}

/// Metadata-derived view of a stored event used by decoders.
pub(crate) struct Incoming<'a> {
    pub event_type: &'a str,
    pub event_version: u32,
    pub payload: &'a [u8],
}

impl<'a> Incoming<'a> {
    pub(crate) fn from_proto(meta: &'a proto::EventMetadata, payload: &'a [u8]) -> Result<Self> {
        check_content_type(&meta.event_type, &meta.content_type)?;
        Ok(Self {
            event_type: &meta.event_type,
            event_version: normalize_version(meta.event_version),
            payload,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregate_type_rules() {
        for ok in ["Account", "Order2", "billing.Invoice", "Order_Line"] {
            assert!(is_valid_aggregate_type(ok), "{ok}");
        }
        for bad in ["", "order-line", "2Order", "Order Line", "Ordér", "_Order"] {
            assert!(!is_valid_aggregate_type(bad), "{bad}");
        }
    }

    #[test]
    fn event_type_rules() {
        assert!(is_valid_event_type("MoneyDeposited"));
        assert!(is_valid_event_type("billing.invoice-issued"));
        assert!(!is_valid_event_type(""));
        assert!(!is_valid_event_type("Money Deposited"));
        assert!(!is_valid_event_type("Dépôt"));
    }

    #[test]
    fn content_types() {
        assert!(is_json_content_type(""));
        assert!(is_json_content_type("application/json"));
        assert!(is_json_content_type("Application/JSON; charset=utf-8"));
        assert!(!is_json_content_type("application/octet-stream"));
        assert!(!is_json_content_type("application/x-protobuf"));
        assert!(matches!(
            check_content_type("E", "application/x-protobuf"),
            Err(Error::UnsupportedContentType { .. })
        ));
    }

    #[test]
    fn outgoing_validation() {
        assert!(check_outgoing("E", 1, br#"{"a":1}"#).is_ok());
        assert!(check_outgoing("E", 1, b"{}").is_ok());
        assert!(check_outgoing("E", 0, b"{}").is_err());
        assert!(check_outgoing("", 1, b"{}").is_err());
        assert!(check_outgoing("E", 1, b"null").is_err());
        assert!(check_outgoing("E", 1, b"\"E\"").is_err());
        assert!(check_outgoing("E", 1, b"[1]").is_err());
    }

    #[test]
    fn version_zero_reads_as_one() {
        assert_eq!(normalize_version(0), 1);
        assert_eq!(normalize_version(3), 3);
    }
}
