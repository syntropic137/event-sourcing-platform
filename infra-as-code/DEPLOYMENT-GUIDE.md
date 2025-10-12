# Event Store Deployment Guide

Complete guide for deploying Event Store to Proxmox with Terraform and Ansible.

## 🚀 Quick Start (Single Command)

```bash
# 1. One-time setup
cd infra-as-code
cp .env.example .env
# Edit .env with your Proxmox credentials and settings

# 2. Deploy everything
make proxmox-deploy-full

# That's it! 🎉
```

## 📋 What Gets Deployed

The `make proxmox-deploy-full` command does:

1. ✅ **Validates** your .env configuration
2. ✅ **Generates** Terraform and Ansible configs
3. ✅ **Builds** Docker image for x86_64
4. ✅ **Provisions** VM on Proxmox with Terraform
5. ✅ **Transfers** Docker image to VM
6. ✅ **Deploys** Event Store + PostgreSQL with Ansible
7. ✅ **Validates** deployment is working

**Total time:** ~10 minutes

## 🔧 Prerequisites

### Required Software:
- **Terraform** >= 1.0
- **Ansible** >= 2.9
- **Docker** with buildx support
- **Python 3** with PyYAML
- **Make**

### Required Access:
- Proxmox API token with VM creation permissions
- SSH access to Proxmox node
- Ubuntu 24.04 cloud-init template in Proxmox

## 📝 Configuration (.env file)

### Minimum Required Variables:

```bash
# Proxmox Connection
PROXMOX_ENDPOINT=https://192.168.0.69:8006/api2/json
PROXMOX_USER=ansible
PROXMOX_TOKEN_ID=ansible@pve!ansible-2025-09
PROXMOX_TOKEN_SECRET=your-token-secret
PROXMOX_NODE=nuc
PROXMOX_TEMPLATE_NAME=ubuntu-2404-cloud

# SSH Keys
PROXMOX_SSH_PUBLIC_KEY_PATH=~/.ssh/nuc-proxmox.pub
PROXMOX_SSH_PRIVATE_KEY_PATH=~/.ssh/nuc-proxmox

# Network
NETWORK_IP_ADDRESS=192.168.0.100/24
NETWORK_GATEWAY=192.168.0.1

# Database
POSTGRES_PASSWORD=your-secure-password-here
```

### Optional Variables:

```bash
# VM Resources
VM_CPU_CORES=2
VM_MEMORY_MB=2048
DISK_SIZE=50

# Event Store
EVENT_STORE_GRPC_PORT=50051
RUST_LOG=info

# Environment
ENVIRONMENT=local
OWNER=developer
```

## 🎯 Step-by-Step Deployment

If you want to run steps individually:

### Step 1: Generate Configuration

```bash
cd infra-as-code
./generate-config.sh
make proxmox-render-local
```

This creates:
- `proxmox/provision/config/local.yml`
- `proxmox/provision/terraform/envs/local/terraform.tfvars.json`
- `proxmox/configure/ansible/envs/local/inventory.ini`
- `proxmox/configure/ansible/envs/local/group_vars/all.yml`

### Step 2: Build Docker Image

```bash
make docker-build-and-save
```

This builds Event Store for x86_64 and saves to `/tmp/event-store-amd64.tar.gz`.

### Step 3: Provision VM with Terraform

```bash
make proxmox-terraform-apply
```

This creates:
- Ubuntu 24.04 VM
- Static IP: 192.168.0.100
- SSH access configured
- Cloud-init setup

### Step 4: Deploy Event Store with Ansible

```bash
make proxmox-ansible-configure
```

This installs:
- Docker & Docker Compose
- Event Store container
- PostgreSQL container
- Health checks

## 🧪 Testing Deployment

### Test with TypeScript Examples:

```bash
# Test Example 002
EVENT_STORE_ADDR=192.168.0.100:50051 npm run start --prefix examples/002-simple-aggregate-ts

# Test Example 003
EVENT_STORE_ADDR=192.168.0.100:50051 npm run start --prefix examples/003-multiple-aggregates-ts
```

### Check Service Status:

```bash
# SSH to VM
ssh -i ~/.ssh/nuc-proxmox ubuntu@192.168.0.100

# Check containers
sudo docker ps

# Check logs
sudo docker logs event-store
sudo docker logs eventstore-postgres

# Check Event Store
curl -v http://localhost:50051
```

## 🗑️ Destroying Deployment

```bash
# Destroy VM and all resources
make proxmox-destroy
```

This will:
- Destroy the VM in Proxmox
- Remove Terraform state
- Clean up resources

