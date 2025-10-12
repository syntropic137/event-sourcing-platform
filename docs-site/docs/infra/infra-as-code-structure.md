---
title: Infrastructure-as-Code Structure
sidebar_position: 1
---

# Infrastructure-as-Code Structure

This guide describes how the event sourcing platform organizes Terraform and Ansible assets for provisioning and configuring the event store across different deployment targets.

## Directory Overview

```text
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
│   │               └── terraform.tfvars.json (generated)
│   └── configure/
│       └── ansible/
│           └── envs/
│               └── prod/
│                   ├── inventory.ini (generated)
│                   ├── group_vars/
│                   │   └── all.yml (generated)
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
│   │               └── terraform.tfvars.json (generated)
│   └── configure/
│       └── ansible/
│           └── envs/
│               └── local/
│                   ├── inventory.ini (generated)
│                   ├── group_vars/
│                   │   └── all.yml (generated)
│                   └── playbook.yml
└── shared/
    ├── provision/
    │   └── terraform/
    │       └── modules/
    │           └── event-store/
    │               ├── main.tf
    │               ├── variables.tf
    │               └── outputs.tf
    └── configure/
        └── ansible/
            └── roles/
                └── event-store/
                    ├── defaults/main.yml
                    ├── tasks/main.yml
                    └── templates/*
```

- **Provisioning** assets (Terraform + environment configs) live under `provision/`.
- **Configuration** assets (Ansible roles and playbooks) live under `configure/`.
- **Shared** modules and roles help avoid duplication between AWS and Proxmox stacks.

## Workflow Summary

1. **Author environment config** in YAML (`config/*.yml`).
2. **Render helper outputs** that convert YAML into Terraform `terraform.tfvars.json` and Ansible inventories/group vars.
3. **Provision infrastructure** with Terraform using the generated variables.
4. **Configure hosts** with Ansible playbooks referencing the generated inventories.

## Validation

- Run `terraform fmt` and `terraform validate` within each `envs/<environment>/` directory.
- Execute `ansible-playbook --syntax-check` for each environment playbook.
- Optional: add `ansible-lint` or integration tests if tooling is available.

## Next Steps

- Implement shared Terraform module at `infra-as-code/shared/provision/terraform/modules/event-store/`.
- Populate target-specific Terraform and Ansible files to align with this structure.
- Extend documentation with deployment walkthroughs once stacks are ready.
