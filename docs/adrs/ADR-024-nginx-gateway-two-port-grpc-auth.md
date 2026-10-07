# ADR-024: nginx Gateway Two-Port Authentication Model for the Event Store

**Status:** Accepted
**Date:** 2026-07-20
**Context:** The `eventstore-bin` gRPC service has no authentication, authorization, or TLS of its own.

---

## Context

`eventstore-bin` (the Rust gRPC event store server) implements no authn/authz
and no TLS termination. Any client that can reach port 50051 can append and
read events for any tenant. This was a deliberate scope decision — the
service focuses on storage/ordering/concurrency correctness — but it means
the service has **no stated trust boundary** as shipped. Prior to this ADR,
that boundary existed only as an unstated assumption ("someone will put
something in front of it"), which is not a defensible security posture and
does not show up anywhere a deployer or reviewer would find it.

This mirrors a pattern already solved elsewhere in the Syntropic137
ecosystem: `syntropic137/syntropic137`'s
`ADR-059-nginx-gateway-two-port-auth-model.md` documents the same problem
(an internal API with no app-layer auth, fronted by nginx) and the same
two-port solution. This ADR adapts that pattern for a gRPC (HTTP/2) upstream
instead of a REST/SPA upstream.

## Decision

An nginx gateway (`event-store/gateway/`) sits in front of `eventstore-bin`
and is the **only** component permitted to be reachable from outside the
Docker network. It exposes two server blocks with different auth policies,
proxying via `grpc_pass` (nginx's native HTTP/2/gRPC proxy support):

| Port | Authentication | Consumers | Published to host? |
|------|----------------|-----------|----------------------|
| 80   | None | Same-Docker-network services, health checks | No |
| 8081 | HTTP Basic Auth (when `ESP_GATEWAY_PASSWORD` is set) | Any external client | Yes — this is the only externally reachable port |

### Port 80 — unauthenticated, network-internal

Security boundary is Docker network isolation, not application auth. This
port must never be published to a host interface or tunnel.

### Port 8081 — Basic Auth required

Enforced whenever `ESP_GATEWAY_PASSWORD` is non-empty. Generated via
`docker-entrypoint.sh` into an nginx `auth_basic_user_file` at container
start (`htpasswd -Bbc`, bcrypt). An unset password disables auth and logs a
loud warning — acceptable for local development, **not** for any deployment
reachable from outside the operator's own machine.

### `ESP_GATEWAY_PASSWORD` lifecycle

- Not generated automatically (unlike ADR-059's setup-wizard flow — this
  platform has no equivalent installer yet). Operators must set it
  explicitly, e.g. `openssl rand -hex 32`, before exposing port 8081.
- Rotation is manual: update the env var and restart the `gateway` service.
- Never baked into the image or committed; lives in the deployer's
  environment/secret store.

## Threat Model

| Threat | Mitigation |
|--------|-----------|
| Unauthenticated external access to the event store | Only port 8081 is published; it requires Basic Auth when configured |
| Operator forgets to set a password | Loud startup warning; documented in `event-store/gateway/README.md` as a precondition for external exposure |
| `eventstore-bin`'s own port (50051) exposed directly, bypassing the gateway | Root `docker-compose.yml` no longer publishes the `event-store` service's port to the host; only `gateway` does |
| Credential sniffing in transit | Basic Auth over plaintext HTTP/2 is only acceptable behind TLS termination (e.g. a tunnel/load balancer that terminates TLS in front of the gateway) — **not yet wired up in this repo**, tracked as follow-up |

## What This Is Not

This is not a per-tenant or per-caller authorization model — one shared
credential grants full access to every tenant behind the gateway. It is not
mutual TLS, and it does not survive a compromised gateway host. It is the
minimum viable trust boundary: a stated, testable, "off by default until you
configure it" gate, replacing an unstated assumption. A real multi-tenant
authz model (per-tenant tokens validated at the gRPC layer, e.g. via a
`tonic` interceptor) is a legitimate future iteration and should get its own
ADR when undertaken.

## Consequences

**Good:**
- The event store's trust boundary is now documented and testable, not
  implicit.
- Local/internal Docker traffic and health checks are unaffected (port 80,
  unauthenticated, network-isolated).
- The only way to reach `eventstore-bin` from outside the Docker network is
  through the gateway — enforced by `docker-compose.yml`, not just
  convention.

**Bad / accepted tradeoffs:**
- Basic Auth is a single shared credential, not per-tenant. Fine for a v1
  gate on a "foundational tool" others build on; not sufficient for a
  multi-tenant SaaS deployment without further work.
- No automatic password generation/rotation tooling exists yet (unlike
  ADR-059's `npx` setup wizard) — this is a manual operator responsibility
  until such tooling exists here.
- `auth_basic` is checked per HTTP/2 request (per gRPC call), but against a
  single static credential with no per-tenant authorization; see the
  "Known limitation" note in `event-store/gateway/README.md`.
- TLS termination in front of the gateway (tracked in #301) is not yet wired into this repo's
  `docker-compose.yml` / `infra-as-code/` — Basic Auth credentials are only
  safe in transit once that's added (e.g. via a tunnel or load balancer that
  terminates TLS).
  Until then the shipped defaults limit exposure: root compose binds the
  gateway to 127.0.0.1 unless `ESP_GATEWAY_BIND` is set, and the AWS prod
  config sets `allow_public_grpc: false` (gRPC ingress only from the admin
  CIDRs).

## References

- `event-store/gateway/` — nginx config, entrypoint, Dockerfile, README
- `docker-compose.yml` — `gateway` service definition
- `syntropic137/syntropic137` `docs/adrs/ADR-059-nginx-gateway-two-port-auth-model.md` — prior art this pattern is adapted from
