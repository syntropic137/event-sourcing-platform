# Deploying to Proxmox with Terraform

This guide covers deploying the Event Store to Proxmox VE using Terraform, including all the gotchas, troubleshooting steps, and security best practices discovered during implementation.

## Overview

The infrastructure-as-code setup uses:
- **Terraform** for provisioning VMs on Proxmox
- **Ansible** for configuring the Event Store application and PostgreSQL
- **Cloud-init** for initial VM setup
- **Docker** for running PostgreSQL in isolated containers

## Prerequisites

### 1. Proxmox VE Setup

- Proxmox VE 8.x (free edition works fine)
- A cloud-init enabled VM template (e.g., Ubuntu 22.04)
- Network access to Proxmox API (typically port 8006)

### 2. Local Tools

```bash
# Terraform
brew install terraform

# Python for config rendering
cd infra-as-code/tools
python3 -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
```

## Configuration

### 1. Create Your `.env` File

```bash
cd infra-as-code
cp .env.example .env
```

Edit `.env` with your Proxmox details:

```bash
# Proxmox API Configuration
PROXMOX_ENDPOINT=https://192.168.0.69:8006/api2/json
PROXMOX_USER=ansible
PROXMOX_TOKEN_ID=ansible@pve!your-token-name
PROXMOX_TOKEN_SECRET=your-uuid-secret
PROXMOX_INSECURE=true  # Set to false in production with valid certs

# Proxmox Node Configuration
PROXMOX_NODE=pve
PROXMOX_TEMPLATE_NAME=ubuntu-22.04-template
PROXMOX_CIUSER=ubuntu
PROXMOX_SSH_PUBLIC_KEY_PATH=/Users/you/.ssh/nuc-proxmox.pub

# Network Configuration
NETWORK_IP_ADDRESS=192.168.0.100/24
NETWORK_GATEWAY=192.168.0.1
NETWORK_DNS=192.168.0.1
SSH_ALLOWED_CIDRS=192.168.0.0/24
NETWORK_BRIDGE=vmbr0

# Storage Configuration
STORAGE_POOL=local-lvm
DISK_SIZE=20

# Compute Resources
VM_CPU_CORES=2
VM_MEMORY_MB=2048

# Event Store Configuration
EVENT_STORE_GRPC_PORT=50051
EVENT_STORE_HTTP_PORT=8080
EVENT_STORE_METRICS_PORT=9090

# Environment Metadata
ENVIRONMENT=local
OWNER=developer
```

**Note:** The database configuration is handled by Ansible, not Terraform. Each VM gets its own PostgreSQL instance in Docker.

### 2. Generate Configuration

```bash
./generate-config.sh
make proxmox-render-local
```

## Proxmox API Token Setup

### Creating an API Token

1. Log into Proxmox web UI
2. Navigate to **Datacenter → Permissions → API Tokens**
3. Click **Add**
4. Fill in:
   - **User**: Select your user (e.g., `ansible@pve`)
   - **Token ID**: Give it a descriptive name (e.g., `terraform-2025-09`)
   - **Privilege Separation**: ❌ **UNCHECK THIS** (critical for simplicity)
   - **Expire**: Set to 0 (never) or your preferred expiration
5. **Copy the secret immediately** - you won't see it again!

### ⚠️ Critical: Privilege Separation

**Privilege Separation = Unchecked (Recommended for automation)**
- Token inherits all permissions from the user
- Simpler to manage
- Suitable for dedicated service accounts
- Perfect for home labs and testing

**Privilege Separation = Checked (More secure, more complex)**
- Token has its own separate permissions
- Requires granting permissions to both user AND token
- Better for production multi-tenant environments
- More granular control

### Granting Permissions

The Telmate Proxmox Terraform provider (v2.9.14+) requires extensive permissions. Grant the user `Administrator` role:

#### Via Web UI:

