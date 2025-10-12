# Infrastructure-as-Code Overview

This directory contains Terraform and Ansible assets for provisioning and configuring the event store across multiple targets (AWS and Proxmox).

## 🚀 Quick Start

**Deploy Event Store to Proxmox with one command:**

```bash
# 1. Configure (one time)
cp .env.example .env
# Edit .env with your Proxmox credentials

# 2. Deploy everything
make proxmox-deploy-full
```

**That's it!** The command will:
- ✅ Validate configuration
- ✅ Build Docker image
- ✅ Provision VM with Terraform
- ✅ Deploy Event Store with Ansible
- ✅ Verify deployment

**See [DEPLOYMENT-GUIDE.md](./DEPLOYMENT-GUIDE.md) for detailed instructions.**

## 📋 Workflow

Every environment follows a "provision then configure" workflow:

1. **Configure:** Edit `.env` file with your settings
2. **Generate:** Run `./generate-config.sh` to create Terraform and Ansible configs
3. **Provision:** Apply Terraform to create VM infrastructure
4. **Configure:** Run Ansible to install and configure Event Store

## Directory Layout

```
infra-as-code/
├── aws/
│   ├── provision/
│   │   ├── config/
│   │   │   └── prod.yml
│   │   └── terraform/
│   │       └── envs/
│   │           └── prod/
│   │               ├── main.tf
│   │               ├── variables.tf
│   │               └── templates/cloud-init.sh.tftpl
│   └── configure/
│       └── ansible/
│           └── envs/
│               └── prod/
│                   ├── inventory.ini
│                   ├── group_vars/all.yml
│                   └── playbook.yml
├── proxmox/
│   ├── provision/
│   │   ├── config/
│   │   │   └── local.yml
│   │   └── terraform/
│   │       └── envs/
│   │           └── local/
│   │               ├── main.tf
│   │               ├── variables.tf
│   │               └── templates/cloud-init.yml.tftpl
│   └── configure/
│       └── ansible/
│           └── envs/
│               └── local/
│                   ├── inventory.ini
│                   ├── group_vars/all.yml
│                   └── playbook.yml
└── shared/
    ├── provision/
    │   └── terraform/
    │       └── modules/
    │           └── event-store/
    │               ├── main.tf
    │               ├── variables.tf
    │               ├── outputs.tf
    │               └── README.md
    └── configure/
        └── ansible/
            └── roles/
                └── event-store/
                    ├── defaults/main.yml
                    ├── handlers/main.yml
                    ├── tasks/main.yml
                    └── templates/eventstore.service.j2
```

## 📝 Configuration File (.env)

The `.env` file contains **all** configuration for both Terraform (infrastructure) and Ansible (application):

**Terraform Section (Infrastructure):**
- Proxmox connection (endpoint, credentials)
- VM specs (CPU, memory, disk)
- Network configuration (static IP, gateway)
- Template settings

**Ansible Section (Application):**
- PostgreSQL configuration (password, port, database)
- Event Store configuration (gRPC port, binary URL)
- Service settings (user, group, directories)
- Environment variables (RUST_LOG, etc.)

See `.env.example` for all available options.

## Rendering Configuration

### Automated (Recommended)

```bash
# Generate all configs from .env
./generate-config.sh

# This creates:
# 1. proxmox/provision/config/local.yml (intermediate YAML)
# 2. terraform.tfvars.json (Terraform variables)
# 3. inventory.ini (Ansible inventory)
# 4. group_vars/all.yml (Ansible variables)
```

### Manual (Advanced)

`tools/render_config.py` converts environment YAML into Terraform and Ansible artifacts. It requires **PyYAML** (`pip install -r tools/requirements.txt`).

```bash
cd infra-as-code/tools
python3 -m venv .venv && source .venv/bin/activate
pip install -r requirements.txt
```

Examples:

```bash
# AWS prod
python tools/render_config.py --target aws --env prod

# Proxmox local
python tools/render_config.py --target proxmox --env local
```

Generated files:

- `infra-as-code/<target>/provision/terraform/envs/<env>/terraform.tfvars.json`
- `infra-as-code/<target>/configure/ansible/envs/<env>/inventory.ini`
- `infra-as-code/<target>/configure/ansible/envs/<env>/group_vars/all.yml`

### Make Targets

A convenience `Makefile` provides composable commands:

```bash
# Render configuration
make -C infra-as-code aws-render-prod
make -C infra-as-code proxmox-render-local

# Terraform
make -C infra-as-code aws-terraform-plan
make -C infra-as-code aws-terraform-apply

# Ansible
make -C infra-as-code aws-ansible-configure
```

Each Make target expects the rendered files from the helper script and any secrets exported via environment variables (e.g., `PROXMOX_TOKEN_SECRET`).

## Running Terraform

```bash
cd infra-as-code/aws/provision/terraform/envs/prod
terraform init
terraform plan -var-file=terraform.tfvars.json
terraform apply -var-file=terraform.tfvars.json
```

For Proxmox, ensure the `PROXMOX_TOKEN_SECRET` environment variable is set before running `terraform`.

## Running Ansible

```bash
export ANSIBLE_ROLES_PATH=$(pwd)/../../../shared/configure/ansible/roles
cd infra-as-code/aws/configure/ansible/envs/prod
ansible-playbook -i inventory.ini playbook.yml
```

Adjust inventory hostnames, SSH keys, and Secrets Manager lookups as needed. The Ansible role expects the event store binary to be supplied; replace the placeholder debug task with your installation mechanism.

## Validation

- `terraform fmt -recursive infra-as-code`
- `terraform validate` inside each `terraform/envs/<env>/` directory after rendering tfvars.
- `ansible-playbook --syntax-check -i inventory.ini playbook.yml`
- Optional: `ansible-lint` if available.

Keep secrets (AWS credential files, Proxmox tokens) outside the repository and load them through environment variables or `terraform.tfvars.json` files that are gitignored.
