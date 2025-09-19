# Fast Local Testing

Get blazing fast test feedback for rapid development iteration.

## Quick Start

```bash
# One-time setup
make dev-init

# Daily workflow
make test-fast    # ⚡ ~2 seconds vs 30+ seconds
```

## Performance

```mermaid
graph LR
    A[Before: 30+ seconds] --> B[After: ~2 seconds]
    B --> C[15x faster feedback]
```

## How It Works

```mermaid
flowchart TD
    A[make test-fast] --> B[Start dev infrastructure]
    B --> C[Drop/Create test database]
    C --> D[Export environment variables]
    D --> E[Run all tests sequentially]
    E --> F[✅ Results in ~2 seconds]
```

## Infrastructure

- **Postgres**: `localhost:15648` (deterministic port per project)
- **Redis**: `localhost:16648` 
- **Test DB**: Recreated fresh every run (perfect isolation)

## Commands

| Command | Purpose | Time |
|---------|---------|------|
| `make test-fast` | Run all tests with fast infrastructure | ~2s |
| `make dev-start` | Start infrastructure only | ~1s |
| `make dev-status` | Check what's running | instant |
| `make dev-clean` | Nuclear cleanup | ~2s |

## Troubleshooting

**Port conflicts?**
```bash
make dev-restart  # Force restart everything
```

**Weird state?**
```bash
make dev-kill     # Nuclear option
make dev-start    # Fresh start
```

**Check status:**
```bash
make dev-status   # See what's running
```

## Architecture

```mermaid
graph TB
    subgraph "Fast Dev Infrastructure"
        A[Docker Containers]
        A --> B[Postgres:15648]
        A --> C[Redis:16648]
    end
    
    subgraph "Test Execution"
        D[Unit Tests: 0.00s]
        E[Integration Tests: 0.26s]
        F[Service Tests: 1.27s]
    end
    
    B --> E
    B --> F
```

## Key Features

- **Idempotent**: Same results every run
- **Isolated**: Fresh database each run
- **Universal**: Works from any project directory
- **Smart**: Auto-detects conflicts and resolves them
- **Fast**: 15x performance improvement

## Fallback

No dev infrastructure? Tests automatically fall back to testcontainers (slower but works in CI).
