#!/bin/bash
# Docker container lifecycle management

check_docker() {
    if ! docker info >/dev/null 2>&1; then
        echo "" >&2
        echo "❌ Docker is not running!" >&2
        echo "" >&2
        echo "   Please start Docker Desktop or the Docker daemon:" >&2
        echo "   • macOS/Windows: Start Docker Desktop app" >&2
        echo "   • Linux: sudo systemctl start docker" >&2
        echo "" >&2
        exit 1
    fi
}

start_or_restart_containers() {
    if [ ! -f docker-compose.dev.yml ]; then
        echo "" >&2
        echo "❌ docker-compose.dev.yml not found!" >&2
        echo "" >&2
        echo "   Please run initialization first:" >&2
        echo "   📋 make dev-init" >&2
        echo "" >&2
        exit 1
    fi
    
    if containers_exist; then
        log "Found existing containers, restarting..."
        docker start "$POSTGRES_CONTAINER" "$REDIS_CONTAINER" 2>/dev/null || {
            log "Restart failed, recreating containers..."
            force_remove_containers
            docker-compose -f docker-compose.dev.yml up -d
        }
    else
        docker-compose -f docker-compose.dev.yml up -d
    fi
}

containers_exist() {
    docker ps -a --format "{{.Names}}" | grep -q "^${POSTGRES_CONTAINER}$"
}

stop_containers() {
    docker stop "$POSTGRES_CONTAINER" "$REDIS_CONTAINER" 2>/dev/null || true
}

force_remove_containers() {
    docker rm -f "$POSTGRES_CONTAINER" "$REDIS_CONTAINER" 2>/dev/null || true
}

wait_for_services() {
    log "Waiting for services to be ready..."
    
    local max_wait=30
    local count=0
    
    while [ $count -lt $max_wait ]; do
        if service_healthy "$POSTGRES_CONTAINER" "pg_isready -U dev" && \
           service_healthy "$REDIS_CONTAINER" "redis-cli ping"; then
            log "All services ready!"
            
            # Create test database if it doesn't exist
            create_test_database
            return 0
        fi
        
        echo -n "."
        sleep 1
        count=$((count + 1))
    done
    
    error "Services failed to start within ${max_wait}s"
}

create_test_database() {
    # Create test database for integration tests
    docker exec "$POSTGRES_CONTAINER" psql -U dev -d dev -c "CREATE DATABASE test;" 2>/dev/null || true
    log "Test database ensured"
}

service_healthy() {
    local container="$1"
    local health_cmd="$2"
    docker exec "$container" $health_cmd >/dev/null 2>&1
}

show_detailed_status() {
    if docker ps --format "{{.Names}}" | grep -q "^${POSTGRES_CONTAINER}$"; then
        log "Infrastructure running"
        source .env.dev
        
        echo "📍 Postgres: localhost:$POSTGRES_PORT ($(get_health_status "$POSTGRES_CONTAINER"))"
        echo "📍 Redis: localhost:$REDIS_PORT ($(get_health_status "$REDIS_CONTAINER"))"
        
        echo "💾 Resource usage:"
        docker stats --no-stream --format "table {{.Name}}\t{{.CPUPerc}}\t{{.MemUsage}}" \
            "$POSTGRES_CONTAINER" "$REDIS_CONTAINER" 2>/dev/null || true
    else
        warn "Infrastructure not running"
        
        if docker ps -a --format "{{.Names}}" | grep -q "^${POSTGRES_CONTAINER}$"; then
            echo "💤 Containers exist but stopped. Run 'dev start' to restart."
        else
            echo "🚫 No containers found. Run 'dev init && dev start' to create."
        fi
    fi
}

get_health_status() {
    local container="$1"
    docker inspect --format='{{.State.Health.Status}}' "$container" 2>/dev/null || echo "no healthcheck"
}

show_service_logs() {
    local service="$1"
    if [ -n "$service" ]; then
        docker logs -f "${PROJECT_NAME}_${PROJECT_HASH}_${service}" 2>/dev/null || \
            error "Service '$service' not found"
    else
        docker-compose -f docker-compose.dev.yml logs -f
    fi
}

connect_to_service() {
    local service="$1"
    case $service in
        postgres|pg)
            docker exec -it "$POSTGRES_CONTAINER" psql -U dev -d dev
            ;;
        redis)
            docker exec -it "$REDIS_CONTAINER" redis-cli
            ;;
        *)
            error "Unknown service: $service. Available: postgres, redis"
            ;;
    esac
}

ensure_infrastructure_running() {
    if ! docker ps --format "{{.Names}}" | grep -q "^${POSTGRES_CONTAINER}$"; then
        log "Starting infrastructure for tests..."
        cmd_start
    fi
}
