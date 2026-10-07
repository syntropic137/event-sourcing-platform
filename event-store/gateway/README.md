# Event Store gRPC Gateway

nginx reverse proxy that puts an authentication boundary in front of the
`event-store` gRPC service. The service itself has no authn/authz — this
gateway is the trust boundary. See
[ADR-024](../../docs/adrs/ADR-024-nginx-gateway-two-port-grpc-auth.md) for
the design rationale, and
`syntropic137/syntropic137`'s `ADR-059-nginx-gateway-two-port-auth-model.md`
for the prior-art pattern this follows.

## Ports

| Port | Auth | Consumers | Publish to host? |
|------|------|-----------|-------------------|
| 80   | None | Same-network services, health checks | No — Docker network isolation is the boundary |
| 8081 | HTTP Basic Auth (when `ESP_GATEWAY_PASSWORD` set) | Any external client | Yes, this is the only port that should leave the Docker network |

## Environment variables (on the gateway container)

| Variable | Default | Description |
|----------|---------|--------------|
| `ESP_UPSTREAM` | `event-store:50051` | Host:port of the `eventstore-bin` gRPC server |
| `ESP_GATEWAY_USER` | `admin` | Basic Auth username |
| `ESP_GATEWAY_PASSWORD` | *(unset)* | Basic Auth password. **Unset = port 8081 is unauthenticated.** Generate a strong random value for any non-local deployment, e.g. `openssl rand -hex 32`. |

## Usage

```bash
docker compose up gateway
```

Verify (port 80 is Docker-internal only — not published to the host, so it's
only reachable as `gateway:80` from another container on the same network;
port 8081 is what `docker-compose.yml` publishes as `${GRPC_PORT:-50051}` on
the host):

`eventstore-bin` does not enable gRPC server reflection, so `grpcurl ...
list` without a proto always fails with "server does not support the
reflection API" — that's a reflection gap, not an auth result, and it fails
identically whether or not you're authenticated. Pass `-proto` (verified
working commands, run from the repo root):

```bash
# Unauthenticated internal port (works) — run from another container on the
# same Docker network, e.g.:
docker run --rm --network event-sourcing-platform_eventstore-network \
  -v "$(pwd)/event-store/eventstore-proto/proto:/proto" \
  fullstorydev/grpcurl \
  -plaintext -import-path /proto -proto eventstore/v1/eventstore.proto \
  gateway:80 list

# External published port, without credentials (fails when
# ESP_GATEWAY_PASSWORD is set - expect gRPC status UNAUTHENTICATED (16);
# the gateway maps nginx's 401 to a proper gRPC status. This one doesn't
# need -proto since it never gets past the auth check)
grpcurl -plaintext -H 'authorization: ' localhost:50051 list

# External published port, with credentials (succeeds)
# Use `tr -d '\n'` after base64, not `-w0` — `-w0` is GNU-only and isn't
# available on macOS/BSD base64; a wrapped/newline-containing token breaks
# the authorization header.
TOKEN=$(echo -n "admin:$ESP_GATEWAY_PASSWORD" | base64 | tr -d '\n')
grpcurl -plaintext \
  -import-path event-store/eventstore-proto/proto \
  -proto eventstore/v1/eventstore.proto \
  -H "authorization: Basic $TOKEN" \
  localhost:50051 list
```

## Client SDK support

- **Rust** (`event-store/sdks/sdk-rs`): supported. The client sends
  `authorization: Basic ...` on every call and refuses to send credentials
  over plaintext to a non-loopback host unless explicitly allowed:

  ```rust
  let client = ClientConfig::new("https://es.example.com:50051")
      .basic_auth("admin", std::env::var("ESP_GATEWAY_PASSWORD")?)
      .connect()
      .await?;
  // Plaintext to a remote host (no TLS in front of the gateway yet, #301):
  // add `.allow_insecure_credentials(true)` and accept the risk.
  ```

- **TypeScript** (`event-store/sdks/sdk-ts`, and `event-sourcing/typescript`
  via `connection`): same semantics, header added by a grpc-js interceptor on
  every call including `subscribe`:

  ```ts
  const client = new EventStoreClientTS("https://es.example.com:50051", {
    auth: Credentials.basic("admin", process.env.ESP_GATEWAY_PASSWORD!),
    // allowInsecureCredentials: true, // plaintext to a remote host (#301)
  });
  ```

- **Python** (`event-sourcing/python` `GrpcEventStoreClient`, and
  `event-store/sdks/sdk-py` `EventStoreClientRT`): same semantics, via
  channel interceptors:

  ```python
  client = GrpcEventStoreClient(
      "https://es.example.com:50051",
      auth=BasicAuth("admin", os.environ["ESP_GATEWAY_PASSWORD"]),
      # allow_insecure_credentials=True,  # plaintext to a remote host (#301)
  )
  ```

All clients also accept a bearer token or a per-call token provider,
redact credentials from their string forms, and surface a gateway rejection
as a typed `UNAUTHENTICATED` error (TS `UnauthenticatedError` /
`EventStoreAuthenticationError`, Python `EventStoreAuthenticationError` /
`UnauthenticatedError`).

## Known limitation

nginx evaluates `auth_basic` per HTTP/2 request, i.e. per gRPC call, before
it reaches the upstream. The limitation is the credential model: one shared
static credential, no per-tenant authorization, and no TLS in front of the
gateway yet (#301), so the credential is only safe on a trusted network.
Accepted for a v1 trust boundary; see "What This Is Not" in the ADR.
