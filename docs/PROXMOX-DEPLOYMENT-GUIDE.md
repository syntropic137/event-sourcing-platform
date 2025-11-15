# Proxmox Deployment Guide for Event Sourcing Platform

## Overview

This guide provides comprehensive instructions for deploying the Event Sourcing Platform on a local Intel NUC server running Proxmox VE. The deployment uses Infrastructure as Code (IaC) with Terraform for provisioning and Ansible for configuration management.

## Prerequisites

### Hardware Requirements
- Intel NUC or compatible server
- Minimum 16GB RAM (32GB recommended)
- Minimum 256GB storage (SSD recommended)
- Network connectivity

### Software Requirements
- Proxmox Virtual Environment 7.4+ or 8.x
- Ubuntu 22.04 VM template
- Terraform 1.5.7+
- Ansible 2.9+
- Python 3.8+ with PyYAML

### Network Requirements
- Static IP address for Proxmox host
- Network bridge configured (typically `vmbr0`)
- SSH access to Proxmox host

## Quick Start

### 1. Setup Secure Secrets Management

```bash
cd infra-as-code
./setup-secrets.sh
```

This creates a secure `.env` file that is excluded from version control.

### 2. Configure Proxmox API Access

Edit the `.env` file with your Proxmox configuration:

```bash
open .env
```

Required configuration:
- `PROXMOX_ENDPOINT`: Your Proxmox API URL
- `PROXMOX_USER`: Your Proxmox user account
- `PROXMOX_TOKEN_ID`: API token identifier
- `PROXMOX_TOKEN_SECRET`: API token secret
- `PROXMOX_TEMPLATE_NAME`: VM template name

### 3. Generate Configuration

```bash
./generate-config.sh
```

This generates the `local.yml` configuration file from your `.env` settings.

### 4. Test and Deploy

```bash
# Test configuration rendering
make proxmox-render-local

# Test Terraform plan (dry run)
make proxmox-terraform-plan

# Deploy infrastructure
make proxmox-terraform-apply

# Configure VM
make proxmox-ansible-configure
```

## Detailed Configuration

### Proxmox API Token Setup

#### Step 1: Access Proxmox Web Interface
1. Open web browser to `https://your-nuc-ip:8006`
2. Log in with root credentials

### Step 2: Create Dedicated User (Security Best Practice)

**⚠️ SECURITY WARNING: Never use root@pam for API tokens!**

Instead, create a dedicated user for Terraform automation:

1. **Create Dedicated User**:
   - Navigate to **Datacenter** → **Permissions** → **Users**
   - Click **Add**
   - **User**: `terraform-provisioner`
   - **Realm**: `Proxmox VE authentication server`
   - **Password**: Set a strong password
   - **Groups**: Add to appropriate groups (or create new group)

2. **Create User Group** (if needed):
   - Navigate to **Datacenter** → **Permissions** → **Groups**
   - Click **Add**
   - **Group**: `automation`
   - **Comment**: `Users for infrastructure automation`

3. **Add User to Group**:
   - Edit the `terraform-provisioner` user
   - Add to `automation` group

### Step 3: Create API Token for Dedicated User

1. **Navigate to API Tokens**:
   - Go to **Datacenter** → **Permissions** → **API Tokens**
   - Click **Add**

2. **Configure Token**:
   - **User**: Select `terraform-provisioner@pve`
   - **Token ID**: Enter descriptive name (e.g., `infra-deployment`)
   - **Privilege Separation**: Leave unchecked
   - **Expiration**: Set reasonable expiration (e.g., 90 days)

3. **Set Permissions**:
   - Click on newly created token
   - Go to **Permissions** tab
   - Add permissions:
     - **Path**: `/` (root)
     - **Role**: `PVEVMAdmin`
     - **Propagate**: ✓ Checked

4. **Copy Token Information**:
   - **Token ID**: Format `terraform-provisioner@pve!infra-deployment`
   - **Token Secret**: UUID format (shown only once)

### Environment Variables

The `.env` file supports the following configuration options:

#### Proxmox API Configuration
```bash
# SECURE: Using dedicated user instead of root
PROXMOX_ENDPOINT=https://192.168.1.100:8006/api2/json
PROXMOX_USER=terraform-provisioner@pve
PROXMOX_TOKEN_ID=terraform-provisioner@pve!infra-deployment
PROXMOX_TOKEN_SECRET=12345678-1234-1234-1234-123456789012
PROXMOX_INSECURE=true
```

⚠️ **Never use root@pam for API tokens in production!**

#### Node and Template Configuration
```bash
PROXMOX_NODE=pve
PROXMOX_TEMPLATE_NAME=ubuntu-22.04-template
PROXMOX_CIUSER=ubuntu
```

#### Network Configuration
```bash
SSH_ALLOWED_CIDRS=192.168.1.0/24
NETWORK_BRIDGE=vmbr0
```

#### Storage Configuration
```bash
STORAGE_POOL=local-lvm
DISK_SIZE=20
```

#### Compute Resources
```bash
VM_CPU_CORES=2
VM_MEMORY_MB=2048
```

#### Event Store Configuration
```bash
EVENT_STORE_GRPC_PORT=50051
EVENT_STORE_HTTP_PORT=8080
EVENT_STORE_METRICS_PORT=9090
```

#### Backend Configuration
```bash
BACKEND_TYPE=postgres
DATABASE_URL=postgres://dev:dev@localhost:15648/dev
```

#### Security Settings
```bash
ALLOW_PUBLIC_GRPC=false
ALLOW_PUBLIC_DASHBOARD=false
```

## VM Template Preparation

### Creating Ubuntu 22.04 Template

