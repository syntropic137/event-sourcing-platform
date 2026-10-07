//! Idempotency fingerprint of an append batch, shared by every backend so an
//! identical retry is recognized the same way everywhere (ADR-028).

use std::collections::BTreeMap;

use prost::encoding::{btree_map, string};
use prost::Message;
use sha2::{Digest, Sha256};

use crate::proto;

/// Protobuf field number of `EventMetadata.headers`.
const HEADERS_FIELD: u32 = 16;

/// Canonical bytes of an event's client-supplied metadata: the protobuf
/// encoding with the server-assigned `recorded_time_unix_ms` and
/// `global_nonce` zeroed and `headers` entries in key order.
///
/// `headers` is a protobuf map, generated as a `HashMap`, and prost encodes
/// it in iteration order. Iteration order differs between map instances, so
/// two decodes of the same request could encode differently: an identical
/// retry with two or more headers was refused as "already used with
/// different payload" (#355).
///
/// With zero or one header the result is byte-identical to
/// `encode_to_vec()` of the normalized metadata (the previous fingerprint
/// input), so fingerprints already stored for such requests stay valid.
pub fn canonical_metadata_bytes(meta: &proto::EventMetadata) -> Vec<u8> {
    let mut m = meta.clone();
    m.recorded_time_unix_ms = 0;
    m.global_nonce = 0;
    let headers: BTreeMap<String, String> = std::mem::take(&mut m.headers).into_iter().collect();
    // Field 17 (`global_nonce`) is zero and therefore omitted, so the headers
    // field is the last one prost would write: appending it here yields the
    // same field order as the generated encoder.
    let mut buf = m.encode_to_vec();
    btree_map::encode(
        string::encode,
        string::encoded_len,
        string::encode,
        string::encoded_len,
        HEADERS_FIELD,
        &headers,
        &mut buf,
    );
    buf
}

/// SHA-256 over each normalized event's [`canonical_metadata_bytes`] followed
/// by its payload. Backends store this per idempotency key and compare it on
/// a retry: equal means "same request", different means the key was reused
/// for another batch (`ALREADY_EXISTS`).
///
/// Input must be the events after server-side normalization (defaults filled
/// in), so a retry that omits a defaultable field matches the original.
///
/// Byte-identical to the fingerprint the Postgres backend has always stored
/// for requests with zero or one header per event (and for every request
/// since #365), so stored idempotency rows stay valid.
pub fn batch_fingerprint(events: &[proto::EventData]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    for ev in events {
        if let Some(meta) = &ev.meta {
            hasher.update(canonical_metadata_bytes(meta));
            hasher.update(&ev.payload);
        }
    }
    hasher.finalize().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn meta(headers: HashMap<String, String>) -> proto::EventMetadata {
        proto::EventMetadata {
            event_id: "e-1".into(),
            aggregate_id: "a-1".into(),
            aggregate_type: "Account".into(),
            aggregate_nonce: 3,
            event_type: "Opened".into(),
            event_version: 2,
            content_type: "application/json".into(),
            tenant_id: "t".into(),
            payload_sha256: vec![1, 2, 3],
            recorded_time_unix_ms: 99,
            global_nonce: 42,
            headers,
            ..Default::default()
        }
    }

    fn normalized_prost(m: &proto::EventMetadata) -> Vec<u8> {
        let mut m = m.clone();
        m.recorded_time_unix_ms = 0;
        m.global_nonce = 0;
        m.encode_to_vec()
    }

    #[test]
    fn matches_previous_encoding_for_zero_or_one_header() {
        let none = meta(HashMap::new());
        assert_eq!(canonical_metadata_bytes(&none), normalized_prost(&none));
        let one = meta(HashMap::from([("schema".to_owned(), "v2".to_owned())]));
        assert_eq!(canonical_metadata_bytes(&one), normalized_prost(&one));
    }

    #[test]
    fn independent_of_header_map_iteration_order() {
        let pairs: Vec<(String, String)> =
            (0..8).map(|i| (format!("h{i}"), format!("v{i}"))).collect();
        let maps: Vec<HashMap<String, String>> =
            (0..50).map(|_| pairs.iter().cloned().collect()).collect();
        let canonical: Vec<Vec<u8>> = maps
            .iter()
            .map(|h| canonical_metadata_bytes(&meta(h.clone())))
            .collect();
        assert!(canonical.windows(2).all(|w| w[0] == w[1]));
        // The generated encoder is not stable across map instances: this is
        // the defect the canonical form fixes.
        let raw: std::collections::HashSet<Vec<u8>> = maps
            .iter()
            .map(|h| normalized_prost(&meta(h.clone())))
            .collect();
        assert!(raw.len() > 1, "prost map encoding order varies");
    }

    fn data(headers: HashMap<String, String>, payload: &[u8]) -> proto::EventData {
        proto::EventData {
            meta: Some(meta(headers)),
            payload: payload.to_vec(),
        }
    }

    #[test]
    fn batch_fingerprint_matches_stored_postgres_fingerprints() {
        // The formula behind every idempotency row Postgres stored before
        // #365: SHA-256 of normalized prost metadata, then payload.
        let legacy = |events: &[proto::EventData]| {
            let mut h = Sha256::new();
            for ev in events {
                h.update(normalized_prost(ev.meta.as_ref().unwrap()));
                h.update(&ev.payload);
            }
            h.finalize().to_vec()
        };
        let batch = vec![
            data(HashMap::new(), b"one"),
            data(
                HashMap::from([("trace".to_owned(), "t-1".to_owned())]),
                b"two",
            ),
        ];
        assert_eq!(batch_fingerprint(&batch), legacy(&batch));
    }

    #[test]
    fn batch_fingerprint_ignores_header_order_but_not_content() {
        let pairs: Vec<(String, String)> =
            (0..8).map(|i| (format!("h{i}"), format!("v{i}"))).collect();
        let build = |payload: &[u8]| data(pairs.iter().cloned().collect(), payload);
        let fps: std::collections::HashSet<Vec<u8>> =
            (0..50).map(|_| batch_fingerprint(&[build(b"p")])).collect();
        assert_eq!(fps.len(), 1);
        let base = batch_fingerprint(&[build(b"p")]);
        assert_ne!(base, batch_fingerprint(&[build(b"q")]));
        let mut changed: HashMap<String, String> = pairs.iter().cloned().collect();
        changed.insert("h0".into(), "other".into());
        assert_ne!(base, batch_fingerprint(&[data(changed, b"p")]));
    }

    #[test]
    fn decodes_back_to_the_same_metadata() {
        let h: HashMap<String, String> = (0..4).map(|i| (format!("k{i}"), "v".into())).collect();
        let m = meta(h);
        let decoded =
            proto::EventMetadata::decode(canonical_metadata_bytes(&m).as_slice()).unwrap();
        let mut expected = m.clone();
        expected.recorded_time_unix_ms = 0;
        expected.global_nonce = 0;
        assert_eq!(decoded, expected);
    }
}
