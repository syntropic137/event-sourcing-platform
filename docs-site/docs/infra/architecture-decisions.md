# Infrastructure Architecture & Design Decisions

**Last Updated:** 2025-10-01  
**Status:** Active Standard  
**Applies To:** All infrastructure deployments (Proxmox, AWS, future platforms)

## 🎯 Overview

This document defines the architectural patterns and design decisions for all infrastructure-as-code deployments. These patterns ensure consistency, maintainability, and clear separation of concerns across all environments.

## 📐 Core Architecture Principle

**Separation of Concerns: Infrastructure vs Application**

```
┌─────────────────────────────────────────────────────────────┐
│                    INFRASTRUCTURE LAYER                      │
│                      (Terraform)                             │
│  ┌────────────────────────────────────────────────────────┐ │
│  │ • VM/Instance Provisioning                             │ │
│  │ • Network Configuration (IP, Gateway, DNS)             │ │
│  │ • Storage Allocation                                   │ │
│  │ • SSH Key Injection                                    │ │
│  │ • Base OS Setup (cloud-init)                           │ │
│  └────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────┐
│                    APPLICATION LAYER                         │
│                       (Ansible)                              │
│  ┌────────────────────────────────────────────────────────┐ │
│  │ • Application Installation (Docker, Event Store)       │ │
│  │ • Service Configuration                                │ │
│  │ • Database Setup (PostgreSQL)                          │ │
│  │ • Environment Variables                                │ │
│  │ • Application-Specific Settings                        │ │
│  └────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

## 🏗️ Directory Structure

```
infra-as-code/
├── .env                          # Environment-specific secrets (gitignored)
├── .env.example                  # Template with all required variables
├── generate-config.sh            # Converts .env → YAML config
├── Makefile                      # Orchestration commands
│
├── tools/
│   └── render_config.py          # Renders YAML → Terraform tfvars.json
│
├── proxmox/                      # Proxmox-specific deployment
│   ├── provision/                # Infrastructure layer (Terraform)
│   │   ├── config/
│   │   │   └── local.yml         # Generated from .env
│   │   └── terraform/
│   │       └── envs/
│   │           └── local/
│   │               ├── main.tf
│   │               ├── variables.tf
│   │               └── terraform.tfvars.json  # Generated
│   │
│   └── configure/                # Application layer (Ansible)
│       └── ansible/
│           └── envs/
│               └── local/
│                   ├── inventory.ini
│                   ├── playbook.yml
│                   └── group_vars/
│
├── aws/                          # AWS-specific deployment (same pattern)
│   ├── provision/
│   └── configure/
│
└── shared/                       # Shared modules/roles
    ├── provision/
    │   └── terraform/
    │       └── modules/
    └── configure/
        └── ansible/
            └── roles/
```

## 🔧 Technology Stack & Responsibilities

### Terraform (Infrastructure Layer)

**Purpose:** Provision and manage infrastructure resources only.

| Responsibility | Examples | Why Terraform? |
|---------------|----------|----------------|
| **Compute Resources** | VM specs (CPU, RAM, disk) | Declarative infrastructure state |
| **Network Configuration** | IP addresses, gateways, bridges | Provider-specific networking |
| **Storage Allocation** | Disk size, storage pools | Infrastructure-level concern |
| **Base OS Setup** | Cloud-init configuration | Part of VM provisioning |
| **SSH Access** | Public key injection | Infrastructure security |
| **Provider-Specific Settings** | Proxmox node, AWS region | Platform integration |

**What Terraform Does NOT Do:**
- ❌ Install applications (Docker, Event Store)
- ❌ Configure services
- ❌ Manage application secrets
- ❌ Set up databases
- ❌ Deploy application code

### Ansible (Application Layer)

**Purpose:** Configure applications and services on provisioned infrastructure.

| Responsibility | Examples | Why Ansible? |
|---------------|----------|---------------|
| **Application Installation** | Docker, Event Store binary | Configuration management |
| **Service Configuration** | Systemd services, ports | Application-level concern |
| **Database Setup** | PostgreSQL in Docker | Application dependency |
| **Environment Variables** | DATABASE_URL, ports | Application configuration |
| **Secrets Management** | Ansible Vault for passwords | Application security |
| **Multiple Environments** | Dev, staging, prod on same VM | Application flexibility |

**What Ansible Does NOT Do:**
- ❌ Provision VMs
- ❌ Configure network interfaces
- ❌ Allocate storage
- ❌ Manage cloud provider resources

## 🌥️ Cloud-Init: The Bridge

### What is Cloud-Init?

**Cloud-init** is an industry-standard tool for initializing cloud instances on first boot. It runs **once** when a VM first starts and configures the base system.

### How Cloud-Init Works

```
1. VM boots from cloud-init-enabled template
   ↓
2. Cloud-init reads configuration from cloud provider
   ↓
3. Configures network (DHCP or static IP)
   ↓
4. Creates users and adds SSH keys
   ↓
5. Installs base packages (curl, python3, qemu-guest-agent)
   ↓
6. Runs any custom scripts
   ↓
