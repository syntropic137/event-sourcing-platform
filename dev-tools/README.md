# EMP Dev Tools

**Smart Container Management for Development**

## Principles

- 🔧 **Smart conflict resolution** - Automatically handle port conflicts
- 🔄 **Robust lifecycle management** - Containers that restart reliably  
- 👨‍💻 **Developer-friendly UX** - Simple commands, clear feedback
- 🏷️ **Labeled containers** - Easy identification and cleanup
- ❤️ **Health checks** - Wait for services to be actually ready
- 🛡️ **Graceful degradation** - Works even when things go wrong

## Quick Start

```bash
# One-time setup
./dev-tools/dev init

# Daily workflow
./dev-tools/dev start   # Start infrastructure
./dev-tools/dev test    # Run fast tests
./dev-tools/dev stop    # Stop when done

# Optional: Add to PATH for convenience
export PATH="$PWD/dev-tools:$PATH"
dev start  # Now works from anywhere in project
```

## Commands

### Core Commands
- `dev init` - Initialize project dev environment
- `dev start` - Start infrastructure (smart restart)
- `dev stop` - Stop infrastructure (preserve containers)
- `dev status` - Show detailed status

### Maintenance Commands  
- `dev restart` - Force restart infrastructure
- `dev kill` - Force kill everything
- `dev clean` - Clean all data and containers
- `dev cleanup-all` - Clean ALL dev-tools containers globally

### Development Commands
- `dev test [args]` - Run tests with infrastructure
- `dev logs [service]` - Show logs (postgres|redis)
- `dev shell [service]` - Connect to service shell

## How It Works

### Project Detection
Each project gets a unique hash based on its directory path:
```bash
Project: event-store
Hash: a1b2c3d4
Postgres: localhost:15847
Redis: localhost:16794
```

### Port Allocation
Deterministic ports prevent conflicts between projects:
- Postgres: `15000 + hash_offset`
- Redis: `16000 + hash_offset`
- Offset range: 0-999 (supports 1000 concurrent projects)

### Container Naming
Containers are uniquely named and labeled:
```
event-store_a1b2c3d4_postgres
event-store_a1b2c3d4_redis
```

Labels enable easy cleanup:
```bash
dev-tools.project=event-store
dev-tools.hash=a1b2c3d4  
dev-tools.auto-cleanup=true
```

## Configuration

### Environment Variables
```bash
# Auto-cleanup after 2 hours of inactivity (default)
export DEV_CLEANUP_AFTER=7200

# Auto-kill conflicting processes (default: false)
export DEV_AUTO_KILL_CONFLICTS=true

# Always restart containers on start (default: false)
export DEV_FORCE_RESTART=true
```

### Generated Files
- `.env.dev` - Environment variables for your project
- `docker-compose.dev.yml` - Generated compose file

## Test Integration

### Rust Projects
```rust
fn get_test_db_url() -> String {
    std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| {
            // Fallback to testcontainers for CI
            setup_testcontainer()
        })
}
```

### Performance Comparison
- **Development**: ~50ms test startup (vs 30+ seconds)
- **CI/CD**: Same as before (isolated testcontainers)

## Troubleshooting

### Port Conflicts
```bash
dev start
# ⚠️  Port 15847 (Postgres) is in use by processes: 1234
# Kill conflicting processes? [y/N] y
```

### Container Issues
```bash
dev restart     # When containers are in weird state
dev kill        # Nuclear option - force kill everything
dev status      # Check what's running
```

### Logs & Debugging
```bash
dev logs postgres   # View Postgres logs
dev shell postgres  # Connect to database
dev shell redis     # Connect to Redis CLI
```

## Installation

### Copy to Project
```bash
# Copy dev-tools to your project
cp -r /path/to/dev-tools/ my-project/
cd my-project
chmod +x dev-tools/dev
```

### Global Installation (Optional)
```bash
# Add to your shell profile
export PATH="/path/to/dev-tools:$PATH"

# Now works from any project directory
cd any-project
dev init
dev start
```

## Architecture

```
dev-tools/
├── dev                    # Main CLI entry point
├── lib/
│   ├── core.sh           # Core functions & commands
│   ├── docker.sh         # Container lifecycle management
│   ├── ports.sh          # Port allocation & conflict resolution
│   └── cleanup.sh        # Cleanup & maintenance
├── templates/
│   └── docker-compose.yml # Docker Compose template
├── config/
│   └── defaults.conf     # Default configuration
└── README.md             # This file
```

## Benefits

✅ **Fast Development** - Sub-second test startup  
✅ **No Conflicts** - Deterministic port allocation  
✅ **Easy Maintenance** - Auto-cleanup and health checks  
✅ **CI/CD Compatible** - Falls back to testcontainers  
✅ **Drag & Drop** - Works in any project immediately  

## License

MIT - Feel free to copy, modify, and use in your projects!
