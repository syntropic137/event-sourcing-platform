# Ansible Configuration for Event Store

This directory contains Ansible playbooks and configuration for deploying the Event Sourcing Platform.

## Quick Start (Automated)

**All configuration is auto-generated from your `.env` file!**

### 1. Configure Your Environment

```bash
cd infra-as-code

# Copy example and edit
cp .env.example .env
# Edit .env - set POSTGRES_PASSWORD and EVENT_STORE_BINARY_URL
```

### 2. Generate Configuration

```bash
# Generate all Terraform AND Ansible configs from .env
./generate-config.sh

# This creates:
# - proxmox/provision/terraform/envs/local/terraform.tfvars.json
# - proxmox/configure/ansible/envs/local/inventory.ini
# - proxmox/configure/ansible/envs/local/group_vars/all.yml
```

### 3. Run the Playbook

```bash
cd proxmox/configure/ansible/envs/local

# Test connection
ansible -i inventory.ini event_store -m ping

# Run the full playbook
ansible-playbook -i inventory.ini playbook.yml

# Run specific tags only
ansible-playbook -i inventory.ini playbook.yml --tags base      # Just Docker install
ansible-playbook -i inventory.ini playbook.yml --tags deploy    # Just deploy compose
```

## What Gets Deployed

The playbook will:
1. Install Docker & Docker Compose
2. Install qemu-guest-agent
3. Copy `docker-compose.yml` to `/opt/eventstore/`
4. Generate `.env` file with your configuration
5. Run `docker-compose up -d`
6. Verify Event Store is running on port 50051

## Manual Configuration (Not Recommended)

If you need to manually create configs:

```bash
cd envs/local
cp inventory.ini.example inventory.ini
cp group_vars/all.yml.example group_vars/all.yml
# Edit both files manually
```

## What Gets Deployed

1. **Base System** (tag: `base`)
   - qemu-guest-agent
   - Docker & Docker Compose
   - Required system packages

2. **PostgreSQL** (tag: `postgres`)
   - PostgreSQL 16 in Docker
   - Data persistence in `/var/lib/eventstore/postgres`
   - Port 5432

3. **Event Store** (tag: `eventstore`)
   - Event Store binary
   - Systemd service
   - gRPC server on port 50051

## Configuration Variables

See `group_vars/all.yml.example` for all available variables.

Key variables:
- `postgres_password` - PostgreSQL password (use ansible-vault!)
- `binary_url` - Event Store binary download URL
- `eventstore_grpc_port` - gRPC server port (default: 50051)
- `postgres_port` - PostgreSQL port (default: 5432)

## Multiple Environments

To run multiple Event Store environments on the same VM:

```yaml
# group_vars/dev.yml
eventstore_grpc_port: 50051
postgres_port: 5432
postgres_container_name: eventstore-postgres-dev

# group_vars/staging.yml
eventstore_grpc_port: 50052
postgres_port: 5433
postgres_container_name: eventstore-postgres-staging
```

## Troubleshooting

### Check service status
```bash
ssh ubuntu@192.168.0.100
sudo systemctl status eventstore
sudo docker ps
```

### View logs
```bash
sudo journalctl -u eventstore -f
sudo docker logs eventstore-postgres
```

### Verify connectivity
```bash
# Test PostgreSQL
docker exec eventstore-postgres pg_isready -U eventstore

# Test Event Store gRPC
grpcurl -plaintext localhost:50051 list
```

## Security

**Important:** Never commit `inventory.ini` or `group_vars/all.yml` to git!

Use Ansible Vault for sensitive data:
```bash
ansible-vault encrypt group_vars/all.yml
ansible-playbook -i inventory.ini playbook.yml --ask-vault-pass
```
