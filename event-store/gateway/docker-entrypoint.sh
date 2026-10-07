#!/bin/sh
# Renders nginx.conf.template and generates the Basic Auth htpasswd file
# from ESP_GATEWAY_PASSWORD before starting nginx. See README.md.
set -eu

: "${ESP_UPSTREAM:=event-store:50051}"
: "${ESP_GATEWAY_USER:=admin}"

mkdir -p /etc/nginx/auth

if [ -n "${ESP_GATEWAY_PASSWORD:-}" ]; then
    htpasswd -Bbc /etc/nginx/auth/htpasswd "$ESP_GATEWAY_USER" "$ESP_GATEWAY_PASSWORD"
    cat > /etc/nginx/auth/auth.conf <<EOF
auth_basic "Event Store";
auth_basic_user_file /etc/nginx/auth/htpasswd;
EOF
    echo "gateway: Basic Auth ENABLED on port 8081 (user=$ESP_GATEWAY_USER)"
else
    cat > /etc/nginx/auth/auth.conf <<'EOF'
auth_basic off;
EOF
    echo "gateway: WARNING - ESP_GATEWAY_PASSWORD not set, port 8081 is UNAUTHENTICATED"
fi

export ESP_UPSTREAM
envsubst '${ESP_UPSTREAM}' < /etc/nginx/nginx.conf.template > /etc/nginx/nginx.conf

exec nginx -g "daemon off;"
