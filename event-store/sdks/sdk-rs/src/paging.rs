//! Paged reads that run to the end of a stream or of the tenant's log.
//!
//! `ReadStream` and `ReadAll` return one page per call. Every caller that
//! wants "everything from here" writes the same loop, and the loop has two
//! traps:
//!
//! * **`is_end` arrives on the final non-empty page.** A loop that checks
//!   `is_end` before consuming the page drops that page's events.
//! * **Termination is in the server's hands.** The loop stops on `is_end`,
//!   an empty page, or the cursor the server returns. A non-empty page with
//!   `is_end == false` and a cursor that does not move would re-request the
//!   same range forever, and a caller that collects pages would also grow
//!   without bound. So the cursor's progress is checked, not assumed.
//!
//! [`StreamPages`] and [`AllPages`] own that loop. Both read in the
//! direction the request asks for: forward the cursor must rise, backward
//! it must fall (see the paging note in `eventstore.proto`).

use std::fmt;

use eventstore_proto::gen::{EventData, ReadAllRequest, ReadStreamRequest};

use crate::EventStore;

/// The server returned a cursor that does not advance in the read's
/// direction, so reading on would request the same range again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PagingStalled {
    /// `"ReadStream"` or `"ReadAll"`.
    pub rpc: &'static str,
    /// Whether the read was ascending.
    pub forward: bool,
    /// The cursor the stalled page was requested from.
    pub from: u64,
    /// The cursor the server returned for the next page.
    pub next: u64,
}

impl fmt::Display for PagingStalled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let direction = if self.forward { "forward" } else { "backward" };
        write!(
            f,
            "{} {direction} read made no progress: requested from {}, server's next is {}",
            self.rpc, self.from, self.next
        )
    }
}

impl std::error::Error for PagingStalled {}

/// What to do after consuming a page: stop, or read on from a cursor.
fn next_cursor(
    rpc: &'static str,
    forward: bool,
    from: u64,
    page_len: usize,
    is_end: bool,
    next: u64,
) -> Result<Option<u64>, PagingStalled> {
    if is_end || page_len == 0 {
        return Ok(None);
    }
    let advanced = if forward { next > from } else { next < from };
    if !advanced {
        return Err(PagingStalled {
            rpc,
            forward,
            from,
            next,
        });
    }
    Ok(Some(next))
}

/// Where a finished read leaves its cursor: the server's cursor, unless it
/// moved against the read's direction. After the final non-empty page that
/// is one past its last event (forward: where the next append lands). An
/// empty page carries the server's normalized position: forward its start
/// (a read from 0 starts at 1), backward 0, before every event.
fn final_cursor(forward: bool, from: u64, next: u64) -> u64 {
    if forward {
        next.max(from)
    } else {
        next.min(from)
    }
}

/// Pages of one aggregate stream, from the request's position to the end.
///
/// Created by [`EventStore::stream_pages`]. Each [`Self::next_page`] is one
/// `ReadStream` call; memory is bounded by the page size.
#[derive(Debug)]
pub struct StreamPages {
    client: EventStore,
    request: ReadStreamRequest,
    done: bool,
}

impl StreamPages {
    /// The next page of events, or `None` once the stream is exhausted.
    ///
    /// The final page may hold events: they are returned, and the call after
    /// it returns `None`.
    ///
    /// # Errors
    /// The read's error, or [`PagingStalled`] if the server's cursor does not
    /// advance. After an error the pager is finished.
    pub async fn next_page(&mut self) -> anyhow::Result<Option<Vec<EventData>>> {
        if self.done {
            return Ok(None);
        }
        let page = match self.client.read_stream(self.request.clone()).await {
            Ok(page) => page,
            Err(err) => {
                self.done = true;
                return Err(err);
            }
        };
        match next_cursor(
            "ReadStream",
            self.request.forward,
            self.request.from_aggregate_nonce,
            page.events.len(),
            page.is_end,
            page.next_from_aggregate_nonce,
        ) {
            Ok(Some(next)) => self.request.from_aggregate_nonce = next,
            Ok(None) => {
                self.done = true;
                self.request.from_aggregate_nonce = final_cursor(
                    self.request.forward,
                    self.request.from_aggregate_nonce,
                    page.next_from_aggregate_nonce,
                );
            }
            Err(stalled) => {
                self.done = true;
                return Err(stalled.into());
            }
        }
        if page.events.is_empty() {
            return Ok(None);
        }
        Ok(Some(page.events))
    }

    /// The aggregate nonce the next page would be read from. After the
    /// stream is exhausted reading forward, this is the nonce the next append
    /// would take.
    pub fn cursor(&self) -> u64 {
        self.request.from_aggregate_nonce
    }

    /// Every remaining event, in the requested order.
    ///
    /// # Errors
    /// As [`Self::next_page`].
    pub async fn collect(mut self) -> anyhow::Result<Vec<EventData>> {
        let mut out = Vec::new();
        while let Some(page) = self.next_page().await? {
            out.extend(page);
        }
        Ok(out)
    }
}

/// Pages of the tenant's global log, from the request's position to the end.
///
/// Created by [`EventStore::all_pages`]. Each [`Self::next_page`] is one
/// `ReadAll` call; memory is bounded by the page size, so a caller that
/// filters or folds each page before asking for the next holds only what it
/// keeps.
#[derive(Debug)]
pub struct AllPages {
    client: EventStore,
    request: ReadAllRequest,
    done: bool,
}