1. **Datacenter → Permissions** (click the main "Permissions" item, not sub-items)
2. Click **Add → User Permission**
3. **Path**: `/`
4. **User**: `ansible@pve` (your automation user)
5. **Role**: `Administrator`
6. **Propagate**: ✅ Checked

#### Via CLI (SSH into Proxmox):

```bash
# Grant Administrator role to user
pveum acl modify / -user ansible@pve -role Administrator

# If using privilege separation, also grant to token
pveum acl modify / -token 'ansible@pve!your-token-name' -role Administrator

# Verify permissions
pveum acl list

# Check token privilege separation status
pveum user token list ansible@pve
```

**Expected output:**
```
┌─────────────────┬─────────┬────────┬─────────┐
│ tokenid         │ comment │ expire │ privsep │
╞═════════════════╪═════════╪════════╪═════════╡
│ your-token-name │         │      0 │ 0       │  ← privsep: 0 is good!
└─────────────────┴─────────┴────────┴─────────┘
```

### Minimum Required Permissions

If you want to use a more restrictive role than `Administrator`, you need at least:

- `VM.Allocate` - Create VMs
- `VM.Clone` - Clone templates
- `VM.Config.Disk` - Configure disks
- `VM.Config.CPU` - Configure CPU
- `VM.Config.Memory` - Configure memory
- `VM.Config.Network` - Configure network
- `VM.Config.Options` - Configure VM options
- `Datastore.AllocateSpace` - Use storage
- `SDN.Use` - Use network bridges
- **`Sys.Audit`** ← Required in v2.9.14+
- **`Sys.Modify`** ← Required in v2.9.14+
- **`Sys.Console`** ← Required in v2.9.14+

## SSH Key Management

### Generating SSH Keys

For Proxmox deployment, generate a dedicated SSH key:

```bash
# Generate a dedicated key for Proxmox automation
ssh-keygen -t rsa -b 4096 -f ~/.ssh/nuc-proxmox -N "" -C "nuc-proxmox-terraform"
```

Update your `.env`:

```bash
PROXMOX_SSH_PUBLIC_KEY_PATH=/Users/you/.ssh/nuc-proxmox.pub
```

### 🔐 Security Best Practices

#### For Development/Home Lab:
- ✅ Dedicated SSH key per environment
- ✅ Store `.env` file locally (gitignored)
- ✅ Use descriptive key names
- ✅ Rotate keys periodically (every 3-6 months)
- ✅ Disable privilege separation for simplicity

#### For Production:
- ✅ Use **1Password** or **HashiCorp Vault** for secrets
- ✅ Enable privilege separation on API tokens
- ✅ Use short-lived tokens with expiration
- ✅ Implement key rotation policies
- ✅ Use SSH certificate authorities
- ✅ Enable MFA on Proxmox accounts
- ✅ Use valid TLS certificates (set `PROXMOX_INSECURE=false`)
- ✅ Audit token usage regularly

### Using 1Password (Recommended for Production)

```bash
# Store SSH key in 1Password
op item create --category="SSH Key" \
  --title="NUC Proxmox Terraform" \
  --vault="Infrastructure"

# Reference secrets in shell
export PROXMOX_TOKEN_SECRET=$(op read "op://Infrastructure/Proxmox Token/credential")

# Run Terraform with secrets from 1Password
./generate-config.sh
make proxmox-terraform-plan
```

## Terraform Workflow

### 1. Initialize Terraform

```bash
cd infra-as-code/proxmox/provision/terraform/envs/local
terraform init
```

### 2. Plan Infrastructure Changes

```bash
# Via Terraform directly
terraform plan -var-file=terraform.tfvars.json

# Or via Makefile (recommended)
cd infra-as-code
make proxmox-terraform-plan
```

Review the plan carefully before applying!

### 3. Apply Infrastructure

```bash
# Via Terraform
terraform apply -var-file=terraform.tfvars.json

# Or via Makefile
make proxmox-terraform-apply
```

### 4. Destroy Infrastructure

