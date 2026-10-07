//! Completeness and ordering checks run after every loaded scenario.
//!
//! The store is read back through the public `ReadAll` RPC and compared with
//! what the writers were acknowledged for, and (for subscriptions) with what
//! every subscriber actually received.

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use std::time::Instant;

use anyhow::Context as _;

use crate::replay::READ_ALL_PAGE;
use crate::stats::{self, Hist};
use crate::{pb, Client};

#[derive(Debug, Clone)]
pub struct Stored {
    pub global: u64,
    pub aggregate_id: String,
    pub aggregate_nonce: u64,
    pub event_id: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Verify {
    pub store_events: u64,
    pub acked: u64,
    /// Acknowledged appends whose events are not in the store. Must be 0.
    pub missing_acked: u64,
    /// Events in the store nobody wrote (or got an error for). Must be 0.
    pub unexpected: u64,
    /// Events of failed appends that committed anyway (allowed, reported).
    pub uncertain_committed: u64,
    /// Non-increasing `global_nonce` in tenant read order. Must be 0.
    pub global_order_violations: u64,
    /// Per-aggregate nonce sequences that skip or repeat. Must be 0.
    pub aggregate_gaps: u64,
    pub ok: bool,
}

impl Verify {
    pub fn merge(&mut self, o: &Verify) {
        self.store_events += o.store_events;
        self.acked += o.acked;
        self.missing_acked += o.missing_acked;
        self.unexpected += o.unexpected;
        self.uncertain_committed += o.uncertain_committed;
        self.global_order_violations += o.global_order_violations;
        self.aggregate_gaps += o.aggregate_gaps;
        self.ok = self.is_clean();
    }

    fn is_clean(&self) -> bool {
        self.missing_acked == 0
            && self.unexpected == 0
            && self.global_order_violations == 0
            && self.aggregate_gaps == 0
    }

    pub fn empty_ok() -> Self {
        Self {
            ok: true,
            ..Default::default()
        }
    }
}

pub fn check(store: &[Stored], acked: &HashSet<String>, uncertain: &HashSet<String>) -> Verify {
    let mut v = Verify {
        store_events: store.len() as u64,
        acked: acked.len() as u64,
        ..Default::default()
    };
    let mut prev_global = 0u64;
    let mut heads: HashMap<&str, u64> = HashMap::new();
    let mut seen: HashSet<&str> = HashSet::with_capacity(store.len());
    for (i, e) in store.iter().enumerate() {
        if i > 0 && e.global <= prev_global {
            v.global_order_violations += 1;
        }
        prev_global = e.global;
        let head = heads.entry(e.aggregate_id.as_str()).or_insert(0);
        if e.aggregate_nonce != *head + 1 {
            v.aggregate_gaps += 1;
        }
        *head = e.aggregate_nonce;
        seen.insert(e.event_id.as_str());
        if !acked.contains(&e.event_id) {
            if uncertain.contains(&e.event_id) {
                v.uncertain_committed += 1;
            } else {
                v.unexpected += 1;
            }
        }
    }
    v.missing_acked = acked
        .iter()
        .filter(|id| !seen.contains(id.as_str()))
        .count() as u64;
    v.ok = v.is_clean();
    v
}

/// Result of comparing a subscriber's delivered sequence to the store's.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SeqCheck {
    pub expected: u64,
    pub delivered: u64,
    pub missing: u64,
    pub duplicates: u64,
    pub out_of_order: u64,
    /// Deliveries of nonces the store does not hold for the tenant.
    pub extra: u64,
    /// Delivered sequence equals the store's tenant order exactly.
    pub exact: bool,
}

pub fn compare_sequences(store: &[u64], seen: &[u64]) -> SeqCheck {
    let mut c = SeqCheck {
        expected: store.len() as u64,
        delivered: seen.len() as u64,
        ..Default::default()
    };
    let mut uniq = HashSet::with_capacity(seen.len());
    for (i, g) in seen.iter().enumerate() {
        if !uniq.insert(*g) {
            c.duplicates += 1;
        }
        if i > 0 && *g <= seen[i - 1] {
            c.out_of_order += 1;
        }
    }
    c.missing = store.iter().filter(|g| !uniq.contains(g)).count() as u64;
    let in_store: HashSet<&u64> = store.iter().collect();
    c.extra = seen.iter().filter(|g| !in_store.contains(g)).count() as u64;
    c.exact = store == seen;
    c
}

