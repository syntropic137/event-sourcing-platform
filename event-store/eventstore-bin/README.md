# eventstore-bin

Tonic-based gRPC server binary and integration tests for the Event Store.

- Boots the service and wires selected backend via `BACKEND` env (memory, postgres)
- Includes integration tests using the generated client
- Useful for local dev: `make run`

See `src/main.rs` and `tests/`.