```bash
terraform destroy -var-file=terraform.tfvars.json
```

## Common Issues & Troubleshooting

### Issue 1: "no such token" Error

**Error:**
```
Error: 401 no such token 'token-name' for user 'user@pve'
```

**Cause:** Token doesn't exist or token ID format is incorrect.

**Solution:**
1. Verify token exists in Proxmox UI: **Datacenter → Permissions → API Tokens**
2. Check token ID format: `user@realm!token-name`
3. Ensure realm matches:
   - `@pam` = Linux system users (e.g., `root@pam`)
   - `@pve` = Proxmox VE users (e.g., `ansible@pve`)
4. Verify the token hasn't expired

### Issue 2: "cannot retrieve user list" Error

**Error:**
```
Error: user ansible@pve has valid credentials but cannot retrieve user list, check privilege separation of api token
```

**Cause:** Insufficient permissions or privilege separation enabled without proper token permissions.

**Solution:**
1. Grant `Administrator` role to the user:
   ```bash
   pveum acl modify / -user ansible@pve -role Administrator
   ```
2. If privilege separation is enabled, also grant to token:
   ```bash
   pveum acl modify / -token 'ansible@pve!token-name' -role Administrator
   ```
3. **Or** recreate token with privilege separation **disabled** (easier)

### Issue 3: Missing Sys.Modify Permission

**Error:**
```
Error: permissions for user/token are not sufficient, please provide also the following permissions that are missing: [Sys.Modify]
```

**Cause:** Telmate provider v2.9.14+ requires system-level permissions that `PVEAdmin` doesn't include.

**Solution:**
```bash
# Grant Administrator role (includes all Sys.* permissions)
pveum acl modify / -user ansible@pve -role Administrator
```

### Issue 4: SSH Key Path with Tilde (~)

**Error:**
```
Error: Invalid function argument
no file exists at "~/.ssh/id_rsa.pub"
```

**Cause:** Terraform's `file()` function doesn't expand `~` (tilde).

**Solution:** Always use absolute paths in `.env`:
```bash
PROXMOX_SSH_PUBLIC_KEY_PATH=/Users/yourname/.ssh/nuc-proxmox.pub
```

### Issue 5: Template Not Found

**Error:**
```
Error: VM template 'ubuntu-22.04-template' not found
```

**Cause:** Template doesn't exist, name is incorrect, or it's on a different node.

**Solution:**
1. Verify template exists in Proxmox UI
2. Check exact template name (case-sensitive)
3. Ensure template is on the correct node specified in `PROXMOX_NODE`
4. Verify template has cloud-init support enabled

### Issue 6: IP Address Already in Use

**Error:**
```
Error: IP address already in use
```

**Solution:**
1. Check your network for IP conflicts: `ping 192.168.0.100`
2. Update `NETWORK_IP_ADDRESS` in `.env` to an available IP
3. Regenerate config: `./generate-config.sh && make proxmox-render-local`
4. Replan: `make proxmox-terraform-plan`

### Issue 7: Realm Mismatch (@pam vs @pve)

**Error:**
```
Error: 401 no such user ('root@pve')
```

**Cause:** User realm doesn't match token realm.

**Solution:**
- `root` user is always `root@pam` (not `@pve`)
- Custom users are typically `username@pve`
- Token ID must match user realm: `root@pam!token` or `ansible@pve!token`

## Database Configuration

**Important:** The database is **NOT** configured by Terraform. Each VM manages its own PostgreSQL instance.

### Architecture:
- **Terraform**: Provisions the VM infrastructure
- **Ansible**: Installs Docker, runs PostgreSQL container, configures Event Store
- **PostgreSQL**: Runs in Docker on each VM (isolated per environment)

### Why This Approach?
- ✅ Each environment is self-contained
- ✅ No shared database dependencies
- ✅ Easy to spin up/down environments
- ✅ Consistent across dev/staging/prod
- ✅ Database credentials managed by Ansible, not Terraform

## Next Steps