#### 1. Download Ubuntu Cloud Image
```bash
wget https://cloud-images.ubuntu.com/jammy/current/jammy-server-cloudimg-amd64.img
```

#### 2. Create VM in Proxmox
1. In Proxmox web interface: **Create VM**
2. **General**:
   - Name: `ubuntu-22.04-template`
   - VM ID: Choose unused ID (e.g., 9000)
3. **OS**: Do not use any media
4. **System**: Machine type `q35`, BIOS `OVMF (UEFI)`
5. **Disks**:
   - Storage: `local-lvm`
   - Disk size: `20G`
   - Format: `qcow2`
6. **CPU**: 2 cores, Type `host`
7. **Memory**: 2048 MB
8. **Network**: Bridge `vmbr0`, Model `VirtIO`
9. **Confirm**: Finish creation

#### 3. Configure Cloud-Init
1. Select the VM
2. Go to **Cloud-Init** tab
3. **User**: `ubuntu`
4. **Password**: Set SSH password
5. **SSH Public Key**: Paste your public key
6. **IP Configuration**: DHCP or static

#### 4. Convert to Template
1. Start the VM and complete cloud-init setup
2. Shutdown the VM
3. Right-click VM → **Convert to template**

## Security Best Practices

### 🔒 User and Access Management
- ✅ **Never use root@pam for API tokens** - create dedicated automation users
- ✅ **Create dedicated user** (e.g., `terraform-provisioner@pve`) for infrastructure automation
- ✅ **Use principle of least privilege** - grant only necessary permissions
- ✅ **Set token expiration** - avoid permanent tokens in production
- ✅ **Use user groups** - organize automation users in dedicated groups
- ✅ **Regularly rotate tokens** - implement token rotation schedule
- ✅ **Monitor token usage** - audit API access logs regularly

### 🔐 Secrets Management
- ✅ Never commit `.env` files to version control
- ✅ Use strong, unique API tokens
- ✅ Store secrets in secure locations (e.g., password managers)
- ✅ Limit token permissions to minimum required
- ✅ Use environment-specific tokens (dev/staging/prod)
- ✅ Implement token revocation process for compromised tokens

### Network Security
- ✅ Restrict SSH access to specific CIDR ranges
- ✅ Use firewall rules to limit access
- ✅ Consider VPN access for management
- ✅ Monitor access logs

### Proxmox Security
- ✅ Keep Proxmox updated
- ✅ Use strong root passwords
- ✅ Enable two-factor authentication if available
- ✅ Regular backup of VM configurations

## Troubleshooting

### Common Issues

#### API Token Not Working
```bash
# Test API access
curl -k -H "Authorization: PVEAPIToken=USER!TOKENID=UUID" \
  https://your-nuc-ip:8006/api2/json/version
```

#### Terraform Plan Fails
```bash
# Check Terraform version
terraform --version

# Initialize Terraform
cd proxmox/provision/terraform/envs/local
terraform init

# Check provider configuration
terraform providers
```

#### VM Creation Fails
1. Check storage availability
2. Verify template exists
3. Check resource limits
4. Review Proxmox logs

#### Ansible Configuration Fails
```bash
# Test SSH connectivity
ansible -i inventory.ini all -m ping

# Check syntax
ansible-playbook --syntax-check playbook.yml
```

### Debug Commands

```bash
# Check Proxmox API connectivity
curl -k https://your-nuc-ip:8006/api2/json/version

# Test Terraform configuration
cd proxmox/provision/terraform/envs/local
terraform plan -var-file=terraform.tfvars.json

# Validate Ansible inventory
ansible-inventory -i inventory.ini --list
```

## Architecture Overview

### Directory Structure
```
infra-as-code/
├── .env                    # Secret configuration (excluded from git)
├── .env.example           # Template for .env file
├── setup-secrets.sh       # Script to create .env file
├── generate-config.sh     # Script to generate local.yml
├── Makefile               # Convenience commands
├── proxmox/
│   ├── provision/
│   │   ├── config/
│   │   │   └── local.yml  # Generated configuration
│   │   └── terraform/
│   │       └── envs/
│   │           └── local/
│   │               ├── main.tf
│   │               ├── variables.tf
│   │               └── terraform.tfvars.json
│   └── configure/
│       └── ansible/
│           └── envs/
│               └── local/
│                   ├── inventory.ini
│                   ├── group_vars/
│                   └── playbook.yml
└── shared/
    ├── provision/
    │   └── terraform/
    │       └── modules/
    │           └── event-store/
    └── configure/
        └── ansible/
            └── roles/
                └── event-store/
```

### Deployment Workflow
1. **Configuration**: YAML configuration files define infrastructure
2. **Rendering**: Python script converts YAML to Terraform/Ansible formats
3. **Provisioning**: Terraform creates VM and network resources
4. **Configuration**: Ansible installs and configures applications
5. **Validation**: Automated checks ensure deployment success

## Monitoring and Maintenance

### Health Checks
```bash
# Check VM status
qm list

# Check resource usage
pvesh get /nodes/{node}/resources

# Check service status
systemctl status pveproxy
```

### Backup Strategy
1. **VM Backups**: Use Proxmox backup scheduler
2. **Configuration Backups**: Export VM configurations regularly
3. **Application Backups**: Backup event store data

### Updates and Patches
1. **Proxmox Updates**: Regularly update Proxmox host
2. **Template Updates**: Keep VM templates updated
3. **Security Patches**: Apply security patches promptly

## Support

For issues or questions:
1. Check this documentation
2. Review Proxmox official documentation
3. Check Terraform and Ansible documentation
4. Review project GitHub issues

---

**Last Updated**: 2025-09-27
**Version**: 1.0
