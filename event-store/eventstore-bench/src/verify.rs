//! Completeness and ordering checks run after every loaded scenario.
//!
//! The store is read back through the public `ReadAll` RPC and compared with
//! what the writers were acknowledged for, and (for subscriptions) with what
//! every subscriber actually received.

use std::collections::{HashMap, HashSet};

use serde::Serialize;

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
    let mut out = Vec::new();
    let mut from = 0u64;
    loop {
        let page = client
            .read_all(pb::ReadAllRequest {
                tenant_id: tenant.to_owned(),
                from_global_nonce: from,
                max_count: 1000,
                forward: true,
            })
            .await?
            .into_inner();
        out.extend(page.events.iter().filter_map(to_stored));
        if page.is_end || page.events.is_empty() {
            break;
        }
        from = page.next_from_global_nonce;
    }
    Ok(out)
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
