//! Latency histograms and their summaries.
//!
//! Values are recorded in microseconds into an HDR histogram (3 significant
//! digits, 1us..1h), so percentiles are exact to within 0.1% and are computed
//! over every recorded sample, never over averages of averages.

use std::time::Duration;

use hdrhistogram::Histogram;
use serde::Serialize;

pub type Hist = Histogram<u64>;

pub fn new_hist() -> Hist {
    Histogram::new_with_bounds(1, 3_600_000_000, 3).expect("static histogram bounds are valid")
}

pub fn record(h: &mut Hist, d: Duration) {
    h.saturating_record((d.as_micros() as u64).max(1));
}

pub fn merge(into: &mut Hist, from: &Hist) {
    into.add(from).expect("histograms share bounds");
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Latency {
    pub count: u64,
    pub mean_ms: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub p999_ms: f64,
    pub max_ms: f64,
}

impl Latency {
    pub fn from_hist(h: &Hist) -> Self {
        if h.is_empty() {
            return Self::default();
        }
        let q = |x: f64| h.value_at_quantile(x) as f64 / 1000.0;
        Self {
            count: h.len(),
            mean_ms: h.mean() / 1000.0,
            p50_ms: q(0.50),
            p95_ms: q(0.95),
            p99_ms: q(0.99),
            p999_ms: q(0.999),
            max_ms: h.max() as f64 / 1000.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_come_from_all_samples() {
        let mut h = new_hist();
        for ms in 1..=100u64 {
            record(&mut h, Duration::from_millis(ms));
        }
        let l = Latency::from_hist(&h);
        assert_eq!(l.count, 100);
        assert!((l.p50_ms - 50.0).abs() < 0.1, "{l:?}");
        assert!((l.p95_ms - 95.0).abs() < 0.1, "{l:?}");
        assert!((l.p99_ms - 99.0).abs() < 0.1, "{l:?}");
        assert!((l.max_ms - 100.0).abs() < 0.1, "{l:?}");
    }

    #[test]
    fn empty_histogram_summarises_to_zero() {
        let l = Latency::from_hist(&new_hist());
        assert_eq!(l.count, 0);
        assert_eq!(l.p99_ms, 0.0);
    }

    #[test]
    fn merge_combines_counts() {
        let mut a = new_hist();
        let mut b = new_hist();
        record(&mut a, Duration::from_millis(1));
        record(&mut b, Duration::from_millis(9));
        merge(&mut a, &b);
        assert_eq!(a.len(), 2);
    }
}
