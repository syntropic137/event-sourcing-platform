# Foundations: the books this platform is built on

The platform was built on three books. **Martin Dilger's *Understanding
Eventsourcing* is the primary one.** Event sourcing as the platform does it,
Event Modeling, the four slice patterns that `vsa` validates, and the
Processor To-Do List behind `ProcessManager` all come from it. Vaughn Vernon's
*Implementing Domain-Driven Design* supplies hexagonal ports and adapters, the
event-sourced aggregate and global-order reads. Eric Evans' *Domain-Driven
Design* supplies the definitions underneath both: ubiquitous language and
bounded contexts.

| Book | Role here |
|---|---|
| Martin Dilger, *Understanding Eventsourcing* ([Leanpub](https://leanpub.com/eventsourcing)) | **Primary.** Events, streams, projections, Event Modeling, slices, processors |
| Vaughn Vernon, *Implementing Domain-Driven Design* (Addison-Wesley, 2013) | Hexagonal architecture, event-sourced aggregates, integrating contexts |
| Eric Evans, *Domain-Driven Design* (Addison-Wesley, 2004) | Ubiquitous language, bounded contexts, aggregates |

**Page numbers are printed pages of the paper editions.** Dilger's book is
also sold as an evolving e-book whose chapter numbers can differ from print:
the Processor To-Do List is chapter 35 in print (pp. 495-509), which existing
docs here cite as chapter 37. Both are kept; use the page number to find it in
print. The printed pages were checked against scans of the paper editions.

## Concept-to-page table

| Concept | Book | Printed pages | Where it lives here |
|---|---|---|---|
| Event: an immutable past-tense fact, the source of truth | Dilger | 16-17, 80-81 | [Platform philosophy](./PLATFORM-PHILOSOPHY.md) |
| Stream: the ordered events of one business entity; stream id = entity id | Dilger; Vernon | Dilger 17-18, 120-121, 124-125; Vernon 539, 562 | `event-store/`; [ADR-020](./adrs/ADR-020-bounded-context-aggregate-convention.md) |
| Stream design: short, business-shaped lifecycles; close the books before snapshotting | Dilger | 121, 127-130, 491-492 | Not yet documented here |
| Optimistic concurrency per stream, version = index of last event | Dilger; Vernon | Dilger 85-89; Vernon 385, 554-558, 566 | [ADR-021](./adrs/ADR-021-expected-version-and-set-based-validation.md) |
| Per-stream read to rebuild one aggregate; all-streams read in global order for projections | Vernon | 563 | [ADR-012](./adrs/ADR-012-readall-global-stream.md) |
| Notification log: consumer tracks its own position; dedupe in the same transaction | Vernon | 311-316, 329-331 | [ADR-014](./adrs/ADR-014-projection-checkpoint-architecture.md) |
| Projection / state view: disposable, rebuildable read model | Dilger; Vernon | Dilger 19-20, 55-56, 469-476; Vernon 570-573 | `CheckpointedProjection`, [consumer patterns](./CONSUMER-PATTERNS.md) |
| Live model: fold one stream per request | Dilger | 330-338, 477-480 | Aggregate load in the SDKs |
| State change slice: trigger, command, event | Dilger | 51, 53-54 | `vsa` slice type `command` |
| State view slice: events, read model, no command | Dilger | 55-56 | `vsa` slice type `view` |
| Automation slice: view, processor, command, event | Dilger | 57-58, 387 | `vsa` slice type `automation` |
| Translation slice: external event, translator, own command and event | Dilger | 58-60, 361-368 | `vsa` slice type `translation` |
| Every state change, view and process step is its own slice | Dilger | 161, 166-176 | [Vertical slices](../docs-site/docs/vsa/concepts/vertical-slices.md); [ADR-008](./adrs/ADR-008-vertical-slices-hexagonal-adapters.md) |
| No direct dependencies between slices; events are their API | Dilger | 172-173, 287-288 | `vsa validate` isolation rules |
| Given/When/Then: When is always a command; views and automations use Given/Then | Dilger | 64, 209-210 | [ADR-015](./adrs/ADR-015-es-test-kit-architecture.md) |
| Processor To-Do List (Dilger's chapter title: "Processor-TODO-List"): the list is a view; the processor issues a command; the event clears the entry | Dilger | 495-509 | `ProcessManager`, [ADR-025](./adrs/ADR-025-process-manager-pattern.md); Rust: `ProjectionRunner` + `LiveProcessor`, batch mode `run_until_idle` (#440) |
| To-do list: the view of pending work, computed from the state of the system; how an entry gets on, what checks it off | Dilger | 496-498 | A `CheckpointedProjection` that only writes to-do records |
| Processor: takes each open entry and issues one command; the resulting event flows back and clears the entry (the back channel) | Dilger | 499-505 | `LiveProcessor` (Rust), `ProcessManager` |
| Projection runner: keeps one view current from its checkpoint and never issues a command | Dilger | 469-476, 502-505 | `ProjectionRunner` (Rust), [ADR-014](./adrs/ADR-014-projection-checkpoint-architecture.md) |
| Background runner: hosts a context's projection runners and processors off the request path; plumbing that owns no context's list or command | Dilger | 506-509 (ownership) | Proposed: #436 (`RunnerGroup`). Beyond the book, see below |
| Never update projections from a side-effecting handler; never replay a command-issuing handler | Dilger | 414-416, 461-462 | [Consumer patterns](./CONSUMER-PATTERNS.md) |
| Reservation pattern: a rule across streams via an aggregate keyed by the scarce value | Dilger | 510-516 | [ADR-021](./adrs/ADR-021-expected-version-and-set-based-validation.md) (set-based validation) |
| Internal vs integration events: internal events belong to one context; integration events are the versioned contract | Dilger; Vernon | Dilger 34-35, 91-95, 234-240, 436; Vernon 452-460 | [Integration events](../docs-site/docs/vsa/concepts/integration-events.md) |
| Event versioning and upcasters | Dilger | 95-100, 452-456 | [ADR-007](./adrs/ADR-007-event-versioning-upcasters.md) |
| Aggregate: a transactional consistency boundary; keep it small | Dilger; Vernon; Evans | Dilger 108, 139-146; Vernon 353-358; Evans 125-129 | [ADR-004](./adrs/ADR-004-command-handlers-in-aggregates.md) |
| Event-sourced aggregate: load, fold, decide, append at expected version | Vernon | 539-549 | SDK repositories |
| Hexagonal ports and adapters | Vernon | 123-130 | [ADR-005](./adrs/ADR-005-hexagonal-architecture-event-sourcing.md) |
| Bounded context: a linguistic boundary, not a deployment unit | Evans; Vernon; Dilger | Evans 335-336; Vernon 62; Dilger 137 | `vsa` contexts; [ADR-020](./adrs/ADR-020-bounded-context-aggregate-convention.md) |
| Ubiquitous language: one language per context, used in code | Evans | 24-27, 335-336 | [Ubiquitous language](../docs-site/docs/event-store/concepts/ubiquitous-language.md) |

## Where the platform goes beyond the books

- **Deletion and privacy.** Dilger does not cover them, and keeps payloads in
  events (p. 22). Consumers that need erasure must design it themselves, for
  example by keeping content out of the log.
- **Shared runners.** Dilger gives each processor its own processing group
  (pp. 270, 418) and no shared runner; in his cross-module example the
  reacting module owns the to-do list, the processor and the command
  (pp. 506-509). A background runner here is hosting plumbing only, so it
  keeps that ownership: each job lives in the context whose command it
  issues.
- **Hash-chained log.** Not in any of the three books;
  [ADR-029](./adrs/ADR-029-hash-chained-event-log.md) is the platform's own.
