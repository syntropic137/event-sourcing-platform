# ADR-030: Extension Model: Capabilities, Not Plugins

**Status:** Proposed
**Date:** 2026-10-08
**Deciders:** NeuralEmpowerment
**Related:** [ADR-029](ADR-029-hash-chained-event-log.md), issue #308

## Context

Requests for "plugins" keep arriving in three shapes: store integrity (hash
chaining, #308), storage of large or external content referenced by events,
and projection sinks (Postgres, SQLite, vector stores). A plugin framework
(hook trait, registry, dynamic or WASM loading, sidecars) was considered as
the common answer.

The Postgres append holds the stream row lock and the per-tenant advisory
lock through its re-checks, write and commit. Anything that runs inside it
serializes that tenant. A sidecar adds a round trip under the lock, WASM
adds a runtime and a trap failure mode, a dynamic library has no stable
ABI, and a compile-time hook trait gives no isolation (hook code has full
database access). Hashing, signing and quotas also differ in ordering and
failure semantics, so one hook shape would not fit them. There are no
third-party extension authors.

## Decision

ESP has **no plugin framework**. An extension is one of two things:

1. **A flag-gated built-in capability** of the store: code in this
   repository, deterministic and bounded, no callbacks, no network calls,
   no third-party code inside the append transaction. Off by default,
   enabled by explicit configuration, advertised through `GetServerInfo`
   capabilities (#366) when active. Example: the hash chain (ADR-029).
2. **An application-layer pattern or library** outside the store: SDK
   traits, adapters behind cargo features, or documentation. Examples:
   projection sinks (existing `ProjectionStore` / `CheckpointStore`
   traits; new sinks are implementations added when an application needs
   one), anchor sinks (`AnchorSink`, ADR-029), and storing content by
   reference (event carries reference and content hash, blob written
   first; a separate pattern document).

Adapter traits are fine where the variation is outside the store's
transaction and trust boundary (where to anchor, where to project, where
to keep blobs). They are not hooks into the append path.

## Consequences

- New in-transaction behavior needs an ADR and lands as built-in code with
  conformance tests in both backends and both flag states.
- Large content, blob storage and garbage collection stay out of the event
  store.
- "Plugin" requests are answered by classifying them as (1) or (2), not by
  adding extension points.
- Revisit only with evidence: a third-party author, or a second
  in-transaction feature whose semantics a shared mechanism would fit.
