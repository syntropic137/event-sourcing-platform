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
| 8081 | TLS (default) + HTTP Basic Auth (when `ESP_GATEWAY_PASSWORD` set) | Any external client | Yes, this is the only port that should leave the Docker network |

## Environment variables (on the gateway container)

| Variable | Default | Description |
|----------|---------|--------------|
| `ESP_UPSTREAM` | `event-store:50051` | Host:port of the `eventstore-bin` gRPC server |
| `ESP_GATEWAY_USER` | `admin` | Basic Auth username |
| `ESP_GATEWAY_PASSWORD` | *(unset)* | Basic Auth password. **Unset = port 8081 is unauthenticated.** Generate a strong random value for any non-local deployment, e.g. `openssl rand -hex 32`. |
| `ESP_GATEWAY_TLS` | `on` | `on`: port 8081 is TLS-only; the container **exits** if the cert/key are missing or invalid. `off`: plaintext, refused unless `ESP_GATEWAY_PUBLISH_BIND` is loopback. Root `docker-compose.yml` sets `off` for local dev. |
| `ESP_GATEWAY_TLS_CERT` | `/etc/nginx/tls/fullchain.pem` | Server cert chain (PEM, leaf first) |
| `ESP_GATEWAY_TLS_KEY` | `/etc/nginx/tls/privkey.pem` | Private key (PEM) |
| `ESP_GATEWAY_TLS_RELOAD_INTERVAL` | `300` | Seconds between checks for a changed cert/key; a change triggers `nginx -t` then a graceful reload. `0` disables the watcher. |
| `ESP_GATEWAY_PUBLISH_BIND` | *(unset)* | Host address the port is published on (compose passes `ESP_GATEWAY_BIND`). With TLS off, a non-loopback or unset value aborts startup. |
| `ESP_GATEWAY_ALLOW_PLAINTEXT_EXTERNAL` | `false` | Override the check above, only when TLS is terminated in front of the gateway on a path you trust. |

## Usage

```bash
docker compose up gateway        # local dev: plaintext on 127.0.0.1 only
```

## TLS

