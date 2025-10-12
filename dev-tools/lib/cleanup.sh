#!/bin/bash
# Container cleanup and maintenance

cleanup_old_containers() {
    local cutoff=$(date -d "${CLEANUP_AFTER} seconds ago" +%s 2>/dev/null || \
                   date -v-${CLEANUP_AFTER}S +%s)
    
    docker ps -a --filter "label=dev-tools.auto-cleanup=true" \
        --format "{{.Names}}\t{{.CreatedAt}}" | \
    while IFS=$'\t' read -r name created_at; do
        if container_older_than "$created_at" "$cutoff"; then
            warn "Removing old container: $name (created $created_at)"
            docker rm -f "$name" 2>/dev/null || true
        fi
    done
}

container_older_than() {
    local created_at="$1"
    local cutoff="$2"
    
    local container_time=$(date -d "$created_at" +%s 2>/dev/null || \
                          date -j -f "%Y-%m-%d %H:%M:%S" "$created_at" +%s)
    [ $container_time -lt $cutoff ]
}

force_cleanup_project() {
    log "Force cleaning project containers..."
    
    # Stop and remove project containers
    docker ps -a --format "{{.Names}}" | grep "^${PROJECT_NAME}_" | \
        xargs -r docker rm -f 2>/dev/null || true
    
    # Remove project volumes
    docker volume ls --format "{{.Name}}" | grep "^${PROJECT_NAME}_" | \
        xargs -r docker volume rm 2>/dev/null || true
    
    # Kill processes on our ports
    lsof -ti :$POSTGRES_PORT | xargs -r kill -9 2>/dev/null || true
    lsof -ti :$REDIS_PORT | xargs -r kill -9 2>/dev/null || true
    
    log "Force cleanup complete"
}

cleanup_project_completely() {
    docker-compose -f docker-compose.dev.yml down -v 2>/dev/null || true
    force_cleanup_project
}

global_cleanup() {
    docker ps -a --filter "label=dev-tools.auto-cleanup=true" \
        --format "{{.Names}}" | xargs -r docker rm -f
    docker volume ls --filter "label=dev-tools.auto-cleanup=true" \
        --format "{{.Name}}" | xargs -r docker volume rm
}
