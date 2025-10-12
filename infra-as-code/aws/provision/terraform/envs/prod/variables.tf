variable "aws_region" {
  description = "AWS region to deploy resources into."
  type        = string
}

variable "aws_profile" {
  description = "Optional AWS profile name."
  type        = string
  default     = "default"
}

variable "aws_shared_credentials_file" {
  description = "Path to shared AWS credentials file (optional)."
  type        = string
  default     = null
}

variable "metadata" {
  description = "Metadata tags applied to resources."
  type = object({
    environment = string
    owner       = string
    extra_tags  = optional(map(string), {})
  })
}

variable "event_store" {
  description = "Event store settings and ports."
  type = object({
    version      = string
    grpc_port    = number
    http_port    = number
    metrics_port = number
  })
}

variable "backend" {
  description = "Backend settings including database connection."
  type = object({
    type         = string
    database_url = string
    extra_env    = optional(map(string), {})
  })
}

variable "network" {
  description = "Networking configuration for VPC, subnets, and ingress."
  type = object({
    vpc_id            = string
    public_subnet_ids = list(string)
    ssh_allowed_cidrs = list(string)
  })
}

variable "compute" {
  description = "Instance sizing and provisioning details."
  type = object({
    instance_type  = string
    ami_id         = string
    key_pair_name  = string
    root_volume_gb = number
    ssh_user       = string
  })
}

variable "security" {
  description = "Security-related flags and resources."
  type = object({
    iam_instance_profile   = string
    allow_public_grpc      = bool
    allow_public_dashboard = bool
  })
}
