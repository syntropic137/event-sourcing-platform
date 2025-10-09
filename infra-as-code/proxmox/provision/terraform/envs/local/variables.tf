variable "metadata" {
  description = "Metadata tags applied to resources."
  type = object({
    environment = string
    owner       = string
    extra_tags  = optional(map(string), {})
  })
}

variable "proxmox" {
  description = "Proxmox API and template configuration."
  type = object({
    endpoint            = string
    insecure            = bool
    user                = string
    token_id            = string
    token_secret        = string
    node                = string
    template_name       = string
    template_id         = number
    storage_pool        = string
    network_bridge      = string
    vlan_tag            = number
    pool                = string
    ciuser              = string
    ssh_public_key_path = string
    clone_timeout       = number
  })
}

variable "network" {
  description = "Network configuration for the VM."
  type = object({
    ip_address  = string
    gateway     = string
    dns_servers = list(string)
  })
}

variable "compute" {
  description = "Compute sizing for the Proxmox VM."
  type = object({
    vm_name   = string
    cores     = number
    sockets   = number
    memory_mb = number
    disk_gb   = number
  })
}
