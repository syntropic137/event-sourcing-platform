terraform {
  required_version = ">= 1.5.7"

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = ">= 5.0"
    }
  }
}

provider "aws" {
  region                   = var.aws_region
  profile                  = var.aws_profile
  shared_credentials_files = var.aws_shared_credentials_file == null ? null : [var.aws_shared_credentials_file]
  default_tags {
    tags = module.event_store.default_tags
  }
}

locals {
  lb_name = "${var.metadata.environment}-event-store"

  grpc_cidrs = var.security.allow_public_grpc ? ["0.0.0.0/0"] : var.network.ssh_allowed_cidrs
  http_cidrs = var.security.allow_public_dashboard ? ["0.0.0.0/0"] : var.network.ssh_allowed_cidrs
}

module "event_store" {
  source = "../../../../../shared/provision/terraform/modules/event-store"

  metadata    = var.metadata
  event_store = var.event_store
  backend     = var.backend
}

resource "aws_security_group" "event_store" {
  name        = "${var.metadata.environment}-event-store-sg"
  description = "Security group for event store ingress"
  vpc_id      = var.network.vpc_id

  ingress {
    description = "SSH access"
    protocol    = "tcp"
    from_port   = 22
    to_port     = 22
    cidr_blocks = var.network.ssh_allowed_cidrs
  }

  ingress {
    description = "gRPC access"
    protocol    = "tcp"
    from_port   = module.event_store.service_ports.grpc
    to_port     = module.event_store.service_ports.grpc
    cidr_blocks = local.grpc_cidrs
  }

  ingress {
    description = "HTTP dashboard access"
    protocol    = "tcp"
    from_port   = module.event_store.service_ports.http
    to_port     = module.event_store.service_ports.http
    cidr_blocks = local.http_cidrs
  }

  ingress {
    description = "Metrics scraping"
    protocol    = "tcp"
    from_port   = module.event_store.service_ports.metrics
    to_port     = module.event_store.service_ports.metrics
    cidr_blocks = var.network.ssh_allowed_cidrs
  }

  egress {
    description = "Allow all outbound"
    protocol    = "-1"
    from_port   = 0
    to_port     = 0
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = merge(module.event_store.default_tags, {
    "Name" = "${var.metadata.environment}-event-store-sg"
  })
}

resource "aws_instance" "event_store" {
  ami                         = var.compute.ami_id
  instance_type               = var.compute.instance_type
  subnet_id                   = var.network.public_subnet_ids[0]
  key_name                    = var.compute.key_pair_name
  iam_instance_profile        = var.security.iam_instance_profile
  associate_public_ip_address = true
  vpc_security_group_ids      = [aws_security_group.event_store.id]

  root_block_device {
    volume_size = var.compute.root_volume_gb
    volume_type = "gp3"
  }

  tags = merge(module.event_store.default_tags, {
    "Name" = "${var.metadata.environment}-event-store"
  })

  user_data = templatefile("${path.module}/templates/cloud-init.sh.tftpl", {
    service_environment = module.event_store.service_environment
    ssh_user            = var.compute.ssh_user
  })
}

# Placeholder for optional outputs to drive DNS, load balancers, or further automation.
