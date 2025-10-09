---
sidebar_label: Overview
slug: /event-store/index
---

# Event Store

A compact guide to the Rust Event Store that powers the platform.

## Essentials

- [Event model](concepts/event-model.md) — envelope fields, wire format, and versioning rules.
- [Decision records](adrs/README.md) — the "why" behind architectural choices.
- [Ubiquitous language](concepts/ubiquitous-language.md) — shared vocabulary for teams.

## Build & Operate

- [Local development](development/rust.md) — toolchain, tasks, and testing.
- [Concurrency & consistency](implementation/concurrency-and-consistency.md) — optimistic checks and sequencing.
- [SQL enforcement](implementation/sql-enforcement.md) — database invariants and triggers.
- [Operations checklist](operations/README.md) — deploy, monitor, and recover.

## SDKs & APIs

- [SDK overview](sdks/overview/sdk-overview.md) — common patterns across languages.
- [TypeScript](sdks/typescript/typescript-sdk.md) · [Python](sdks/python/python-sdk.md) · [Rust](sdks/rust/rust-sdk.md)
- [API quick reference](sdks/api-reference.md) — request/response shapes.

## Keep the Docs Fresh

See [Docs maintenance](../development/docs-maintenance.md) for regeneration commands, doc style, and publishing tips.

> **Mantra:** keep it simple, but no simpler. Short pages age better and are easier to trust.
