//! Backend conformance suite for append idempotency, optimistic
//! concurrency (ADR-028) and read pagination (#403).
//!
//! Every backend must give the same answer to the same sequence of appends.
//! Each case is a public async fn taking a fresh store; it uses its own
//! random tenant, so a shared database is fine. A backend runs the whole
//! suite from an integration test with:
//!
//! ```ignore
//! async fn store() -> std::sync::Arc<dyn eventstore_core::EventStore> { /* ... */ }
//! eventstore_core::append_conformance_tests!(store);
//! ```
//!
//! Enabled by the `conformance` feature (dev-dependency only).

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use prost::Message;

use crate::fingerprint::canonical_metadata_bytes;
use crate::proto::{
    AppendRequest, AppendResponse, EventData, EventMetadata, ReadAllRequest, ReadStreamRequest,
};
use crate::{EventStore, StoreError};

/// Store under test.
pub type Store = Arc<dyn EventStore>;

const AGGREGATE_TYPE: &str = "ConformanceAccount";

/// Generates one `#[tokio::test]` per conformance case. `$factory` is a path
/// to an `async fn() -> Arc<dyn EventStore>` that returns a ready store.
#[macro_export]
macro_rules! append_conformance_tests {
    ($factory:path) => {
        $crate::__append_conformance_case!(
            $factory,
            identical_retry_with_shuffled_headers_returns_original_ack
        );
        $crate::__append_conformance_case!(
            $factory,
            retry_after_lost_ack_returns_original_ack_after_stream_advanced
        );
        $crate::__append_conformance_case!($factory, same_key_different_payload_is_already_exists);
        $crate::__append_conformance_case!(
            $factory,
            stale_expected_revision_without_matching_key_is_concurrency_conflict
        );
        $crate::__append_conformance_case!($factory, idempotency_keys_are_scoped_per_aggregate);
        $crate::__append_conformance_case!($factory, concurrent_identical_retries_commit_once);
        $crate::__append_conformance_case!($factory, concurrent_unkeyed_writers_one_wins);
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __append_conformance_case {
    ($factory:path, $name:ident) => {
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn $name() {
            let store = $factory().await;
            $crate::conformance::$name(store).await;
        }
    };
}

fn unique(prefix: &str) -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!(
        "{prefix}-{}-{nanos}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    )
}

/// Header pairs used by the multi-header cases. Eight entries make the
/// generated `HashMap` encoding order differ between instances with
/// overwhelming probability (asserted where it matters).
fn header_pairs() -> Vec<(String, String)> {
    (0..8)
        .map(|i| (format!("x-header-{i}"), format!("value-{i}")))
        .collect()
}

/// A fresh `HashMap` (new hasher seed) filled in a rotated order.
fn headers(rotation: usize) -> HashMap<String, String> {
    let mut pairs = header_pairs();
    let len = pairs.len();
    pairs.rotate_left(rotation % len);
    pairs.into_iter().collect()
}

struct Stream {
    tenant: String,
    aggregate: String,
}

impl Stream {
    fn new() -> Self {
        Self {
            tenant: unique("conf-tenant"),
            aggregate: unique("conf-agg"),
        }
    }

    fn other_aggregate(&self) -> Self {
        Self {
            tenant: self.tenant.clone(),
            aggregate: unique("conf-agg"),
        }
    }

    /// Event `nonce` of this stream; `tag` distinguishes logically different
    /// events at the same position (event id and payload).
    fn event(&self, nonce: u64, tag: &str, headers: HashMap<String, String>) -> EventData {
        EventData {
            meta: Some(EventMetadata {
                event_id: format!("{}-{nonce}-{tag}", self.aggregate),
                aggregate_id: self.aggregate.clone(),
                aggregate_type: AGGREGATE_TYPE.into(),
                aggregate_nonce: nonce,
                event_type: "Deposited".into(),
                event_version: 1,
                content_type: "application/json".into(),
                tenant_id: self.tenant.clone(),
                headers,
                ..Default::default()
            }),
            payload: format!("{{\"tag\":\"{tag}\",\"nonce\":{nonce}}}").into_bytes(),
        }
    }

    fn request(&self, expected: u64, key: &str, events: Vec<EventData>) -> AppendRequest {
        AppendRequest {
            tenant_id: self.tenant.clone(),
            aggregate_id: self.aggregate.clone(),
            aggregate_type: AGGREGATE_TYPE.into(),
            expected_aggregate_nonce: expected,
            idempotency_key: key.into(),
            events,
        }
    }

    /// Batch of `count` events after `expected`, every event carrying the
    /// multi-header map in `rotation` order.
    fn batch(&self, expected: u64, count: u64, tag: &str, rotation: usize) -> Vec<EventData> {
        (1..=count)
            .map(|i| self.event(expected + i, tag, headers(rotation + i as usize)))
            .collect()
    }

    async fn event_ids(&self, store: &Store) -> Vec<String> {
        let resp = store
            .read_stream(ReadStreamRequest {
                tenant_id: self.tenant.clone(),
                aggregate_id: self.aggregate.clone(),
                from_aggregate_nonce: 1,
                max_count: 1000,
                forward: true,
            })
            .await
            .expect("read_stream");
        resp.events
            .into_iter()
            .map(|e| e.meta.expect("meta").event_id)
            .collect()
    }
}

fn assert_already_exists(result: Result<AppendResponse, StoreError>, context: &str) {
    match result {
        Err(StoreError::AlreadyExists(_)) => {}
        other => panic!("{context}: expected AlreadyExists, got {other:?}"),
    }
}

fn assert_conflict(result: Result<AppendResponse, StoreError>, actual_head: u64, context: &str) {
    match result {
        Err(StoreError::Concurrency { detail, .. }) => {
            let detail = detail.unwrap_or_else(|| panic!("{context}: conflict without detail"));
            assert_eq!(
                detail.actual_last_aggregate_nonce, actual_head,
                "{context}: detail reports the current head"
            );
        }
        other => panic!("{context}: expected Concurrency, got {other:?}"),
    }
}

/// An identical retry (same key, same events) whose headers map is a
/// different instance, so it encodes in a different order, returns the
/// original acknowledgment and writes nothing (#362).
pub async fn identical_retry_with_shuffled_headers_returns_original_ack(store: Store) {
    let s = Stream::new();
    let first = store
        .append(s.request(0, "k-shuffle", s.batch(0, 3, "a", 0)))
        .await
        .expect("first append");

    let mut encodings = HashSet::new();
    for rotation in 1..=16 {
        let retry = s.request(0, "k-shuffle", s.batch(0, 3, "a", rotation));
        encodings.insert(retry.encode_to_vec());
        let ack = store.append(retry).await.expect("identical retry");
        assert_eq!(ack, first, "retry {rotation} returns the original ack");
    }
    // Guard against a false pass: the retries really were encoded in
    // different header orders.
    assert!(
        encodings.len() > 1,
        "test precondition: header encoding order varied between retries"
    );
    assert_eq!(s.event_ids(&store).await.len(), 3, "nothing duplicated");
}

/// Idempotency is checked before the concurrency precondition: a retry
/// after a lost acknowledgment returns the original ack even though other
/// writers have since advanced the stream past its expected revision (#363).
pub async fn retry_after_lost_ack_returns_original_ack_after_stream_advanced(store: Store) {
    let s = Stream::new();
    let original = s.request(0, "k-lost-ack", s.batch(0, 2, "mine", 0));
    let first = store.append(original.clone()).await.expect("first append");
    assert_eq!(first.last_aggregate_nonce, 2);

    // Other writers advance the stream (one keyed, one not).
    store
        .append(s.request(2, "", s.batch(2, 1, "other", 0)))
        .await
        .expect("other writer");
    store
        .append(s.request(3, "k-other", s.batch(3, 1, "other2", 0)))
        .await
        .expect("keyed other writer");

    // The retry still says expected 0, which is now stale; headers differ in
    // order from the original.
    let retry = s.request(0, "k-lost-ack", s.batch(0, 2, "mine", 5));
    let ack = store.append(retry).await.expect("retry after lost ack");
    assert_eq!(ack, first, "original ack, not a conflict");
    assert_eq!(s.event_ids(&store).await.len(), 4, "nothing duplicated");
}

/// Reusing a key for a different batch is `AlreadyExists`, whether or not
/// the new batch's expected revision is current, and writes nothing.
pub async fn same_key_different_payload_is_already_exists(store: Store) {
    let s = Stream::new();
    store
        .append(s.request(0, "k-reuse", s.batch(0, 1, "a", 0)))
        .await
        .expect("first append");

    // Stale expected revision: the key decides, not the conflict.
    assert_already_exists(
        store
            .append(s.request(0, "k-reuse", s.batch(0, 1, "b", 0)))
            .await,
        "different payload, stale expected",
    );
    // Current expected revision: still a key reuse.
    assert_already_exists(
        store
            .append(s.request(1, "k-reuse", s.batch(1, 1, "c", 0)))
            .await,
        "different payload, current expected",
    );
    // Same events, different metadata (a header value changed).
    let mut changed = s.batch(0, 1, "a", 0);
    changed[0]
        .meta
        .as_mut()
        .unwrap()
        .headers
        .insert("x-header-0".into(), "changed".into());
    assert_already_exists(
        store.append(s.request(0, "k-reuse", changed)).await,
        "different header value",
    );
    assert_eq!(s.event_ids(&store).await.len(), 1, "nothing written");

    // Bytes moved from metadata into the payload are a different batch (the
    // fingerprint frames each part; an unframed hash collides here).
    let t = Stream::new();
    let single_header = HashMap::from([("h".to_owned(), "v".to_owned())]);
    let original = t.event(1, "a", single_header);
    store
        .append(t.request(0, "k-smuggle", vec![original.clone()]))
        .await
        .expect("first append");
    let mut moved = original.clone();
    moved.meta.as_mut().unwrap().headers.clear();
    let with = canonical_metadata_bytes(original.meta.as_ref().unwrap());
    let without = canonical_metadata_bytes(moved.meta.as_ref().unwrap());
    assert!(with.starts_with(&without), "test precondition");
    moved.payload = [&with[without.len()..], original.payload.as_slice()].concat();
    assert_already_exists(
        store.append(t.request(0, "k-smuggle", vec![moved])).await,
        "header bytes moved into the payload",
    );
}

/// Without a matching key (no key, or a key never seen) a stale expected
/// revision is an ordinary optimistic concurrency conflict.
pub async fn stale_expected_revision_without_matching_key_is_concurrency_conflict(store: Store) {
    let s = Stream::new();
    store
        .append(s.request(0, "", s.batch(0, 2, "a", 0)))
        .await
        .expect("first append");

    assert_conflict(
        store.append(s.request(0, "", s.batch(0, 1, "b", 0))).await,
        2,
        "no key, expected 0 on existing stream",
    );
    assert_conflict(
        store.append(s.request(1, "", s.batch(1, 1, "b", 0))).await,
        2,
        "no key, stale expected",
    );
    assert_conflict(
        store.append(s.request(7, "", s.batch(7, 1, "b", 0))).await,
        2,
        "no key, expected ahead of head",
    );
    assert_conflict(
        store
            .append(s.request(1, "k-fresh", s.batch(1, 1, "b", 0)))
            .await,
        2,
        "unused key, stale expected",
    );
    // The failed keyed attempt did not claim its key.
    let ok = store
        .append(s.request(2, "k-fresh", s.batch(2, 1, "c", 0)))
        .await
        .expect("key still free after a conflict");
    assert_eq!(ok.last_aggregate_nonce, 3);
    assert_eq!(s.event_ids(&store).await.len(), 3);
}

/// Keys are scoped to (tenant, aggregate): the same key on another stream
/// is a new request.
pub async fn idempotency_keys_are_scoped_per_aggregate(store: Store) {
    let a = Stream::new();
    let b = a.other_aggregate();
    let ack_a = store
        .append(a.request(0, "k-shared", a.batch(0, 1, "a", 0)))
        .await
        .expect("append a");
    let ack_b = store
        .append(b.request(0, "k-shared", b.batch(0, 1, "b", 0)))
        .await
        .expect("same key, other aggregate");
    assert_ne!(ack_a.last_global_nonce, ack_b.last_global_nonce);
    assert_eq!(b.event_ids(&store).await.len(), 1);
}

const CONCURRENT_RETRIES: usize = 8;

async fn race_identical(store: &Store, req: AppendRequest) -> Vec<AppendResponse> {
    let barrier = Arc::new(tokio::sync::Barrier::new(CONCURRENT_RETRIES));
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..CONCURRENT_RETRIES {
        let store = store.clone();
        let req = req.clone();
        let barrier = barrier.clone();
        tasks.spawn(async move {
            barrier.wait().await;
            store.append(req).await
        });
    }
    let mut acks = Vec::new();
    while let Some(joined) = tasks.join_next().await {
        acks.push(
            joined
                .expect("task")
                .expect("an in-flight identical retry returns the ack"),
        );
    }
    acks
}

/// Identical keyed requests in flight at the same time (a client retry that
/// overtakes the original) commit once, and every one of them gets the
/// committed ack, on a new stream and on an existing one.
///
/// Best effort: the barrier aligns the starts, not the backend's internal
/// steps, so a run can pass without the racers overlapping. Backends with
/// internal race windows must also force the interleavings in their own
/// tests (Postgres: `tests/it_idempotency.rs`).
pub async fn concurrent_identical_retries_commit_once(store: Store) {
    let s = Stream::new();
    let acks = race_identical(&store, s.request(0, "k-race-new", s.batch(0, 2, "a", 0))).await;
    assert!(acks.windows(2).all(|w| w[0] == w[1]), "one ack: {acks:?}");
    assert_eq!(acks[0].last_aggregate_nonce, 2);
    assert_eq!(s.event_ids(&store).await.len(), 2);

    let acks = race_identical(
        &store,
        s.request(2, "k-race-existing", s.batch(2, 2, "b", 0)),
    )
    .await;
    assert!(acks.windows(2).all(|w| w[0] == w[1]), "one ack: {acks:?}");
    assert_eq!(acks[0].last_aggregate_nonce, 4);
    assert_eq!(s.event_ids(&store).await.len(), 4);
}

/// Different unkeyed writers racing for the same revision: exactly one
/// wins, the rest get a concurrency conflict.
pub async fn concurrent_unkeyed_writers_one_wins(store: Store) {
    let s = Arc::new(Stream::new());
    let barrier = Arc::new(tokio::sync::Barrier::new(CONCURRENT_RETRIES));
    let mut tasks = tokio::task::JoinSet::new();
    for i in 0..CONCURRENT_RETRIES {
        let store = store.clone();
        let req = s.request(0, "", s.batch(0, 1, &format!("w{i}"), 0));
        let barrier = barrier.clone();
        tasks.spawn(async move {
            barrier.wait().await;
            store.append(req).await
        });
    }
    let (mut ok, mut conflicts) = (0, 0);
    while let Some(joined) = tasks.join_next().await {
        match joined.expect("task") {
            Ok(_) => ok += 1,
            Err(StoreError::Concurrency { .. }) => conflicts += 1,
            Err(e) => panic!("unexpected error: {e:?}"),
        }
    }
    assert_eq!((ok, conflicts), (1, CONCURRENT_RETRIES - 1));
    assert_eq!(s.event_ids(&store).await.len(), 1);
}

// ---------------------------------------------------------------------------
// Read pagination (#403)
// ---------------------------------------------------------------------------

/// Generates one `#[tokio::test]` per read pagination conformance case.
#[macro_export]
macro_rules! read_conformance_tests {
    ($factory:path) => {
        $crate::__append_conformance_case!($factory, read_stream_pages_forward);
        $crate::__append_conformance_case!($factory, read_stream_pages_backward);
        $crate::__append_conformance_case!($factory, read_stream_empty_and_unbounded);
        $crate::__append_conformance_case!($factory, read_all_pages_forward);
        $crate::__append_conformance_case!($factory, read_all_pages_backward);
        $crate::__append_conformance_case!($factory, forward_end_cursor_resumes_after_append);
        $crate::__append_conformance_case!($factory, read_all_page_size_default_and_cap);
    };
}

/// Events in the paged stream. Page sizes 2, 3 and 6 divide it exactly;
/// 4 and 10 leave a partial last page.
const PAGED_EVENTS: u64 = 6;
/// Page sizes tried by every pagination case.
const PAGE_SIZES: [u32; 6] = [1, 2, 3, 4, 6, 10];

/// One page of a paged read, direction-agnostic.
struct Page {
    positions: Vec<u64>,
    is_end: bool,
    next: u64,
}

/// Follows `next` from `from` until `is_end` and checks the paging contract:
/// pages concatenate to exactly `expected` (no duplicate, no gap, in
/// order), every page but the last is full, `is_end` is set on the page
/// that holds the last event (not one empty page later), and the call
/// count is the minimum.
async fn assert_pages<F, Fut>(read: F, from: u64, size: u32, expected: &[u64], context: &str) -> u64
where
    F: Fn(u64) -> Fut,
    Fut: std::future::Future<Output = Page>,
{
    let mut cursor = from;
    let mut seen = Vec::new();
    let mut calls = 0usize;
    loop {
        calls += 1;
        assert!(
            calls <= expected.len() + 1,
            "{context}: paging did not terminate; read so far {seen:?}"
        );
        let page = read(cursor).await;
        assert!(
            page.positions.len() <= size as usize,
            "{context}: page of {} exceeds max_count",
            page.positions.len()
        );
        seen.extend_from_slice(&page.positions);
        let is_end = page.is_end;
        if !is_end {
            assert_eq!(
                page.positions.len(),
                size as usize,
                "{context}: page before the end is full (cursor {cursor})"
            );
        }
        cursor = page.next;
        if is_end {
            break;
        }
    }
    assert_eq!(seen, expected, "{context}: concatenated pages");
    let min_calls = expected.len().div_ceil(size as usize).max(1);
    assert_eq!(calls, min_calls, "{context}: is_end on the last page");
    cursor
}

/// The cursor the final page must return: one past the last event, or, for
/// an empty read, `start` forward (the normalized request position) and 0
/// backward.
fn end_cursor(forward: bool, start: u64, expected: &[u64]) -> u64 {
    match (forward, expected.last()) {
        (true, Some(last)) => last + 1,
        (true, None) => start,
        (false, Some(last)) => last - 1,
        (false, None) => 0,
    }
}

/// Runs `assert_pages` and checks the final cursor.
#[allow(clippy::too_many_arguments)]
async fn assert_paging<F, Fut>(
    read: F,
    from: u64,
    start: u64,
    forward: bool,
    size: u32,
    expected: &[u64],
    context: &str,
) where
    F: Fn(u64) -> Fut,
    Fut: std::future::Future<Output = Page>,
{
    let end = assert_pages(read, from, size, expected, context).await;
    assert_eq!(
        end,
        end_cursor(forward, start, expected),
        "{context}: final cursor"
    );
}

async fn read_stream_page(store: &Store, s: &Stream, from: u64, size: u32, forward: bool) -> Page {
    let resp = store
        .read_stream(ReadStreamRequest {
            tenant_id: s.tenant.clone(),
            aggregate_id: s.aggregate.clone(),
            from_aggregate_nonce: from,
            max_count: size,
            forward,
        })
        .await
        .expect("read_stream");
    Page {
        positions: resp
            .events
            .iter()
            .map(|e| e.meta.as_ref().expect("meta").aggregate_nonce)
            .collect(),
        is_end: resp.is_end,
        next: resp.next_from_aggregate_nonce,
    }
}

async fn paged_stream(store: &Store) -> Stream {
    let s = Stream::new();
    store
        .append(s.request(0, "", s.batch(0, PAGED_EVENTS, "p", 0)))
        .await
        .expect("append paged stream");
    s
}

/// Forward `ReadStream` pages from the start, the middle, the head and
/// past the head.
pub async fn read_stream_pages_forward(store: Store) {
    let s = paged_stream(&store).await;
    for from in [0, 1, 2, 4, PAGED_EVENTS, PAGED_EVENTS + 1, u64::MAX] {
        let expected: Vec<u64> = (from.max(1)..=PAGED_EVENTS).collect();
        for size in PAGE_SIZES {
            assert_paging(
                |cursor| read_stream_page(&store, &s, cursor, size, true),
                from,
                from.max(1),
                true,
                size,
                &expected,
                &format!("forward from {from} size {size}"),
            )
            .await;
        }
    }
}

/// Backward `ReadStream` pages from the head, the middle, version 1,
/// beyond the head (head probes) and from 0 (before every event).
pub async fn read_stream_pages_backward(store: Store) {
    let s = paged_stream(&store).await;
    for from in [
        PAGED_EVENTS,
        PAGED_EVENTS - 1,
        3,
        2,
        1,
        0,
        PAGED_EVENTS + 1,
        i64::MAX as u64,
        u64::MAX,
    ] {
        let expected: Vec<u64> = (1..=from.min(PAGED_EVENTS)).rev().collect();
        for size in PAGE_SIZES {
            assert_paging(
                |cursor| read_stream_page(&store, &s, cursor, size, false),
                from,
                from,
                false,
                size,
                &expected,
                &format!("backward from {from} size {size}"),
            )
            .await;
        }
    }
}

/// An unknown stream is one empty, final page in both directions, and
/// `max_count` 0 means no limit.
pub async fn read_stream_empty_and_unbounded(store: Store) {
    let missing = Stream::new();
    for forward in [true, false] {
        for from in [0, 1, 5] {
            let page = read_stream_page(&store, &missing, from, 10, forward).await;
            assert!(page.positions.is_empty(), "missing stream has no events");
            assert!(page.is_end, "missing stream: is_end (forward {forward})");
        }
    }

    let s = paged_stream(&store).await;
    let page = read_stream_page(&store, &s, 1, 0, true).await;
    assert_eq!(page.positions, (1..=PAGED_EVENTS).collect::<Vec<_>>());
    assert!(page.is_end, "max_count 0 forward reads to the end");
    let page = read_stream_page(&store, &s, PAGED_EVENTS, 0, false).await;
    assert_eq!(page.positions, (1..=PAGED_EVENTS).rev().collect::<Vec<_>>());
    assert!(page.is_end, "max_count 0 backward reads to the start");
}

async fn read_all_page(store: &Store, tenant: &str, from: u64, size: u32, forward: bool) -> Page {
    let resp = store
        .read_all(ReadAllRequest {
            tenant_id: tenant.to_owned(),
            from_global_nonce: from,
            max_count: size,
            forward,
        })
        .await
        .expect("read_all");
    Page {
        positions: resp
            .events
            .iter()
            .map(|e| e.meta.as_ref().expect("meta").global_nonce)
            .collect(),
        is_end: resp.is_end,
        next: resp.next_from_global_nonce,
    }
}

/// A tenant whose events interleave two aggregates and whose global
/// positions are not contiguous (another tenant writes in between).
/// Returns the tenant and its global positions, ascending.
async fn paged_tenant(store: &Store) -> (String, Vec<u64>) {
    let a = Stream::new();
    let b = a.other_aggregate();
    let noise = Stream::new();
    let mut globals = Vec::new();
    let mut heads = [0u64, 0u64];
    for i in 0..PAGED_EVENTS {
        let (s, head) = if i % 2 == 0 {
            (&a, &mut heads[0])
        } else {
            (&b, &mut heads[1])
        };
        let ack = store
            .append(s.request(*head, "", s.batch(*head, 1, "g", 0)))
            .await
            .expect("append tenant event");
        *head += 1;
        globals.push(ack.last_global_nonce);
        store
            .append(noise.request(i, "", noise.batch(i, 1, "n", 0)))
            .await
            .expect("append other tenant");
    }
    (a.tenant, globals)
}

/// Forward `ReadAll` pages over a tenant with gaps in its global positions.
pub async fn read_all_pages_forward(store: Store) {
    let (tenant, globals) = paged_tenant(&store).await;
    let head = *globals.last().unwrap();
    for from in [
        0,
        globals[0],
        globals[2],
        globals[3] - 1,
        head,
        head + 1,
        u64::MAX,
    ] {
        let expected: Vec<u64> = globals.iter().copied().filter(|g| *g >= from).collect();
        for size in PAGE_SIZES {
            assert_paging(
                |cursor| read_all_page(&store, &tenant, cursor, size, true),
                from,
                from,
                true,
                size,
                &expected,
                &format!("read_all forward from {from} size {size}"),
            )
            .await;
        }
    }
}

/// Backward `ReadAll` pages from head probes, the head, the middle, the
/// first event and before it.
pub async fn read_all_pages_backward(store: Store) {
    let (tenant, globals) = paged_tenant(&store).await;
    let head = *globals.last().unwrap();
    for from in [
        i64::MAX as u64,
        u64::MAX,
        head,
        globals[3],
        globals[3] - 1,
        globals[0],
        globals[0] - 1,
        0,
    ] {
        let expected: Vec<u64> = globals
            .iter()
            .rev()
            .copied()
            .filter(|g| *g <= from)
            .collect();
        for size in PAGE_SIZES {
            assert_paging(
                |cursor| read_all_page(&store, &tenant, cursor, size, false),
                from,
                from,
                false,
                size,
                &expected,
                &format!("read_all backward from {from} size {size}"),
            )
            .await;
        }
    }
}

/// The cursor on a final forward page is where the next append lands, so a
/// reader polling the tail sees each new event exactly once.
pub async fn forward_end_cursor_resumes_after_append(store: Store) {
    let s = paged_stream(&store).await;
    let end = read_stream_page(&store, &s, 1, 10, true).await;
    assert!(end.is_end);
    let tail = read_all_page(&store, &s.tenant, 0, 10, true).await;
    assert!(tail.is_end);

    // Polling the tail before anything new is empty and keeps the cursor.
    let empty = read_stream_page(&store, &s, end.next, 10, true).await;
    assert!(empty.positions.is_empty() && empty.is_end);
    assert_eq!(empty.next, end.next, "read_stream empty poll keeps cursor");
    let empty = read_all_page(&store, &s.tenant, tail.next, 10, true).await;
    assert!(empty.positions.is_empty() && empty.is_end);
    assert_eq!(empty.next, tail.next, "read_all empty poll keeps cursor");

    let ack = store
        .append(s.request(PAGED_EVENTS, "", s.batch(PAGED_EVENTS, 1, "tail", 0)))
        .await
        .expect("append after the end");

    let page = read_stream_page(&store, &s, end.next, 10, true).await;
    assert_eq!(page.positions, vec![PAGED_EVENTS + 1], "read_stream tail");
    assert!(page.is_end);
    let page = read_all_page(&store, &s.tenant, tail.next, 10, true).await;
    assert_eq!(page.positions, vec![ack.last_global_nonce], "read_all tail");
    assert!(page.is_end);
}

/// `ReadAll` `max_count` 0 means 100, and larger requests are capped at
/// 1000; `is_end` stays false while events remain.
pub async fn read_all_page_size_default_and_cap(store: Store) {
    let s = Stream::new();
    let total = 1001;
    store
        .append(s.request(0, "", s.batch(0, total, "cap", 0)))
        .await
        .expect("append 1001 events");

    let page = read_all_page(&store, &s.tenant, 0, 0, true).await;
    assert_eq!(page.positions.len(), 100, "default page size");
    assert!(!page.is_end);

    let page = read_all_page(&store, &s.tenant, 0, 5000, true).await;
    assert_eq!(page.positions.len(), 1000, "page size cap");
    assert!(!page.is_end, "one event remains past the cap");
    let rest = read_all_page(&store, &s.tenant, page.next, 5000, true).await;
    assert_eq!(rest.positions.len(), 1);
    assert!(rest.is_end);
}