7. Signals completion and starts services
```

### Cloud-Init Configuration (Terraform Managed)

```yaml
#cloud-config
hostname: event-store-local
manage_etc_hosts: true
fqdn: event-store-local

# User setup
user: ubuntu
ssh_authorized_keys:
  - ssh-rsa AAAA... your-key

# Network (static IP for infrastructure)
network:
  version: 2
  ethernets:
    eth0:
      addresses: [192.168.0.100/24]
      gateway4: 192.168.0.1
      nameservers:
        addresses: [192.168.0.1]

# Base packages only
packages:
  - curl
  - python3
  - python3-pip
  - qemu-guest-agent  # Required for VM management (qm guest cmd)

# Start and enable guest agent
runcmd:
  - systemctl start qemu-guest-agent
  - systemctl enable qemu-guest-agent

# NO application-specific config here!
```

### Why Cloud-Init is Critical

| Benefit | Description |
|---------|-------------|
| **Standardization** | Works across all cloud providers (AWS, Azure, GCP, Proxmox) |
| **Automation** | No manual VM configuration needed |
| **Idempotency** | Runs once, produces consistent results |
| **SSH Key Injection** | Secure passwordless access |
| **Network Configuration** | Automatic IP assignment |
| **Guest Agent** | Enables VM introspection and management |

### Cloud-Init Template Requirements

For a template to work with cloud-init:

```bash
# Required components:
1. Cloud-init package installed in OS
2. Cloud-init drive attached (ide2: cloudinit)
3. Guest agent enabled (agent: 1)
4. No ISO attached (only cloud-init drive)

# Verify template:
qm config <template-id> | grep -E "ide2|agent"

# Should show:
# agent: enabled=1
# ide2: local-lvm:cloudinit,media=cdrom
```

## 📋 Separation of Concerns Table

| Concern | Terraform | Cloud-Init | Ansible | Rationale |
|---------|-----------|------------|---------|-----------|
| **VM Creation** | ✅ | ❌ | ❌ | Infrastructure provisioning |
| **CPU/Memory/Disk** | ✅ | ❌ | ❌ | Infrastructure resources |
| **Network Config** | ✅ | ✅ | ❌ | Infrastructure + OS setup |
| **SSH Keys** | ✅ | ✅ | ❌ | Infrastructure security |
| **Base Packages** | ❌ | ✅ | ❌ | OS-level dependencies |
| **Docker Installation** | ❌ | ❌ | ✅ | Application dependency |
| **PostgreSQL Setup** | ❌ | ❌ | ✅ | Application database |
| **Event Store Config** | ❌ | ❌ | ✅ | Application configuration |
| **Service Management** | ❌ | ❌ | ✅ | Application lifecycle |
| **Environment Variables** | ❌ | ❌ | ✅ | Application settings |
| **Multiple Environments** | ❌ | ❌ | ✅ | Application flexibility |

## 🎨 Configuration Flow

### 1. Environment Variables (.env)

```bash
# Infrastructure concerns only
PROXMOX_ENDPOINT=https://192.168.0.69:8006/api2/json
PROXMOX_NODE=nuc
PROXMOX_TEMPLATE_ID=9000
PROXMOX_TEMPLATE_NAME=ubuntu-2404-cloud
VM_CPU_CORES=2
VM_MEMORY_MB=2048
DISK_SIZE=50
NETWORK_BRIDGE=vmbr0
```

### 2. Generate YAML Config

```bash
./generate-config.sh
# Converts .env → proxmox/provision/config/local.yml
```

### 3. Render Terraform Variables

```bash
make proxmox-render-local
# Converts YAML → terraform.tfvars.json
```

### 4. Apply Infrastructure

```bash
make proxmox-terraform-apply
# Creates VM with cloud-init
```

### 5. Configure Application

```bash
make proxmox-ansible-configure
# Installs and configures Event Store
```

## 🔑 Key Design Decisions

### Decision 1: Use Official Cloud Images

**Decision:** Always use official cloud images (Ubuntu Cloud Images) instead of manually created templates.

**Rationale:**
- Pre-configured with cloud-init
- Regularly updated and maintained
- Consistent across environments
- Minimal size (3.5GB base)
- Industry standard

**How to Create:**
```bash
# Download official cloud image
wget https://cloud-images.ubuntu.com/noble/current/noble-server-cloudimg-amd64.img

