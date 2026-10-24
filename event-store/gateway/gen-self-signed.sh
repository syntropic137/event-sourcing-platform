#!/bin/sh
# Generate a throwaway CA and a gateway server certificate signed by it, for
# local development and private networks (see README.md "TLS").
#
#   ./gen-self-signed.sh [out_dir] [dns_name_or_ip ...]
#
# Writes <out_dir>/ca.pem (give this to clients), fullchain.pem and
# privkey.pem (mount <out_dir> into the gateway at /etc/nginx/tls).
# Defaults: out_dir=./tls, names=localhost 127.0.0.1.
set -eu

out=${1:-"$(dirname "$0")/tls"}
[ $# -gt 0 ] && shift
[ $# -gt 0 ] || set -- localhost 127.0.0.1

mkdir -p "$out"
umask 077
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

san=""
for name in "$@"; do
    case "$name" in
    *[!0-9.:]*) entry="DNS:$name" ;;
    *) entry="IP:$name" ;;
    esac
    san="${san:+$san,}$entry"
done

openssl req -x509 -newkey rsa:3072 -sha256 -days 3650 -nodes \
    -subj "/CN=ESP gateway dev CA" \
    -keyout "$tmp/ca.key" -out "$out/ca.pem" 2>/dev/null

openssl req -newkey rsa:2048 -sha256 -nodes -subj "/CN=$1" \
    -keyout "$out/privkey.pem" -out "$tmp/server.csr" 2>/dev/null

cat > "$tmp/ext.cnf" <<EOF
basicConstraints=CA:FALSE
keyUsage=digitalSignature,keyEncipherment
extendedKeyUsage=serverAuth
subjectAltName=$san
EOF

openssl x509 -req -in "$tmp/server.csr" -CA "$out/ca.pem" -CAkey "$tmp/ca.key" \
    -CAcreateserial -CAserial "$tmp/ca.srl" -days 825 -sha256 \
    -extfile "$tmp/ext.cnf" -out "$out/fullchain.pem" 2>/dev/null

chmod 644 "$out/ca.pem" "$out/fullchain.pem"
chmod 600 "$out/privkey.pem"
echo "Wrote $out/{ca.pem,fullchain.pem,privkey.pem} for: $*"
echo "The CA key was discarded: re-run to issue a new pair (clients must then trust the new ca.pem)."
