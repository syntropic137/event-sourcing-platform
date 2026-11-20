# nginx 1.30.7 (stable), pinned by the multi-arch index digest: fixes
# CVE-2026-123007 (HTTP/2) and nine further CVEs from the 2026-11-18 release.
# 1.27.x and 1.28.x receive no fix. This gateway needs HTTP/2 for gRPC.
FROM nginx:1.30.7-alpine@sha256:caf4e1e032a20f9c3c5e28fc7f25ffd768ad8677b4975c54193075997b3a97e3

RUN apk add --no-cache apache2-utils

COPY nginx.conf.template /etc/nginx/nginx.conf.template
COPY docker-entrypoint.sh /docker-entrypoint.sh
RUN chmod +x /docker-entrypoint.sh

EXPOSE 80 8081

ENTRYPOINT ["/docker-entrypoint.sh"]
