#!/bin/bash

# Configuration Generation Script
# Reads from .env file and generates local.yml configuration

set -e

echo "⚙️  Generating Proxmox configuration from .env file..."
echo "===================================================="

# Check if .env file exists
if [ ! -f ".env" ]; then
    echo "❌ .env file not found."
    echo ""
    echo "📝 Please create .env file:"
    echo "   cp .env.example .env"
    echo "   # Edit .env with your settings"
    exit 1
fi

echo "✓ Found .env file"

# Load environment variables from .env
set -a
source .env
set +a

# Check required Terraform variables
terraform_required_vars=(
    "PROXMOX_ENDPOINT"
    "PROXMOX_USER" 
    "PROXMOX_TOKEN_ID"
    "PROXMOX_TOKEN_SECRET"
    "PROXMOX_TEMPLATE_NAME"
)

# Check required Ansible variables
ansible_required_vars=(
    "POSTGRES_PASSWORD"
)

missing_vars=()
for var in "${terraform_required_vars[@]}" "${ansible_required_vars[@]}"; do
    if [ -z "${!var}" ]; then
        missing_vars+=("$var")
    fi
done

if [ ${#missing_vars[@]} -gt 0 ]; then
    echo "❌ Missing required environment variables:"
    printf '   - %s\n' "${missing_vars[@]}"
    echo ""
    echo "📝 Please edit your .env file and add these values:"
    echo "   open .env"
    exit 1
fi

echo "✓ All required variables present"

# Generate local.yml configuration
echo "📝 Generating proxmox/provision/config/local.yml..."
cat > proxmox/provision/config/local.yml << EOF
# Proxmox local environment configuration for the event store deployment
# Generated from .env file - DO NOT EDIT DIRECTLY

metadata:
  environment: ${ENVIRONMENT:-local}
  owner: ${OWNER:-developer}
  extra_tags:
    purpose: testing
    generated_by: env-config

compute:
  vm_name: "event-store-${ENVIRONMENT:-local}"
  cores: ${VM_CPU_CORES:-2}
  sockets: 1
  memory_mb: ${VM_MEMORY_MB:-2048}
  disk_gb: ${DISK_SIZE:-20}

proxmox:
  endpoint: "${PROXMOX_ENDPOINT}"
  user: "${PROXMOX_USER}"
  token_id: "${PROXMOX_TOKEN_ID}"
  token_secret: "${PROXMOX_TOKEN_SECRET}"
  insecure: ${PROXMOX_INSECURE:-true}
  node: "${PROXMOX_NODE:-pve}"
  template_name: "${PROXMOX_TEMPLATE_NAME}"
  template_id: ${PROXMOX_TEMPLATE_ID:-9000}
  storage_pool: "${STORAGE_POOL:-local-lvm}"
  network_bridge: "${NETWORK_BRIDGE:-vmbr0}"
  vlan_tag: 0
  pool: ""
  ciuser: "${PROXMOX_CIUSER:-ubuntu}"
  ssh_public_key_path: "${PROXMOX_SSH_PUBLIC_KEY_PATH:-~/.ssh/id_rsa.pub}"
  clone_timeout: 600

network:
  ip_address: ${NETWORK_IP_ADDRESS:-192.168.0.100/24}
  gateway: ${NETWORK_GATEWAY:-192.168.0.1}
  dns_servers:
    - ${NETWORK_DNS:-192.168.0.1}
  ssh_allowed_cidrs: ${SSH_ALLOWED_CIDRS:-192.168.0.0/24}
  bridge: "${NETWORK_BRIDGE:-vmbr0}"

storage:
  pool: "${STORAGE_POOL:-local-lvm}"

security:
  allow_public_grpc: ${ALLOW_PUBLIC_GRPC:-false}
  allow_public_dashboard: ${ALLOW_PUBLIC_DASHBOARD:-false}

ansible:
  inventory_host_group: event_store
  ssh_user: ${PROXMOX_CIUSER:-ubuntu}
  ssh_private_key_path: ${PROXMOX_SSH_PRIVATE_KEY_PATH:-~/.ssh/nuc-proxmox}
  inventory_hostname: event-store-${ENVIRONMENT:-local}
  host: ${NETWORK_IP_ADDRESS%/*}  # Extract IP from CIDR
  
  # PostgreSQL configuration
  postgres:
    container_name: eventstore-postgres-${ENVIRONMENT:-local}
    db: ${POSTGRES_DB:-eventstore}
    user: ${POSTGRES_USER:-eventstore}
    password: ${POSTGRES_PASSWORD:-changeme}
    port: ${POSTGRES_PORT:-5432}
    data_dir: /var/lib/eventstore/postgres
  
  # Event Store configuration
  eventstore:
    grpc_port: ${EVENT_STORE_GRPC_PORT:-50051}
    backend: ${EVENT_STORE_BACKEND:-postgres}
    binary_url: ${EVENT_STORE_BINARY_URL:-}
    rust_log: ${RUST_LOG:-info}
  
  # Service configuration
  service:
    user: eventstore
    group: eventstore
    name: eventstore
    description: Event Store Service (${ENVIRONMENT:-local})
    install_dir: /opt/event-store
  
  docker_compose_dir: /opt/eventstore/docker
EOF

echo "✅ Configuration generated successfully!"
echo ""
echo "📋 Generated configuration:"
echo "   File: proxmox/provision/config/local.yml"
echo "   Environment: ${ENVIRONMENT:-local}"
echo "   VM Name: event-store-${ENVIRONMENT:-local}"
echo "   Template: ${PROXMOX_TEMPLATE_NAME}"
echo "   CPU Cores: ${VM_CPU_CORES:-2}"
echo "   Memory: ${VM_MEMORY_MB:-2048}MB"
echo "   Disk: ${DISK_SIZE:-20}GB"
echo ""
echo "🔍 You can now test the configuration:"
echo "   make proxmox-render-local"
echo "   make proxmox-terraform-plan"
