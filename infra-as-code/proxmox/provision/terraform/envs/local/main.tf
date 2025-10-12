terraform {
  required_version = ">= 1.5.7"

  required_providers {
    proxmox = {
      source  = "bpg/proxmox"
      version = "~> 0.50"
    }
    local = {
      source  = "hashicorp/local"
      version = ">= 2.4.0"
    }
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

locals {
  # Tags for VM resources
  tags_map = merge({
    "app"         = "event-store"
    "environment" = var.metadata.environment
    "owner"       = var.metadata.owner
  }, var.metadata.extra_tags)
  
  tags_list = [for key, value in local.tags_map : "${key}=${value}"]
  tags      = join(";", local.tags_list)

  cloud_init_path = "${path.module}/cloud-init-user-data.yml"
}

resource "proxmox_virtual_environment_vm" "event_store" {
  name        = var.compute.vm_name
  node_name   = var.proxmox.node
  pool_id     = var.proxmox.pool != "" ? var.proxmox.pool : null
  
  clone {
    vm_id = var.proxmox.template_id
    full  = true
  }
  
  cpu {
    cores   = var.compute.cores
    sockets = var.compute.sockets
  }
  
  memory {
    dedicated = var.compute.memory_mb
  }
  
  disk {
    datastore_id = var.proxmox.storage_pool
    size         = var.compute.disk_gb
    interface    = "scsi0"
  }
  
  network_device {
    bridge = var.proxmox.network_bridge
    model  = "virtio"
  }
  
  initialization {
    ip_config {
      ipv4 {
        address = var.network.ip_address
        gateway = var.network.gateway
      }
    }
    
    user_account {
      username = var.proxmox.ciuser
      keys     = [trimspace(file(var.proxmox.ssh_public_key_path))]
    }
  }
  
  on_boot = true
  tags    = [for k, v in local.tags_map : "${k}-${v}"]
}

output "provisioned_vm_id" {
  description = "The identifier of the provisioned Proxmox VM."
  value       = proxmox_virtual_environment_vm.event_store.id
}
