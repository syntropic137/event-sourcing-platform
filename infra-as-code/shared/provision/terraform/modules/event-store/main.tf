terraform {
  required_version = ">= 1.5.7"
}

locals {
  tags = merge({
    "app"         = "event-store",
    "environment" = var.metadata.environment,
    "owner"       = var.metadata.owner,
  }, var.metadata.extra_tags)

  ports = {
    grpc    = var.event_store.grpc_port
    http    = var.event_store.http_port
    metrics = var.event_store.metrics_port
  }

  # Note: DATABASE_URL is intentionally NOT included here.
  # Ansible will configure the database connection after PostgreSQL is running on the VM.
  service_environment = merge({
    "BACKEND"      = var.backend.type,
    "GRPC_PORT"    = tostring(var.event_store.grpc_port),
    "HTTP_PORT"    = tostring(var.event_store.http_port),
    "METRICS_PORT" = tostring(var.event_store.metrics_port),
    "ENVIRONMENT"  = var.metadata.environment,
  }, var.backend.extra_env)
}

# TODO: Instantiate provider-specific resources (compute instance, networking, security)
#       in the calling stacks. This module standardizes shared metadata, ports, and
#       service environment outputs for downstream Terraform and Ansible usage.
