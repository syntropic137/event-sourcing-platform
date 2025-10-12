---
title: Deployment QA & Setup Guide
sidebar_position: 2
---

# Deployment QA & Setup Guide

Use this checklist to validate each infrastructure target (AWS, Proxmox, and local development) before and after running the event store. The steps assume you have cloned the repository and installed the core tooling (Terraform >= 1.5, Ansible >= 2.16, Python 3.10+, `make`).

## 1. Prepare Tooling

- **Terraform**: `brew install terraform` (or package manager of choice). Confirm with `terraform version`.
- **Ansible**: `pipx install ansible` or `pip install ansible`. Confirm with `ansible --version`.
- **Python helper deps**: use the helper script virtual environment.

```bash
cd infra-as-code/tools
python3 -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
```

## 2. Render Environment Configuration

The helper script converts YAML under `infra-as-code/<target>/provision/config/` into Terraform and Ansible artifacts.

```bash
# AWS production example
make -C infra-as-code aws-render-prod

# Proxmox local example
PROXMOX_TOKEN_SECRET=your-token-secret \
  make -C infra-as-code proxmox-render-local
```

Outputs:

- `infra-as-code/<target>/provision/terraform/envs/<env>/terraform.tfvars.json`
- `infra-as-code/<target>/configure/ansible/envs/<env>/group_vars/all.yml`
- `infra-as-code/<target>/configure/ansible/envs/<env>/inventory.ini`

Review generated files to ensure hostnames, secrets, and artifact URLs are correct.

## 3. QA Checkpoint — Terraform

Run fmt/validate for every environment after rendering tfvars.

```bash
make -C infra-as-code aws-terraform-plan
make -C infra-as-code proxmox-terraform-plan
```

If the plan looks correct, apply:

```bash
make -C infra-as-code aws-terraform-apply
make -C infra-as-code proxmox-terraform-apply
```

> **Note:** For Proxmox, ensure `PROXMOX_TOKEN_SECRET` (or an inline `token_secret`) is exported before running any Terraform commands.

## 4. QA Checkpoint — Ansible

After infrastructure provisioning completes, configure each target:

```bash
# AWS EC2 deployment
make -C infra-as-code aws-ansible-configure

# Proxmox VM deployment
make -C infra-as-code proxmox-ansible-configure
```

Perform syntax checks first when iterating on playbooks:

```bash
ANSIBLE_ROLES_PATH=infra-as-code/shared/configure/ansible/roles \
  ansible-playbook --syntax-check \
  -i infra-as-code/aws/configure/ansible/envs/prod/inventory.ini \
  infra-as-code/aws/configure/ansible/envs/prod/playbook.yml
```

Validate service status manually on the target machine (`systemctl status eventstore`) and tail logs for errors.

## 5. Local Development & Smoke Tests

For rapid iteration without provisioning:

```bash
# Start dev Postgres + Redis
make dev-start

# Run event store binary against dev Postgres
BACKEND=postgres \
DATABASE_URL=postgres://dev:dev@localhost:15648/dev \
cargo run -p eventstore-bin --release
```

Run TypeScript examples (or other SDK tests) against the local server to verify end-to-end behaviour. Example:

```bash
cd examples/009-web-dashboard-ts
pnpm install
pnpm start -- --memory
```

## 6. Post-Deployment Verification

- **Connectivity**: verify `grpcurl` or SDK clients can append and read events.
- **Metrics**: access the metrics port (default `:9100`) and ensure Prometheus format output.
- **Dashboards**: for AWS, confirm the dashboard or API endpoints are accessible via the configured host/IP. For Proxmox, verify LAN connectivity.
- **Logs**: check systemd journal or Docker logs for errors.

## 7. Cleanup

Use Terraform destroy to tear down environments when finished testing.

```bash
make -C infra-as-code aws-terraform-apply ACTION=destroy
make -C infra-as-code proxmox-terraform-apply ACTION=destroy
```

(Alternatively run `terraform destroy` manually inside each environment directory.)

## 8. Troubleshooting Tips

- **Secrets**: AWS Secrets Manager lookups require the IAM role/credentials specified in `prod.yml`. Ensure the EC2 instance profile has `secretsmanager:GetSecretValue` permissions.
- **Proxmox token**: if Terraform reports authentication errors, confirm the API token has permissions on the target pool and storage.
- **Ports**: update `allow_public_grpc` or `allow_public_dashboard` in the YAML config if you need to expose services publicly.
- **Binary delivery**: the Ansible role includes a placeholder debug task; replace it with your real installation method (S3 download, rsync, container image) before running in production.

Following this flow keeps AWS, Proxmox, and local testing in sync and ensures every deploy is repeatable and auditable.
