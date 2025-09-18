---
sidebar_position: 1
---

# Event Sourcing Platform

Welcome to the documentation hub.

- Concepts that explain how the event store and SDKs work.
- Development guides for running and contributing to the repository.
- Implementation details and ADRs for architectural decisions.

## Mermaid example

```mermaid
sequenceDiagram
  participant C as Client
  participant ES as Event Store
  participant P as Projection
  C->>ES: Append(Event)
  ES-->>C: Ack + Version
  ES->>P: Project(Event)
  P-->>ES: Projection Updated
```