pub fn to_stored(e: &pb::EventData) -> Option<Stored> {
    let m = e.meta.as_ref()?;
    Some(Stored {
        global: m.global_nonce,
        aggregate_id: m.aggregate_id.clone(),
        aggregate_nonce: m.aggregate_nonce,
        event_id: m.event_id.clone(),
    })
}

/// Reads one tenant's whole log in global order, 1000 events per page.
pub async fn read_tenant(client: &mut Client, tenant: &str) -> anyhow::Result<Vec<Stored>> {
    read_tenant_timed(client, tenant, None).await
}

/// Next forward `ReadAll` cursor after a non-empty page. Errors unless it
/// moves strictly past both the request cursor and the page's last event, so
/// a misbehaving server cannot loop the reader forever or re-serve events.
pub fn next_cursor(from: u64, page: &[Stored], next: u64) -> anyhow::Result<u64> {
    let last = page.last().map_or(0, |s| s.global);
    anyhow::ensure!(
        next > from && next > last,
        "ReadAll cursor did not advance (from {from}, last event {last}, next {next})"
    );
    Ok(next)
}

/// Pages one tenant's whole log forward, 1000 events per page, recording
/// each page's latency in `hist` when given.
pub async fn read_tenant_timed(
    client: &mut Client,
    tenant: &str,
    mut hist: Option<&mut Hist>,
) -> anyhow::Result<Vec<Stored>> {
    let mut out = Vec::new();
    let mut from = 0u64;
    loop {
        let t0 = Instant::now();
        let page = client
            .read_all(pb::ReadAllRequest {
                tenant_id: tenant.to_owned(),
                from_global_nonce: from,
                max_count: READ_ALL_PAGE,
                forward: true,
            })
            .await?
            .into_inner();
        if let Some(h) = hist.as_deref_mut() {
            stats::record(h, t0.elapsed());
        }
        let stored = page
            .events
            .iter()
            .map(|e| to_stored(e).context("ReadAll returned an event without metadata"))
            .collect::<anyhow::Result<Vec<_>>>()?;
        if page.is_end || stored.is_empty() {
            out.extend(stored);
            break;
        }
        from = next_cursor(from, &stored, page.next_from_global_nonce)?;
        out.extend(stored);
    }
    Ok(out)
}

/// A full-aggregate `ReadStream` response is valid only if it holds exactly
/// the events the bench wrote to that aggregate: right count, tenant,
/// aggregate id/type, contiguous nonces and the matching event ids.
pub fn valid_stream(events: &[pb::EventData], tenant: &str, aggregate_id: &str, n: u64) -> bool {
    events.len() as u64 == n
        && events.iter().enumerate().all(|(i, e)| {
            let nonce = i as u64 + 1;
            e.meta.as_ref().is_some_and(|m| {
                m.aggregate_nonce == nonce
                    && m.aggregate_id == aggregate_id
                    && m.aggregate_type == crate::AGGREGATE_TYPE
                    && m.tenant_id == tenant
                    && m.event_id == crate::event_id(aggregate_id, nonce)
            })
        })
}

