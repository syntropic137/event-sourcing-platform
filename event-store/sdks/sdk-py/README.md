# Python SDK (experimental)

- Generate stubs: `make -C event-store gen-py` (outputs to `sdk_py/gen`, needs `uv`).
  Pinned to grpcio-tools 1.76.0, the same generator as `event-sourcing/python`.
  The generated gRPC module imports its messages relatively, so the package
  imports as `sdk_py.gen.eventstore.v1` once installed.
- Import smoke test against an installed copy: `make -C event-store smoke-py-import` (runs in CI).
- See `examples/basic.py` for a simple append/read using the generated stubs.
