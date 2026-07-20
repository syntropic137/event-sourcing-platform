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

Verify:

```bash
# Unauthenticated internal port (works)
grpcurl -plaintext localhost:80 list  # from inside the Docker network

# External port without credentials (fails when ESP_GATEWAY_PASSWORD is set)
grpcurl -plaintext -H 'authorization: ' localhost:8081 list  # 401

# External port with credentials
grpcurl -plaintext -H "authorization: Basic $(echo -n admin:$ESP_GATEWAY_PASSWORD | base64)" localhost:8081 list
```

## Known limitation

`auth_basic` inspects headers before the gRPC call reaches the upstream, but
per-call re-authentication over a single multiplexed HTTP/2 connection is
weaker than mTLS or a per-RPC token check at the application layer. This is
accepted as good-enough for a v1 trust boundary — see "What This Is Not" in
the ADR.