After Terraform creates the VM:

1. **Verify VM is running** in Proxmox UI
2. **Check VM has correct IP**:
   ```bash
   ping 192.168.0.100
   ```
3. **Test SSH access**:
   ```bash
   ssh -i ~/.ssh/nuc-proxmox ubuntu@192.168.0.100
   ```
4. **Run Ansible configuration**:
   ```bash
   cd infra-as-code
   make proxmox-ansible-configure
   ```
5. **Verify Event Store** is running:
   ```bash
   curl http://192.168.0.100:8080/health
   ```

## Security Checklist

Before deploying to production:

- [ ] Use valid TLS certificates (`PROXMOX_INSECURE=false`)
- [ ] Enable privilege separation on API tokens
- [ ] Set token expiration dates
- [ ] Use 1Password or Vault for secrets
- [ ] Enable MFA on Proxmox accounts
- [ ] Implement SSH certificate authorities
- [ ] Set up key rotation policies
- [ ] Audit token usage regularly
- [ ] Use firewall rules to restrict access
- [ ] Enable Proxmox audit logging
- [ ] Regularly update Proxmox and templates
- [ ] Use separate tokens per environment

## Troubleshooting Checklist

If Terraform fails:

1. **Check Proxmox connectivity**:
   ```bash
   curl -k https://192.168.0.69:8006/api2/json/version
   ```

2. **Verify token works**:
   ```bash
   curl -k -H "Authorization: PVEAPIToken=ansible@pve!token-name=secret-uuid" \
     https://192.168.0.69:8006/api2/json/nodes
   ```

3. **Check permissions**:
   ```bash
   ssh root@192.168.0.69
   pveum acl list
   pveum user token list ansible@pve
   ```

4. **Verify template exists**:
   ```bash
   ssh root@192.168.0.69
   qm list | grep template
   ```

5. **Check Terraform logs**:
   ```bash
   TF_LOG=DEBUG terraform plan -var-file=terraform.tfvars.json
   ```

## Real-World Troubleshooting Journey

This section documents actual issues encountered during deployment and their solutions.

### Issue 1: "no such token" Error

**Error:**
```
Error: 401 no such token 'ansible-2025-09' for user 'ansible@pve'
```

**Root Cause:** Token ID format was incorrect in the `.env` file.

**Solution:**
- Token ID must include the realm: `ansible@pve!token-name`
- Not just: `token-name`
- Updated `.env` with correct format: `PROXMOX_TOKEN_ID=ansible@pve!ansible-2025-09`

### Issue 2: "cannot retrieve user list" - Privilege Separation

**Error:**
```
Error: user ansible@pve has valid credentials but cannot retrieve user list, check privilege separation of api token
```

**Root Cause:** Token had privilege separation enabled without proper permissions.

**Solution:**
1. Recreated token with privilege separation **disabled** (unchecked)
2. Granted `Administrator` role to user:
   ```bash
   pveum acl modify / -user ansible@pve -role Administrator
   ```
3. Verified privilege separation status:
   ```bash
   pveum user token list ansible@pve
   # Should show privsep: 0
   ```

### Issue 3: Missing Sys.Modify Permission

**Error:**
```
Error: permissions for user/token are not sufficient, please provide also the following permissions that are missing: [Sys.Modify]
```

**Root Cause:** Telmate provider v2.9.14+ requires system-level permissions that `PVEAdmin` role doesn't include.

**Solution:**
```bash
pveum acl modify / -user ansible@pve -role Administrator
```

The `Administrator` role includes all necessary permissions including `Sys.Modify`, `Sys.Audit`, and `Sys.Console`.

