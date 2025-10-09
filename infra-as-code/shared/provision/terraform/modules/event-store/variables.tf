variable "metadata" {
  description = "Metadata tags applied to provisioned resources."
  type = object({
    environment = string
    owner       = string
    extra_tags  = optional(map(string), {})
  })
}

variable "event_store" {
  description = "Event store service configuration (ports, version)."
  type = object({
    version      = string
    grpc_port    = number
    http_port    = number
    metrics_port = number
  })
}

variable "backend" {
  description = "Backend configuration. Database connection is configured by Ansible after PostgreSQL is running."
  type = object({
    type      = string
    extra_env = optional(map(string), {})
  })
}