Port 8081 carries the Basic Auth credential on every call, so anything
reachable beyond your own machine must use TLS (#301). nginx terminates TLS
itself (TLS 1.2/1.3, ECDHE + AEAD ciphers, h2 via ALPN); gRPC streaming
(`Subscribe`) is unaffected (`grpc_read_timeout 24h`). Mount a **directory**
holding `fullchain.pem` and `privkey.pem` at `/etc/nginx/tls` (a directory,
not single files, so renewed files written into it are seen). Plain HTTP to
the TLS port gets a 400 and never reaches the upstream.

### Local / private network: self-signed CA

```bash
event-store/gateway/gen-self-signed.sh                   # localhost + 127.0.0.1 -> event-store/gateway/tls/
docker compose -f docker-compose.yml -f docker-compose.tls.yml up -d
```

For another host pass the out dir and names, e.g.
`gen-self-signed.sh /etc/event-store/tls es.lan 10.0.0.5`. Give clients
`ca.pem`. The Proxmox Ansible path does this on the VM by default and fetches
the CA to `infra-as-code/proxmox/configure/ansible/envs/local/gateway-ca.pem`.
The cert is valid for 825 days; a playbook run within 30 days of expiry
reissues it. The CA key is not kept, so a reissue means a new CA that
clients must trust again (delete `/etc/event-store/tls` on the VM to force a
reissue, e.g. after changing names). Use Let's Encrypt where that matters.

### Production: Let's Encrypt

Any ACME client works; the gateway only needs the two files. With certbot on
the Docker host (the Ansible role mounts `/etc/event-store/tls`):

```bash
# DNS-01 (no inbound port 80 needed; works for private IPs). Route 53 shown;
# the instance role needs route53:ChangeResourceRecordSets on the zone.
sudo certbot certonly --dns-route53 -d es.example.com \
  --deploy-hook /usr/local/bin/esp-gateway-deploy-hook
# or HTTP-01 if port 80 on the host is reachable from the internet:
sudo certbot certonly --standalone -d es.example.com \
  --deploy-hook /usr/local/bin/esp-gateway-deploy-hook
```

`/usr/local/bin/esp-gateway-deploy-hook` (mode 0755) copies the renewed pair
(certbot's `live/` files are symlinks, which a bind mount cannot follow) and
reloads nginx:

```sh
#!/bin/sh
set -eu
install -m 0644 "$RENEWED_LINEAGE/fullchain.pem" /etc/event-store/tls/fullchain.pem.new
install -m 0600 "$RENEWED_LINEAGE/privkey.pem"   /etc/event-store/tls/privkey.pem.new
mv /etc/event-store/tls/fullchain.pem.new /etc/event-store/tls/fullchain.pem
mv /etc/event-store/tls/privkey.pem.new   /etc/event-store/tls/privkey.pem
docker exec event-store-gateway sh -c 'nginx -t -q && nginx -s reload' || true
```

Run the hook once by hand after the first `certonly` (with
`RENEWED_LINEAGE=/etc/letsencrypt/live/es.example.com`) before deploying,
since the role refuses to deploy without the files. certbot's systemd timer
renews; the hook (or, without it, the in-container watcher within
`ESP_GATEWAY_TLS_RELOAD_INTERVAL`) applies the renewal with no restart and
no dropped connections. Alternatively obtain the cert anywhere and let
Ansible ship it (`esp_gateway_tls_cert_src` / `esp_gateway_tls_key_src`,
re-run the playbook on renewal).

### Alternative: tunnel in front

A tunnel such as Cloudflare Tunnel can terminate public TLS instead. Keep the
gateway's own TLS on and point the tunnel at it, so the hop to the gateway is
encrypted too (`cloudflared` ingress `service: https://localhost:50051` with
`originRequest: {http2Origin: true, caPool: /etc/event-store/tls/ca.pem,
originServerName: <cert name>}`), or set `esp_gateway_tls: false` in Ansible,
which then publishes the plaintext port on `127.0.0.1` only for a terminator
on the same host. Either way do not publish the port beyond the host.

### Verify

```bash
make -C event-store gateway-tls-e2e   # Rust SDK via the TLS gateway: auth, streaming, rotation, fail-closed
```

### grpcurl

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

# External published port (TLS mode: replace -plaintext with
# -cacert event-store/gateway/tls/ca.pem in the commands below), without
# credentials (fails when
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
      // Private CA only; omit to use the OS trust store (Let's Encrypt).
      .tls(TlsConfig::new().ca_certificate_pem(std::fs::read("ca.pem")?))
      .basic_auth("admin", std::env::var("ESP_GATEWAY_PASSWORD")?)
      .connect()
      .await?;
  ```

- **TypeScript** (`event-store/sdks/sdk-ts`, and `event-sourcing/typescript`
  via `connection`): same semantics, header added by a grpc-js interceptor on
  every call including `subscribe`:

  ```ts
  const client = new EventStoreClientTS("https://es.example.com:50051", {
    auth: Credentials.basic("admin", process.env.ESP_GATEWAY_PASSWORD!),
    tls: { rootCerts: fs.readFileSync("ca.pem") }, // private CA only
  });
  ```

- **Python** (`event-sourcing/python` `GrpcEventStoreClient`, and
  `event-store/sdks/sdk-py` `EventStoreClientRT`): same semantics, via
  channel interceptors:

  ```python
  client = GrpcEventStoreClient(
      "https://es.example.com:50051",
      auth=BasicAuth("admin", os.environ["ESP_GATEWAY_PASSWORD"]),
      tls=TlsConfig(root_certificates=open("ca.pem", "rb").read()),  # private CA only
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
static credential and no per-tenant authorization.
Accepted for a v1 trust boundary; see "What This Is Not" in the ADR.