**Note:** This does NOT delete:
- Docker images on your local machine
- Configuration files
- .env file

## 🔄 Redeployment

To redeploy from scratch:

```bash
# 1. Destroy existing deployment
make proxmox-destroy

# 2. Deploy again
make proxmox-deploy-full
```

## 📊 Available Make Targets

### Configuration:
- `make proxmox-render-local` - Generate configs from .env

### Docker:
- `make docker-build-amd64` - Build image for x86_64
- `make docker-save` - Save image to tar.gz
- `make docker-build-and-save` - Build and save

### Terraform:
- `make proxmox-terraform-plan` - Preview changes
- `make proxmox-terraform-apply` - Create VM

### Ansible:
- `make proxmox-ansible-configure` - Deploy Event Store

### Full Workflow:
- `make proxmox-deploy-full` - Complete deployment
- `make proxmox-destroy` - Destroy everything

## 🐛 Troubleshooting

### Issue: "Missing required environment variables"

**Solution:** Edit your `.env` file and add the missing variables.

```bash
cd infra-as-code
./generate-config.sh  # Shows which variables are missing
```

### Issue: "Docker image not found"

**Solution:** Build the Docker image first.

```bash
cd infra-as-code
make docker-build-and-save
```

### Issue: "Terraform apply fails"

**Possible causes:**
1. Proxmox API token expired or invalid
2. Template doesn't exist
3. Network configuration incorrect

**Solution:**
```bash
# Check Proxmox connection
curl -k https://192.168.0.69:8006/api2/json/version

# Verify template exists
# Login to Proxmox web UI and check

# Check Terraform plan
make proxmox-terraform-plan
```

### Issue: "Ansible playbook fails"

**Possible causes:**
1. VM not accessible via SSH
2. Docker image not transferred
3. Network issues

**Solution:**
```bash
# Test SSH connection
ssh -i ~/.ssh/nuc-proxmox ubuntu@192.168.0.100

# Check if image exists
ls -lh /tmp/event-store-amd64.tar.gz

# Run Ansible with verbose output
cd proxmox/configure/ansible/envs/local
ansible-playbook -i inventory.ini playbook.yml -vvv
```

### Issue: "Event Store container keeps restarting"

**Possible causes:**
1. Database connection issues
2. Wrong architecture (ARM vs x86_64)
3. Missing environment variables

**Solution:**
```bash
# Check logs
ssh -i ~/.ssh/nuc-proxmox ubuntu@192.168.0.100
sudo docker logs event-store

# Check PostgreSQL
sudo docker logs eventstore-postgres

# Verify image architecture
sudo docker image inspect event-sourcing-platform-event-store:latest | grep Architecture
```

## 📚 Architecture

### Infrastructure Layer (Terraform):
- Provisions VM on Proxmox
- Configures networking
- Sets up cloud-init
- Manages VM lifecycle

### Application Layer (Ansible):
- Installs Docker
- Transfers Docker images
- Deploys containers
- Manages configuration

### Application Stack (Docker Compose):
- Event Store (gRPC server)
- PostgreSQL (database)
- Networking
- Health checks

## 🔐 Security Considerations

### Production Deployments:

1. **Use Ansible Vault** for sensitive data:
```bash
ansible-vault encrypt group_vars/all.yml
```

2. **Change default passwords:**
- PostgreSQL password
- Proxmox API token

3. **Restrict network access:**
- Update firewall rules
- Use VPN for management
- Limit SSH access

4. **Use TLS:**
- Enable TLS for gRPC
- Use HTTPS for Proxmox
- Encrypt database connections

## 📖 Additional Resources

- [Terraform Proxmox Provider](https://registry.terraform.io/providers/Telmate/proxmox/latest/docs)
- [Ansible Documentation](https://docs.ansible.com/)
- [Docker Compose Documentation](https://docs.docker.com/compose/)
- [Event Sourcing Platform Examples](../examples/)

## 🆘 Getting Help

If you encounter issues:

1. Check this troubleshooting guide
2. Review logs (Terraform, Ansible, Docker)
3. Verify prerequisites are installed
4. Check network connectivity
5. Ensure .env file is complete

## 🎉 Success!

Once deployed, you should see:

```
🎉 Deployment complete!

📊 Connection info:
   Event Store: 192.168.0.100:50051

🧪 Test with:
   EVENT_STORE_ADDR=192.168.0.100:50051 npm run start --prefix ../examples/002-simple-aggregate-ts
```

Your Event Sourcing Platform is now running! 🚀