pub async fn verify_tenant(
    client: &mut Client,
    tenant: &str,
    acked: &HashSet<String>,
    uncertain: &HashSet<String>,
) -> anyhow::Result<(Verify, Vec<u64>)> {
    let store = read_tenant(client, tenant).await?;
    let v = check(&store, acked, uncertain);
    Ok((v, store.iter().map(|s| s.global).collect()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(global: u64, agg: &str, n: u64) -> Stored {
        Stored {
            global,
            aggregate_id: agg.into(),
            aggregate_nonce: n,
            event_id: format!("{agg}#{n}"),
        }
    }

    fn set(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn clean_log_passes() {
        let store = vec![ev(1, "a", 1), ev(3, "b", 1), ev(7, "a", 2)];
        let v = check(&store, &set(&["a#1", "a#2", "b#1"]), &HashSet::new());
        assert!(v.ok, "{v:?}");
    }

    #[test]
    fn detects_missing_unexpected_order_and_gaps() {
        let store = vec![ev(5, "a", 1), ev(4, "a", 3), ev(9, "z", 1)];
        let v = check(&store, &set(&["a#1", "a#2", "a#3"]), &set(&[]));
        assert_eq!(v.missing_acked, 1);
        assert_eq!(v.unexpected, 1);
        assert_eq!(v.global_order_violations, 1);
        assert_eq!(v.aggregate_gaps, 1);
        assert!(!v.ok);
    }

    #[test]
    fn uncertain_commits_are_tolerated() {
        let store = vec![ev(1, "a", 1)];
        let v = check(&store, &HashSet::new(), &set(&["a#1"]));
        assert!(v.ok);
        assert_eq!(v.uncertain_committed, 1);
    }

    #[test]
    fn cursor_must_advance_past_page() {
        let page = vec![ev(10, "a", 1), ev(12, "a", 2)];
        assert_eq!(next_cursor(0, &page, 13).unwrap(), 13);
        assert!(next_cursor(13, &page, 13).is_err(), "unchanged cursor");
        assert!(
            next_cursor(0, &page, 12).is_err(),
            "would re-serve last event"
        );
        assert!(next_cursor(20, &page, 15).is_err(), "moved backwards");
    }

    fn stream(tenant: &str, agg: &str, ty: &str, n: u64) -> Vec<pb::EventData> {
        (1..=n)
            .map(|i| pb::EventData {
                meta: Some(pb::EventMetadata {
                    event_id: crate::event_id(agg, i),
                    aggregate_id: agg.into(),
                    aggregate_type: ty.into(),
                    aggregate_nonce: i,
                    tenant_id: tenant.into(),
                    ..Default::default()
                }),
                payload: vec![],
            })
            .collect()
    }

    #[test]
    fn read_stream_validity_checks_identity() {
        let ty = crate::AGGREGATE_TYPE;
        assert!(valid_stream(&stream("t", "a", ty, 3), "t", "a", 3));
        assert!(
            !valid_stream(&stream("t", "a", ty, 2), "t", "a", 3),
            "short"
        );
        assert!(
            !valid_stream(&stream("t", "b", ty, 3), "t", "a", 3),
            "wrong aggregate"
        );
        assert!(
            !valid_stream(&stream("t", "a", "Other", 3), "t", "a", 3),
            "wrong type"
        );
        assert!(
            !valid_stream(&stream("u", "a", ty, 3), "t", "a", 3),
            "wrong tenant"
        );
        let mut s = stream("t", "a", ty, 3);
        s[1].meta.as_mut().unwrap().event_id = "someone-else#2".into();
        assert!(!valid_stream(&s, "t", "a", 3), "wrong event identity");
        let mut s = stream("t", "a", ty, 3);
        s.swap(0, 1);
        assert!(!valid_stream(&s, "t", "a", 3), "reordered");
    }

    #[test]
    fn trailing_duplicates_and_extras_break_exactness() {
        let c = compare_sequences(&[1, 2, 5], &[1, 2, 5, 5]);
        assert!(!c.exact);
        assert_eq!(c.duplicates, 1);
        let c = compare_sequences(&[1, 2, 5], &[1, 2, 5, 9]);
        assert!(!c.exact);
        assert_eq!(c.extra, 1);
    }

    #[test]
    fn sequence_compare() {
        let c = compare_sequences(&[1, 2, 5], &[1, 2, 5]);
        assert!(c.exact && c.missing == 0);
        let c = compare_sequences(&[1, 2, 5], &[1, 5, 2, 2]);
        assert!(!c.exact);
        assert_eq!(c.duplicates, 1);
        assert_eq!(c.out_of_order, 2);
        assert_eq!(c.missing, 0);
        let c = compare_sequences(&[1, 2, 5], &[1, 5]);
        assert_eq!(c.missing, 1);
    }
}
