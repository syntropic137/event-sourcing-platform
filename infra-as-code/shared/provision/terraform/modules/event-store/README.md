# Event Store Terraform Module

This module centralises common configuration for provisioning the event store across multiple providers (AWS EC2, Proxmox VM, etc.). It exposes shared metadata tags, service ports, and environment variables that downstream stacks consume to keep behaviour consistent.

## Inputs

- `metadata` – Object containing `environment`, `owner`, and optional `extra_tags` (map).
- `event_store` – Object defining the service `version`, `grpc_port`, `http_port`, and `metrics_port`.
- `backend` – Object specifying the storage backend:
  - `type` – e.g., `postgres`
  - `database_url` – Connection string or secrets reference
  - `extra_env` – Optional additional environment variables rendered into cloud-init / Ansible

## Outputs

- `default_tags` – Map of tags to apply to infrastructure resources.
- `service_ports` – Map of port numbers (`grpc`, `http`, `metrics`).
- `service_environment` – Map of environment variables required for the event store service.

## Example Usage

```hcl
module "event_store" {
  source = "../../../../shared/provision/terraform/modules/event-store"

  metadata = {
    environment = var.metadata.environment
    owner       = var.metadata.owner
    extra_tags  = var.metadata.extra_tags
  }

  event_store = var.event_store

  backend = {
    type        = var.backend.type
    database_url = var.backend.database_url
    extra_env   = var.backend.extra_env
  }
}
```

Downstream stacks should pair this module with provider-specific resources (e.g., `aws_instance`, `proxmox_vm_qemu`) and pass the module outputs into cloud-init templates, Ansible inventories, or other tooling.
