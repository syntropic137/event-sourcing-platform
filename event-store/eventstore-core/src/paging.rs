//! Cursor rules shared by every backend's paged reads (`ReadStream`,
//! `ReadAll`), so they agree on where the next page starts (#403).
//!
//! A position is a per-aggregate or global nonce. A page holds the events
//! from the requested position (inclusive) in the requested direction. The
//! next page starts just past the page's last event: one above it going
//! forward, one below it going backward. `is_end` is set when nothing lies
//! beyond the page in that direction, so the last page carries it.

use crate::proto::EventData;

/// Where the page after this one starts. `from` is the (normalized) start
/// of this page and `last` the position of its final event, if any.
///
/// An empty forward page keeps its start, so a caller polling a stream's
/// tail resumes there. An empty backward page, and a backward page that
/// ends at position 1, return 0: backward from 0 is empty, so 0 is a
/// terminal cursor that never repeats an event.
pub fn next_cursor(forward: bool, from: u64, last: Option<u64>) -> u64 {
    match (forward, last) {
        (true, Some(last)) => last.saturating_add(1),
        (true, None) => from,
        (false, Some(last)) => last.saturating_sub(1),
        (false, None) => 0,
    }
}

/// Takes up to `limit` events from `events` (already filtered and ordered)
/// and reports whether any remain after them.
pub fn take_page<'a>(
    mut events: impl Iterator<Item = &'a EventData>,
    limit: usize,
) -> (Vec<EventData>, bool) {
    let page: Vec<EventData> = events.by_ref().take(limit).cloned().collect();
    (page, events.next().is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forward_cursor_follows_last_event() {
        assert_eq!(next_cursor(true, 1, Some(4)), 5);
        assert_eq!(next_cursor(true, 7, None), 7);
    }

    #[test]
    fn backward_cursor_precedes_last_event() {
        // A backward page [6, 5] continues at 4, not 5.
        assert_eq!(next_cursor(false, 6, Some(5)), 4);
        assert_eq!(next_cursor(false, 1, Some(1)), 0);
        assert_eq!(next_cursor(false, 3, None), 0);
    }

    #[test]
    fn take_page_reports_remaining() {
        let events = vec![EventData::default(); 3];
        assert!(take_page(events.iter(), 2).1);
        assert!(!take_page(events.iter(), 3).1);
        assert_eq!(take_page(events.iter(), usize::MAX).0.len(), 3);
    }
}
