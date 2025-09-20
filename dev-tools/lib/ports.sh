#!/bin/bash
# Port allocation and conflict resolution

calc_project_ports() {
    local base_offset=$((0x${PROJECT_HASH:0:3} % 1000))
    POSTGRES_PORT=$((POSTGRES_BASE + base_offset))
    REDIS_PORT=$((REDIS_BASE + base_offset))
}

resolve_port_conflicts() {
    check_and_resolve_port "$POSTGRES_PORT" "Postgres"
    check_and_resolve_port "$REDIS_PORT" "Redis"
}

check_and_resolve_port() {
    local port="$1"
    local service="$2"
    
    local pids=$(lsof -ti :$port 2>/dev/null || true)
    if [ -n "$pids" ]; then
        # Check if it's our own container first
        if is_our_container_on_port "$port"; then
            log "Port $port is used by our own $service container - will restart it"
            return 0
        fi
        
        warn "Port $port ($service) is in use by external processes: $pids"
        
        if [ "${DEV_AUTO_KILL_CONFLICTS:-false}" = "true" ]; then
            kill_port_processes "$port" "$pids"
        else
            prompt_kill_processes "$port" "$pids"
        fi
    fi
}

prompt_kill_processes() {
    local port="$1"
    local pids="$2"
    
    read -p "Kill conflicting processes? [y/N] " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        kill_port_processes "$port" "$pids"
    else
        error "Cannot start - port conflict on $port"
    fi
}

kill_port_processes() {
    local port="$1"
    local pids="$2"
    
    echo $pids | xargs kill -9 2>/dev/null || true
    log "Killed processes on port $port"
}

is_our_container_on_port() {
    local port="$1"
    
    # Check if our containers are running and using this port
    if [ "$port" = "$POSTGRES_PORT" ]; then
        docker ps --format "{{.Names}}" 2>/dev/null | grep -q "^${POSTGRES_CONTAINER}$" || return 1
        docker port "$POSTGRES_CONTAINER" 2>/dev/null | grep -q ":$port" || return 1
        return 0
    elif [ "$port" = "$REDIS_PORT" ]; then
        docker ps --format "{{.Names}}" 2>/dev/null | grep -q "^${REDIS_CONTAINER}$" || return 1
        docker port "$REDIS_CONTAINER" 2>/dev/null | grep -q ":$port" || return 1
        return 0
    fi
    
    return 1
}