> **⚠️ SECURITY WARNING**
> 
> Using the `Administrator` role is **NOT recommended for production environments**. This grants full access to your Proxmox cluster.
> 
> **For Production:**
> - Create a custom role with only the minimum required permissions
> - Use principle of least privilege
> - Separate tokens for different automation tasks
> - Enable privilege separation and grant permissions to both user and token
> - Regularly audit token usage
> - Set token expiration dates
> 
> **For Home Lab/Development:**
> - `Administrator` role is acceptable for simplicity
> - Still use dedicated service accounts (not root)
> - Rotate tokens periodically
> 
> See the [Security Checklist](#security-checklist) section for production best practices.

### Issue 4: SSH Key Path with Tilde (~)

**Error:**
```
Error: Invalid function argument
no file exists at "~/.ssh/id_rsa.pub"
```

**Root Cause:** Terraform's `file()` function doesn't expand `~` (tilde).

**Solution:**
1. Always use absolute paths in `.env`:
   ```bash
   PROXMOX_SSH_PUBLIC_KEY_PATH=/Users/yourname/.ssh/nuc-proxmox.pub
   ```
2. Generate dedicated SSH key:
   ```bash
   ssh-keygen -t rsa -b 4096 -f ~/.ssh/nuc-proxmox -N "" -C "nuc-proxmox-terraform"
   ```

### Issue 5: EOF Error with Telmate Provider

**Error:**
```
Error: Post "https://192.168.0.69:8006/api2/json/nodes/nuc/qemu/169/clone": EOF
```

**Root Cause:** This was actually caused by **Proxmox disk being full** (not a provider bug initially suspected).

**Diagnosis Steps:**
1. Tested API with curl (worked fine):
   ```bash
   curl -k -H 'Authorization: PVEAPIToken=ansible@pve!token=secret' \
     -X POST 'https://192.168.0.69:8006/api2/json/nodes/nuc/qemu/169/clone' \
     -d 'newid=998&name=test-clone&full=1'
   # Returned: HTTP 200 with task ID
   ```

2. Checked Proxmox logs:
   ```bash
   journalctl -u pveproxy -n 50 --no-pager
   # Found: "No space left on device"
   ```

3. Checked disk usage:
   ```bash
   df -h
   # /dev/mapper/pve-root   94G   94G     0 100% /
   ```

4. Found culprit:
   ```bash
   du -sh /var/lib/vz/*
   # 86G	/var/lib/vz/dump  (old VM backups!)
   ```

**Solution:**
1. Delete old backups:
   ```bash
   # Keep only most recent backups
   rm /var/lib/vz/dump/vzdump-*-2025_0[1-8]_*
   rm /var/lib/vz/dump/vzdump-*-2024_*
   ```

2. Configure backup retention in Proxmox UI:
   - Go to **Datacenter → Backup**
   - Edit backup job
   - Set **Retention → Keep Last: 2**
   - This prevents disk from filling up again

3. Verify space:
   ```bash
   df -h /
   # Should show 60GB+ available
   ```

**Key Lesson:** EOF errors can be caused by disk space issues, not just network/API problems!

### Issue 6: Telmate vs BPG Provider

After fixing disk space, we switched from `Telmate/proxmox` to `bpg/proxmox` provider for better reliability:

**Old (Telmate):**
```hcl
required_providers {
  proxmox = {
    source  = "Telmate/proxmox"
    version = ">= 2.9.0"
  }
}

provider "proxmox" {
  pm_api_url          = var.proxmox.endpoint
  pm_user             = var.proxmox.user
  pm_api_token_id     = var.proxmox.token_id
  pm_api_token_secret = var.proxmox.token_secret
  pm_tls_insecure     = var.proxmox.insecure
}
```

**New (BPG - Recommended):**
```hcl
required_providers {
  proxmox = {
    source  = "bpg/proxmox"
    version = "~> 0.50"
  }
}

provider "proxmox" {
  endpoint  = var.proxmox.endpoint
  api_token = "${var.proxmox.token_id}=${var.proxmox.token_secret}"
  insecure  = var.proxmox.insecure
  
  ssh {
    agent = true
  }
}
```

**Benefits of BPG provider:**
- More actively maintained
- Better API compatibility
- Cleaner syntax
- More reliable clone operations

### Issue 7: Disk Resize Error

**Error:**
```
Error: disk resize failure: requested size (20G) is lower than current size (50G)
```

**Root Cause:** Cannot shrink disks in Proxmox - you can only grow them. The template had a 50GB disk but we tried to create a VM with 20GB.

**Solution:**
1. Update `.env` to match or exceed template disk size:
   ```bash
   DISK_SIZE=50  # Must be >= template disk size
   ```

2. Regenerate configuration:
   ```bash
   ./generate-config.sh
   make proxmox-render-local
   ```

**Key Lesson:** Always check your template's disk size before cloning. Set `DISK_SIZE` to match or exceed it.

### Issue 8: Tag Format with BPG Provider

**Error:**
```
Error: Parameter verification failed. (tags: invalid format - invalid characters in tag)
```

**Root Cause:** The bpg/proxmox provider doesn't accept `=` signs in tags. We were using `key=value` format.

**Solution:**
Changed tag format from `key=value` to `key-value`:
```hcl
tags = [for k, v in local.tags_map : "${k}-${v}"]
```

### Issue 9: Template Without Cloud-Init (THE BIG ONE!)

**Symptoms:**
```
ssh: connect to host 192.168.0.100 port 22: No route to host
ping: 100% packet loss
qm guest cmd 103 network-get-interfaces: No QEMU guest agent configured
```

VM runs for 24+ hours but is completely unreachable on the network.

**Root Cause:** The Proxmox template (`docker-dev-prod`) was created without cloud-init support. It had:
- ❌ Ubuntu ISO still attached (`ide2: local:iso/ubuntu-24.04.1-live-server-amd64.iso`)
- ❌ No cloud-init drive
- ❌ No guest agent enabled
- ❌ Cloud-init not installed in the OS

**What Happened:**
1. Terraform successfully cloned the VM
2. Terraform passed cloud-init configuration (network, SSH keys, packages)
3. VM booted but **cloud-init never ran** (not installed)
4. VM sat there with no network configuration
5. All our troubleshooting was trying to configure an unconfigurable VM!

**Diagnosis Steps:**
```bash
# Check template configuration
qm config 169

# Look for these (should exist but didn't):
# ide2: local-lvm:vm-169-cloudinit  ❌ Missing!
# agent: 1                           ❌ Missing!

# Check if template has cloud-init
qm config 169 | grep cloudinit
# (returned nothing - that's the problem!)
```

**Solution: Create Proper Cloud-Init Template**

Use Ubuntu's official cloud image instead of manual installation:

```bash
ssh root@192.168.0.69

# 1. Download Ubuntu 24.04 cloud image
cd /var/lib/vz/template/iso
wget https://cloud-images.ubuntu.com/noble/current/noble-server-cloudimg-amd64.img

# 2. Create a new VM (ID 9000)
qm create 9000 --name ubuntu-2404-cloud \
  --memory 2048 --cores 2 --net0 virtio,bridge=vmbr0

# 3. Import the cloud image as a disk
qm importdisk 9000 noble-server-cloudimg-amd64.img local-lvm

# 4. Attach the disk to the VM
qm set 9000 --scsihw virtio-scsi-pci --scsi0 local-lvm:vm-9000-disk-0

# 5. Add cloud-init drive (THE CRITICAL PART!)
qm set 9000 --ide2 local-lvm:cloudinit

# 6. Make the disk bootable
qm set 9000 --boot c --bootdisk scsi0

# 7. Add serial console (for debugging)
qm set 9000 --serial0 socket --vga serial0

# 8. Enable QEMU guest agent
qm set 9000 --agent enabled=1

# 9. Convert to template
qm template 9000

echo "✅ Cloud-init template created!"
```

**Update Configuration:**
```bash
# Update .env
PROXMOX_TEMPLATE_NAME=ubuntu-2404-cloud

# Regenerate and apply
./generate-config.sh
make proxmox-render-local
make proxmox-terraform-apply
```

**Verify Template Has Cloud-Init:**
```bash
qm config 9000 | grep -E "ide2|agent"
# Should show:
# agent: enabled=1
# ide2: local-lvm:cloudinit,media=cdrom
```

**Key Lessons:**
- **Never create templates from manual installations** - use official cloud images
- **Always verify cloud-init is configured** before using as a template
- **Cloud-init requires:** 
  - Cloud-init package installed in OS
  - Cloud-init drive attached (`ide2: cloudinit`)
  - Guest agent enabled (`agent: 1`)
- **Separation of concerns:**
  - Template = Clean base OS with cloud-init
  - Ansible = Application setup (Docker, packages, config)
  - Don't bake applications into templates!

**Why This Was Hard to Debug:**
- VM appeared to work (status: running)
- Terraform succeeded (VM created)
- No obvious errors in logs
- Network config looked correct in cloud-init dump
- But cloud-init never actually ran!

This is why checking `qm config` for the cloud-init drive is critical!

## Summary of Issues Resolved

1. ✅ **Token format** - Fixed ID format to include realm
2. ✅ **Privilege separation** - Disabled for simplicity
3. ✅ **Permissions** - Granted Administrator role
4. ✅ **SSH key paths** - Used absolute paths
5. ✅ **Disk space** - Cleaned up 86GB of old backups
6. ✅ **Provider** - Switched to bpg/proxmox for reliability
7. ✅ **Tag format** - Changed from `key=value` to `key-value`
8. ✅ **Disk size** - Matched template size (50GB)
9. ✅ **Template without cloud-init** - Created proper cloud-init template from Ubuntu cloud image

**Total issues resolved: 9**  
**Time to resolution: ~24 hours (with 21 hours debugging networking!)**  

**Key takeaways:**
- ⚠️ **ALWAYS verify template has cloud-init before using it!** (`qm config <id> | grep cloudinit`)
- Always check disk space when seeing EOF errors
- Cannot shrink disks in Proxmox, only grow them
- Different providers have different tag format requirements
- Use official cloud images instead of manual template creation
- Separate concerns: Template = base OS, Ansible = application setup

## Additional Resources

- [Proxmox VE Documentation](https://pve.proxmox.com/pve-docs/)
- [Telmate Proxmox Provider](https://registry.terraform.io/providers/Telmate/proxmox/latest/docs)
- [Cloud-init Documentation](https://cloudinit.readthedocs.io/)
- [Ansible Documentation](https://docs.ansible.com/)

## Accessing Your VM

The VM is configured with a **static IP address** for reliability and simplicity.

### Default Configuration

```bash
# Static IP (configured in .env)
NETWORK_IP_ADDRESS=192.168.0.100/24
NETWORK_GATEWAY=192.168.0.1
```

### SSH Access

```bash
# SSH to the VM using the static IP
ssh -i ~/.ssh/nuc-proxmox ubuntu@192.168.0.100
```

**No IP discovery needed!** The VM is always at the configured static IP address.

### Verifying Network Configuration

```bash
# On the VM, check IP configuration
ip addr show

# Check connectivity
ping 192.168.0.1  # Gateway
ping 8.8.8.8      # Internet
```

## Serial Console Issues

If the Proxmox console shows "Starting serial terminal interface 00" and gets stuck:

**This is a display issue, not a boot issue.** The VM is likely booting fine, but the serial console isn't configured correctly.

**Workaround:** Use one of the IP discovery methods above to find the VM's IP and SSH in directly.

**Fix (optional):** Remove or modify the `vga: serial0` setting in the template configuration.

## Related Documentation

- [Architecture Decisions](./architecture-decisions.md) - Core principles and design decisions
- [Infrastructure as Code Structure](./infra-as-code-structure.md)
- [Ansible Configuration Guide](./ansible-configuration.md) *(coming soon)*
- [AWS Terraform Deployment](./aws-terraform-deployment.md) *(coming soon)*
