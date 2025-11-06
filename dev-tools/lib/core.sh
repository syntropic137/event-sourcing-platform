#!/bin/bash
# Core project detection and configuration

init_project_context() {
    PROJECT_HASH=$(pwd | sha256sum | cut -c1-8)
    PROJECT_NAME=$(basename "$(pwd)")
    
    # Load defaults
    source "$DEV_TOOLS_DIR/config/defaults.conf"
    
    # Calculate ports
    calc_project_ports
    
    # Set container names
    POSTGRES_CONTAINER="${PROJECT_NAME}_${PROJECT_HASH}_postgres"
    REDIS_CONTAINER="${PROJECT_NAME}_${PROJECT_HASH}_redis"
    
    export PROJECT_HASH PROJECT_NAME POSTGRES_PORT REDIS_PORT
    export POSTGRES_CONTAINER REDIS_CONTAINER
}

log() { echo "🔧 $*"; }
error() { echo "❌ $*" >&2; exit 1; }
warn() { echo "⚠️  $*"; }

cmd_init() {
    check_docker
    log "Initializing dev environment for $PROJECT_NAME (hash: $PROJECT_HASH)"
    log "Postgres: localhost:$POSTGRES_PORT"
    log "Redis: localhost:$REDIS_PORT"
    
    generate_env_file
    generate_compose_file
    log "Ready! Run 'dev start' to begin"
}

cmd_start() {
    check_docker
    load_env_file
    
    resolve_port_conflicts
    cleanup_old_containers
    
    log "Starting infrastructure..."
    start_or_restart_containers
    wait_for_services
    
    show_connection_info
}

cmd_restart() {
    check_docker
    log "Restarting infrastructure..."
    force_remove_containers
    cmd_start
}

cmd_stop() {
    log "Stopping infrastructure..."
    stop_containers
    log "Infrastructure stopped (containers preserved)"
}

cmd_kill() {
    log "Force killing infrastructure..."
    force_cleanup_project
}

cmd_clean() {
    log "Cleaning environment..."
    cleanup_project_completely
    log "Environment cleaned"
}

cmd_status() {
    show_detailed_status
}

cmd_logs() {
    local service="$1"
    show_service_logs "$service"
}

cmd_shell() {
    local service="${1:-postgres}"
    connect_to_service "$service"
}

cmd_test() {
    load_env_file
    ensure_infrastructure_running
    
    # Clean the test database for idempotent tests
    log "Recreating test database for fresh run..."
    docker exec "$POSTGRES_CONTAINER" psql -U dev -d dev -c "DROP DATABASE IF EXISTS test;" 2>/dev/null || true
    docker exec "$POSTGRES_CONTAINER" psql -U dev -d dev -c "CREATE DATABASE test;" 2>/dev/null || true
    
    log "Running tests with fast infrastructure..."
    
    # Export environment variables for tests
    export DATABASE_URL
    export TEST_DATABASE_URL
    export REDIS_URL
    
    # Run tests sequentially to avoid database conflicts
    cargo test "$@" -- --test-threads=1
}

cmd_cleanup_all() {
    log "Cleaning up ALL dev-tools containers across projects..."
    global_cleanup
    log "Global cleanup complete"
}

cmd_help() {
    cat << 'EOF'
EMP Dev Tools - Smart Container Management

PRINCIPLES:
• Smart conflict resolution  • Robust lifecycle management
• Developer-friendly UX      • Labeled containers
• Health checks             • Graceful degradation

COMMANDS:
  init          Initialize project dev environment
  start         Start infrastructure (smart restart)
  restart       Force restart infrastructure  
  stop          Stop infrastructure (preserve containers)
  kill          Force kill everything
  clean         Clean all data and containers
  status        Show detailed status
  logs [svc]    Show logs (postgres|redis)
  shell [svc]   Connect to service shell
  test [args]   Run tests with infrastructure
  cleanup-all   Clean ALL dev-tools containers globally

ENVIRONMENT:
  DEV_CLEANUP_AFTER=7200    Auto-cleanup after seconds (default: 2h)
  DEV_FORCE_RESTART=false   Always restart containers on start
EOF
}

generate_env_file() {
    cat > .env.dev << EOF
DATABASE_URL=postgres://dev:dev@localhost:$POSTGRES_PORT/dev
TEST_DATABASE_URL=postgres://dev:dev@localhost:$POSTGRES_PORT/test
REDIS_URL=redis://localhost:$REDIS_PORT
PROJECT_HASH=$PROJECT_HASH
POSTGRES_PORT=$POSTGRES_PORT
REDIS_PORT=$REDIS_PORT
EOF
}

generate_compose_file() {
    envsubst < "$DEV_TOOLS_DIR/templates/docker-compose.yml" > docker-compose.dev.yml
}

load_env_file() {
    if [ ! -f .env.dev ]; then
        echo "" >&2
        echo "❌ Development environment not initialized!" >&2
        echo "" >&2
        echo "   Please run the following command first:" >&2
        echo "   📋 make dev-init" >&2
        echo "" >&2
        echo "   This will:" >&2
        echo "   • Generate .env.dev configuration" >&2
        echo "   • Create docker-compose.dev.yml" >&2
        echo "   • Calculate unique ports for your project" >&2
        echo "" >&2
        exit 1
    fi
    source .env.dev
}

show_connection_info() {
    log "Infrastructure ready!"
    log "Postgres: localhost:$POSTGRES_PORT"
    log "Redis: localhost:$REDIS_PORT"
    log "Auto-cleanup after: $((CLEANUP_AFTER / 3600))h of inactivity"
}
