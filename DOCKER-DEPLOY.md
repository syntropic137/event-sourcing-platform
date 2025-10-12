# Docker Compose Deployment

Quick guide for deploying Event Store with Docker Compose.

## Quick Start

```bash
# 1. Create environment file
cp env.docker.example .env
# Edit .env with your configuration

# 2. Build and run with local PostgreSQL
docker-compose --profile local up --build

# 3. Or run with external database
DATABASE_URL=postgres://user:pass@host:5432/db docker-compose up
```

## Configuration Options

### Local PostgreSQL (Development)

```bash
# .env
DATABASE_URL=postgres://eventstore:changeme@postgres:5432/eventstore
POSTGRES_PASSWORD=changeme
GRPC_PORT=50051

# Run
docker-compose --profile local up
```

### External Database (Production)

```bash
# .env
DATABASE_URL=postgres://user:pass@supabase.co:5432/postgres
GRPC_PORT=50051

# Run (no --profile needed)
docker-compose up
```

### Use Existing Dev Database

```bash
# .env
DATABASE_URL=postgres://dev:dev@host.docker.internal:15342/dev
GRPC_PORT=50051

# Run
docker-compose up
```

## Commands

```bash
# Build
docker-compose build

# Run in foreground
docker-compose up

# Run in background
docker-compose up -d

# View logs
docker-compose logs -f event-store

# Stop
docker-compose down

# Stop and remove volumes
docker-compose down -v

# Rebuild and restart
docker-compose up --build --force-recreate
```

## Health Checks

```bash
# Check if Event Store is running
docker-compose ps

# Check Event Store logs
docker-compose logs event-store

# Check PostgreSQL (if using local)
docker-compose exec postgres pg_isready -U eventstore

# Test gRPC endpoint
grpcurl -plaintext localhost:50051 list
```

## Troubleshooting

### Build fails
```bash
# Clean build
docker-compose build --no-cache
```

### Connection refused
```bash
# Check if services are healthy
docker-compose ps

# Check logs
docker-compose logs
```

### Port already in use
```bash
# Change port in .env
GRPC_PORT=50052

# Or stop conflicting service
docker ps  # Find conflicting container
docker stop <container-id>
```

## Integration with Dev Tools

The `docker-compose.dev.yml` is separate and used for local development:

```bash
# Dev infrastructure (PostgreSQL on 15342, Redis on 16342)
make dev-start

# Event Store (can connect to dev PostgreSQL)
DATABASE_URL=postgres://dev:dev@host.docker.internal:15342/dev docker-compose up
```

## Deployment

### Proxmox/AWS

Ansible will:
1. Copy `docker-compose.yml` to VM
2. Copy `.env` file with configuration
3. Run `docker-compose up -d`

See `infra-as-code/` for deployment automation.
