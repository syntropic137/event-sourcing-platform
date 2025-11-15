# eventstore-backend-postgres

PostgreSQL-backed implementation of the Event Store backend.

- Durable storage using PostgreSQL
- SQL migrations under `migrations/`
- Integration tests leverage Testcontainers

Status: implemented. See `src/` and `tests/` for details.

- [eventstore-backend-postgres](#eventstore-backend-postgres)
  - [Rationale \& Guarantees](#rationale--guarantees)
  - [Future Enhancements](#future-enhancements)


## Rationale & Guarantees

- **Global sequencing**: `global_nonce BIGSERIAL PRIMARY KEY` provides a strictly increasing, gap-free sequence for the entire log (per-node). This is the canonical global order.
- **Per-stream sequencing**: each stream increments `stream_version` by 1 starting at 1. Enforced by:
  - `UNIQUE(stream_id, stream_version)` for OCC + duplicates prevention
  - `CHECK (stream_version > 0)` to disallow zero/negative
  - `BEFORE INSERT` trigger to require `stream_version = max(prev) + 1` (or `1` if first)
- **Immutability**: events are append-only. Enforced by `BEFORE UPDATE/DELETE` triggers that always raise.
- **Idempotency**: `event_id UUID UNIQUE` prevents duplicate writes of the same logical event.

See: `migrations/*_init.sql` for the full DDL.

## Future Enhancements

- **Privileges lockdown**: run the application with a role granted only `INSERT, SELECT` on `events` and no `UPDATE/DELETE`. Keep a separate migration role for schema changes.
- **Indexes**: consider additional composite indexes (e.g., `(stream_type, stream_id, stream_version)`) if access patterns demand.
- **Stream type semantics**: revisit once the proto and plan stabilize to ensure indexes/constraints align with query semantics.
