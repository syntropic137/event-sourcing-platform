#!/bin/sh
# Renders nginx.conf.template, the TLS listener and the Basic Auth htpasswd
# file before starting nginx. See README.md and ADR-024.
set -eu

: "${ESP_UPSTREAM:=event-store:50051}"
: "${ESP_GATEWAY_USER:=admin}"
# TLS on the external port (8081). Default ON: the image fails closed when no
# certificate is mounted. Local dev sets ESP_GATEWAY_TLS=off explicitly.
: "${ESP_GATEWAY_TLS:=on}"
: "${ESP_GATEWAY_TLS_CERT:=/etc/nginx/tls/fullchain.pem}"
: "${ESP_GATEWAY_TLS_KEY:=/etc/nginx/tls/privkey.pem}"
# Seconds between checks for a rotated cert/key (0 = no watcher; reload with
# `nginx -s reload` from a deploy hook instead).
: "${ESP_GATEWAY_TLS_RELOAD_INTERVAL:=300}"

die() {
    echo "gateway: FATAL - $*" >&2
    exit 1
}

mkdir -p /etc/nginx/auth

if [ -n "${ESP_GATEWAY_PASSWORD:-}" ]; then
    htpasswd -Bbc /etc/nginx/auth/htpasswd "$ESP_GATEWAY_USER" "$ESP_GATEWAY_PASSWORD"
    cat > /etc/nginx/auth/auth.conf <<EOF
auth_basic "Event Store";
auth_basic_user_file /etc/nginx/auth/htpasswd;
# The credential stops here: the upstream has no auth and must never see it.
grpc_set_header Authorization "";
EOF
    echo "gateway: Basic Auth ENABLED on port 8081 (user=$ESP_GATEWAY_USER)"
else
    cat > /etc/nginx/auth/auth.conf <<'EOF'
auth_basic off;
EOF
    echo "gateway: WARNING - ESP_GATEWAY_PASSWORD not set, port 8081 is UNAUTHENTICATED"
fi

case "$ESP_GATEWAY_TLS" in
on)
    [ -r "$ESP_GATEWAY_TLS_CERT" ] && [ -s "$ESP_GATEWAY_TLS_CERT" ] ||
        die "ESP_GATEWAY_TLS=on but certificate '$ESP_GATEWAY_TLS_CERT' is missing or empty. Mount it (see README 'TLS') or set ESP_GATEWAY_TLS=off for loopback-only local dev."
    [ -r "$ESP_GATEWAY_TLS_KEY" ] && [ -s "$ESP_GATEWAY_TLS_KEY" ] ||
        die "ESP_GATEWAY_TLS=on but private key '$ESP_GATEWAY_TLS_KEY' is missing or empty."
    # Mozilla "intermediate" profile: TLS 1.2 + 1.3, AEAD/ECDHE only.
    cat > /etc/nginx/auth/listen.conf <<EOF
listen 8081 ssl;
ssl_certificate $ESP_GATEWAY_TLS_CERT;
ssl_certificate_key $ESP_GATEWAY_TLS_KEY;
ssl_protocols TLSv1.2 TLSv1.3;
ssl_ciphers ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256:ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-RSA-AES256-GCM-SHA384:ECDHE-ECDSA-CHACHA20-POLY1305:ECDHE-RSA-CHACHA20-POLY1305;
ssl_prefer_server_ciphers off;
ssl_session_cache shared:ESPTLS:10m;
ssl_session_timeout 1d;
ssl_session_tickets off;
EOF
    echo "gateway: TLS ENABLED on port 8081 (cert=$ESP_GATEWAY_TLS_CERT)"
    ;;
off)
    # Plaintext only for a loopback-published port. Compose passes the host
    # bind address as ESP_GATEWAY_PUBLISH_BIND so a LAN-exposed plaintext
    # gateway is refused instead of silently leaking credentials. Unset
    # counts as non-loopback (fail closed).
    case "${ESP_GATEWAY_PUBLISH_BIND:-}" in
    127.* | ::1 | "[::1]" | localhost) ;;
    *)
        [ "${ESP_GATEWAY_ALLOW_PLAINTEXT_EXTERNAL:-false}" = "true" ] ||
            die "ESP_GATEWAY_TLS=off with the port published on '${ESP_GATEWAY_PUBLISH_BIND:-<unknown, set ESP_GATEWAY_PUBLISH_BIND>}' would send Basic Auth credentials in plaintext. Enable TLS, bind to 127.0.0.1, or set ESP_GATEWAY_ALLOW_PLAINTEXT_EXTERNAL=true if TLS is terminated in front of the gateway."
        ;;
    esac
    echo "listen 8081;" > /etc/nginx/auth/listen.conf
    echo "gateway: WARNING - TLS disabled, port 8081 is PLAINTEXT (local dev only)"
    ;;
*)
    die "ESP_GATEWAY_TLS must be 'on' or 'off' (got '$ESP_GATEWAY_TLS')"
    ;;
esac

export ESP_UPSTREAM
envsubst '${ESP_UPSTREAM}' < /etc/nginx/nginx.conf.template > /etc/nginx/nginx.conf

# Fail at startup (not on first request) if the cert/key pair or config is bad.
nginx -t -q || die "nginx config test failed (bad certificate/key pair?)"

# Cert rotation: reload nginx when the mounted cert or key changes. A broken
# new pair fails `nginx -t` and the running config is kept.
if [ "$ESP_GATEWAY_TLS" = "on" ] && [ "$ESP_GATEWAY_TLS_RELOAD_INTERVAL" -gt 0 ]; then
    (
        fingerprint() { cat "$ESP_GATEWAY_TLS_CERT" "$ESP_GATEWAY_TLS_KEY" 2>/dev/null | sha256sum; }
        last=$(fingerprint)
        while sleep "$ESP_GATEWAY_TLS_RELOAD_INTERVAL"; do
            now=$(fingerprint)
            [ "$now" = "$last" ] && continue
            if nginx -t -q; then
                nginx -s reload && last=$now && echo "gateway: TLS certificate changed, nginx reloaded"
            else
                echo "gateway: WARNING - new TLS certificate/key failed nginx -t, keeping the old one" >&2
            fi
        done
    ) &
fi

exec nginx -g "daemon off;"