# Import and configure
qm create 9000 --name ubuntu-2404-cloud --memory 2048 --cores 2
qm importdisk 9000 noble-server-cloudimg-amd64.img local-lvm
qm set 9000 --scsihw virtio-scsi-pci --scsi0 local-lvm:vm-9000-disk-0
qm set 9000 --ide2 local-lvm:cloudinit
qm set 9000 --boot c --bootdisk scsi0
qm set 9000 --serial0 socket --vga serial0
qm set 9000 --agent enabled=1
qm template 9000
```

### Decision 2: Static IPs for Infrastructure Services

**Decision:** Use static IPs for infrastructure services (Event Store, databases, etc.).

**Rationale:**
- Predictable and reliable - always know where your service is
- No IP discovery needed - just SSH to the known address
- Professional setup - services can reference each other by IP
- Simpler troubleshooting - no hunting for IPs with arp-scan
- Infrastructure services should be stable, not ephemeral

**Configuration:**
```bash
# In .env
NETWORK_IP_ADDRESS=192.168.0.100/24
NETWORK_GATEWAY=192.168.0.1
```

**For ephemeral/testing VMs:** Use DHCP for temporary environments.

### Decision 3: Template ID as Variable

**Decision:** Make template ID configurable via environment variable, not hardcoded.

**Rationale:**
- Different templates for different OS versions
- Easy to update templates
- Environment-specific templates
- No code changes needed

```hcl
# Bad (hardcoded)
clone {
  vm_id = 9000
}

# Good (variable)
clone {
  vm_id = var.proxmox.template_id
}
```

### Decision 4: Automated Config Generation

**Decision:** Make config generation automatic via Makefile dependencies.

**Rationale:**
- Fewer manual steps
- Prevents stale config
- Consistent workflow
- Less error-prone

```makefile
# Automatic dependency chain
proxmox-terraform-apply: proxmox-render-local
proxmox-render-local: proxmox/provision/config/local.yml
proxmox/provision/config/local.yml: .env
	./generate-config.sh
```

### Decision 5: No Application Secrets in Terraform

**Decision:** All application secrets managed by Ansible Vault, not Terraform.

**Rationale:**
- Terraform state can be exposed
- Ansible Vault designed for secrets
- Application secrets are application concern
- Better security model

## 🚀 Deployment Workflow

### Standard Deployment Process

```bash
# 1. Configure environment
cp .env.example .env
# Edit .env with your values

# 2. Provision infrastructure (automatic config generation)
make proxmox-terraform-plan    # Review changes
make proxmox-terraform-apply   # Create VM

# 3. Wait for cloud-init (2-5 minutes)
ssh root@proxmox "qm guest cmd <vm-id> network-get-interfaces"

# 4. Configure application
make proxmox-ansible-configure

# 5. Verify
ssh ubuntu@<vm-ip>
docker ps  # Should show PostgreSQL
systemctl status event-store
```

### Multiple Environments on Same VM

```bash
# Deploy dev environment
make proxmox-ansible-configure ENV=dev

# Deploy staging environment (different ports)
make proxmox-ansible-configure ENV=staging

# Deploy prod environment (different ports)
make proxmox-ansible-configure ENV=prod

# All three running on same VM with different ports!
```

## 📊 Benefits of This Architecture

| Benefit | Description |
|---------|-------------|
| **Separation of Concerns** | Clear boundaries between infrastructure and application |
| **Flexibility** | Change app config without reprovisioning VMs |
| **Reusability** | Same patterns work across Proxmox, AWS, Azure, GCP |
| **Multiple Environments** | Run dev/staging/prod on same infrastructure |
| **Maintainability** | Clear ownership of each layer |
| **Security** | Secrets managed at appropriate layer |
| **Testability** | Can test infrastructure and application separately |
| **Documentation** | Architecture is self-documenting |

## 🎓 Learning from Mistakes

### Mistake 1: Template Without Cloud-Init

**Problem:** Used manually created template without cloud-init support.

**Impact:** 24 hours of debugging, VM never got network/SSH configured.

**Lesson:** Always verify `qm config <id> | grep cloudinit` before using template.

### Mistake 2: Mixing Infrastructure and Application

**Problem:** Put Event Store config in Terraform layer.

**Impact:** Couldn't change app config without Terraform changes, couldn't run multiple environments.

**Lesson:** Strict separation of concerns is critical.

### Mistake 3: Hardcoded Values

**Problem:** Hardcoded template ID in Terraform.

**Impact:** Had to edit code to change templates.

**Lesson:** Make everything configurable via variables.

### Mistake 4: DHCP for Infrastructure Services

**Problem:** Used DHCP for Event Store VM, requiring IP discovery after every creation.

**Impact:** Had to use arp-scan, check router, or hunt for IP - frustrating and unprofessional.

**Lesson:** Infrastructure services need static IPs. DHCP is for ephemeral/testing VMs only.

## 📚 References

- [Cloud-Init Documentation](https://cloudinit.readthedocs.io/)
- [Ubuntu Cloud Images](https://cloud-images.ubuntu.com/)
- [Terraform Best Practices](https://www.terraform-best-practices.com/)
- [Ansible Best Practices](https://docs.ansible.com/ansible/latest/user_guide/playbooks_best_practices.html)
- [Infrastructure as Code Principles](https://www.oreilly.com/library/view/infrastructure-as-code/9781491924358/)

## 🔄 Future Enhancements

- [ ] 1Password integration for SSH keys
- [ ] Automated template updates
- [ ] Multi-region deployments
- [ ] Disaster recovery automation
- [ ] Monitoring and alerting integration
- [ ] Cost optimization tracking

---

**This architecture is a living standard.** As we learn and improve, this document will be updated to reflect our evolving best practices.