impl AllPages {
    /// The next page of events, or `None` once the log is exhausted.
    ///
    /// # Errors
    /// The read's error, or [`PagingStalled`] if the server's cursor does not
    /// advance. After an error the pager is finished.
    pub async fn next_page(&mut self) -> anyhow::Result<Option<Vec<EventData>>> {
        if self.done {
            return Ok(None);
        }
        let page = match self.client.read_all(self.request.clone()).await {
            Ok(page) => page,
            Err(err) => {
                self.done = true;
                return Err(err);
            }
        };
        match next_cursor(
            "ReadAll",
            self.request.forward,
            self.request.from_global_nonce,
            page.events.len(),
            page.is_end,
            page.next_from_global_nonce,
        ) {
            Ok(Some(next)) => self.request.from_global_nonce = next,
            Ok(None) => {
                self.done = true;
                self.request.from_global_nonce = final_cursor(
                    self.request.forward,
                    self.request.from_global_nonce,
                    page.next_from_global_nonce,
                );
            }
            Err(stalled) => {
                self.done = true;
                return Err(stalled.into());
            }
        }
        if page.events.is_empty() {
            return Ok(None);
        }
        Ok(Some(page.events))
    }

    /// The position the next page would be read from. After the log is
    /// exhausted reading forward, this is where a later tail read resumes.
    pub fn cursor(&self) -> u64 {
        self.request.from_global_nonce
    }

    /// Every remaining event, in the requested order.
    ///
    /// # Errors
    /// As [`Self::next_page`].
    pub async fn collect(mut self) -> anyhow::Result<Vec<EventData>> {
        let mut out = Vec::new();
        while let Some(page) = self.next_page().await? {
            out.extend(page);
        }
        Ok(out)
    }
}

impl EventStore {
    /// Page through one aggregate stream from `req.from_aggregate_nonce` to
    /// its end, `req.max_count` events per call (`0` asks the server for no
    /// limit). The pager shares this client's channel.
    pub fn stream_pages(&self, req: ReadStreamRequest) -> StreamPages {
        StreamPages {
            client: self.clone(),
            request: req,
            done: false,
        }
    }

    /// Page through the tenant's global log from `req.from_global_nonce` to
    /// its end. The pager shares this client's channel.
    pub fn all_pages(&self, req: ReadAllRequest) -> AllPages {
        AllPages {
            client: self.clone(),
            request: req,
            done: false,
        }
    }

    /// Read one aggregate stream to its end. Shorthand for
    /// `stream_pages(req).collect()`.
    ///
    /// # Errors
    /// As [`StreamPages::next_page`].
    pub async fn read_stream_to_end(
        &self,
        req: ReadStreamRequest,
    ) -> anyhow::Result<Vec<EventData>> {
        self.stream_pages(req).collect().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn final_non_empty_page_is_consumed_then_stops() {
        assert_eq!(next_cursor("ReadAll", true, 1, 3, true, 4), Ok(None));
    }

    #[test]
    fn empty_page_stops_even_without_is_end() {
        assert_eq!(next_cursor("ReadAll", true, 7, 0, false, 7), Ok(None));
    }

    #[test]
    fn forward_cursor_must_rise() {
        assert_eq!(next_cursor("ReadStream", true, 1, 2, false, 3), Ok(Some(3)));
        let stalled = next_cursor("ReadStream", true, 3, 2, false, 3).unwrap_err();
        assert_eq!(
            stalled,
            PagingStalled {
                rpc: "ReadStream",
                forward: true,
                from: 3,
                next: 3
            }
        );
        assert!(next_cursor("ReadStream", true, 3, 2, false, 2).is_err());
    }

    #[test]
    fn backward_cursor_must_fall() {
        assert_eq!(next_cursor("ReadAll", false, 10, 2, false, 8), Ok(Some(8)));
        assert!(next_cursor("ReadAll", false, 10, 2, false, 10).is_err());
        assert!(next_cursor("ReadAll", false, 10, 2, false, 11).is_err());
    }

    #[test]
    fn a_finished_read_keeps_the_server_cursor_only_when_it_moved() {
        assert_eq!(final_cursor(true, 1, 4), 4);
        assert_eq!(final_cursor(true, 4, 4), 4);
        // Empty forward page from 0: the server's normalized start.
        assert_eq!(final_cursor(true, 0, 1), 1);
        // A forward cursor that moved backward is ignored.
        assert_eq!(final_cursor(true, 4, 2), 4);
        assert_eq!(final_cursor(false, 10, 8), 8);
        // Empty backward page: 0, before every event.
        assert_eq!(final_cursor(false, 10, 0), 0);
        assert_eq!(final_cursor(false, 10, 11), 10);
    }

    #[test]
    fn stalled_names_the_rpc_and_both_cursors() {
        let msg = PagingStalled {
            rpc: "ReadAll",
            forward: true,
            from: 5,
            next: 5,
        }
        .to_string();
        assert!(msg.contains("ReadAll forward"), "{msg}");
        assert!(msg.contains("from 5") && msg.contains("next is 5"), "{msg}");
    }
}
