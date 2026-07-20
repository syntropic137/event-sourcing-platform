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

```bash
# Unauthenticated internal port (works) — run from another container on the
# same Docker network, e.g.:
docker run --rm --network event-sourcing-platform_eventstore-network \
  fullstorydev/grpcurl -plaintext gateway:80 list

# External published port, without credentials (fails when
# ESP_GATEWAY_PASSWORD is set — expect a 401/Unauthorized)
grpcurl -plaintext -H 'authorization: ' localhost:50051 list

# External published port, with credentials
# Use `tr -d '\n'` after base64, not `-w0` — `-w0` is GNU-only and isn't
# available on macOS/BSD base64; a wrapped/newline-containing token breaks
# the authorization header.
TOKEN=$(echo -n "admin:$ESP_GATEWAY_PASSWORD" | base64 | tr -d '\n')
grpcurl -plaintext -H "authorization: Basic $TOKEN" localhost:50051 list
```

## Known limitation

`auth_basic` inspects headers before the gRPC call reaches the upstream, but
per-call re-authentication over a single multiplexed HTTP/2 connection is
weaker than mTLS or a per-RPC token check at the application layer. This is
accepted as good-enough for a v1 trust boundary — see "What This Is Not" in
the ADR.
