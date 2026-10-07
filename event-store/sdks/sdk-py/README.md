# Python SDK (experimental)

- Generate stubs: `make -C event-store gen-py` (outputs to `sdk_py/gen`, needs `uv`).
  Pinned to grpcio-tools 1.76.0, the same generator as `event-sourcing/python`.
  The generated gRPC module imports its messages relatively, so the package
  imports as `sdk_py.gen.eventstore.v1` once installed.
- Import smoke test against an installed copy: `make -C event-store smoke-py-import` (runs in CI).
- See `examples/basic.py` for a simple append/read using the generated stubs.
- Credentials and TLS (ADR-024 gateway), header sent on every call including `subscribe`:

  ```python
  from sdk_py.auth import BasicAuth
  from sdk_py.client_rt import EventStoreClientRT

  client = EventStoreClientRT(
      "https://es.example.com:50051",  # host:port = plaintext
      auth=BasicAuth("admin", os.environ["ESP_GATEWAY_PASSWORD"]),
      # BearerToken(...) / TokenProviderAuth(SharedToken(...)); tls=TlsConfig(...)
      # allow_insecure_credentials=True,  # plaintext to a non-loopback host
  )
  ```

  Credentials over plaintext to a non-loopback host raise `ClientConfigError`
  unless allowed; rejected credentials raise `UnauthenticatedError` (a
  `grpc.RpcError`). Unit tests: `make -C event-store test-sdk-py`.
