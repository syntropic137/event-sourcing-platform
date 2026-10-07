FROM nginx:1.27-alpine

RUN apk add --no-cache apache2-utils

COPY nginx.conf.template /etc/nginx/nginx.conf.template
COPY docker-entrypoint.sh /docker-entrypoint.sh
RUN chmod +x /docker-entrypoint.sh

EXPOSE 80 8081

ENTRYPOINT ["/docker-entrypoint.sh"]
