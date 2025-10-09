#!/usr/bin/env python3
"""Render IaC environment configuration from YAML into Terraform and Ansible artifacts."""

import argparse
import json
import os
import sys
from pathlib import Path

try:
    import yaml  # type: ignore
except ModuleNotFoundError:  # pragma: no cover
    sys.stderr.write(
        "[render_config] Missing dependency: PyYAML\n"
        "Install it with: pip install pyyaml\n"
    )
    sys.exit(1)

ROOT = Path(__file__).resolve().parent.parent


def load_yaml(path: Path) -> dict:
    with path.open("r", encoding="utf-8") as handle:
        return yaml.safe_load(handle)


def write_json(path: Path, data: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as handle:
        json.dump(data, handle, indent=2)
        handle.write("\n")


def write_text(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as handle:
        handle.write(content)


def write_yaml(path: Path, data: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as handle:
        yaml.safe_dump(data, handle, sort_keys=False)


def _stringify_map(values: dict) -> dict:
    return {key: str(value) for key, value in values.items()}


def render_aws(env: str, cfg: dict) -> list[str]:
    provider_cfg = cfg["aws"]
    compute_cfg = cfg["compute"]
    metadata_cfg = cfg["metadata"]
    postgres_cfg = cfg["postgres"]
    event_cfg = cfg["event_store"]
    ansible_cfg = cfg["ansible"]

    terraform_env_dir = ROOT / "aws" / "provision" / "terraform" / "envs" / env
    ansible_env_dir = ROOT / "aws" / "configure" / "ansible" / "envs" / env

    backend_type = postgres_cfg.get("type", "postgres")
    backend_url = postgres_cfg.get("database_url_secret_arn", "")

    terraform_payload = {
        "aws_region": provider_cfg["region"],
        "aws_profile": provider_cfg.get("profile", "default"),
        "aws_shared_credentials_file": provider_cfg.get("shared_credentials_file"),
        "metadata": {
            "environment": metadata_cfg["environment"],
            "owner": metadata_cfg["owner"],
            "extra_tags": metadata_cfg.get("extra_tags", {}),
        },
        "event_store": {
            "version": event_cfg["version"],
            "grpc_port": event_cfg["grpc_port"],
            "http_port": event_cfg["http_port"],
            "metrics_port": event_cfg["metrics_port"],
        },
        "backend": {
            "type": backend_type,
            "database_url": backend_url,
            "extra_env": postgres_cfg.get("extra_env", {}),
        },
        "network": {
            "vpc_id": cfg["network"]["vpc_id"],
            "public_subnet_ids": cfg["network"]["public_subnet_ids"],
            "ssh_allowed_cidrs": cfg["network"]["ssh_allowed_cidrs"],
        },
        "compute": {
            "instance_type": compute_cfg["instance_type"],
            "ami_id": compute_cfg["ami_id"],
            "key_pair_name": compute_cfg["key_pair_name"],
            "root_volume_gb": compute_cfg["root_volume_gb"],
            "ssh_user": compute_cfg.get("ssh_user", "ubuntu"),
        },
        "security": {
            "iam_instance_profile": cfg["security"]["iam_instance_profile"],
            "allow_public_grpc": cfg["security"].get("allow_public_grpc", False),
            "allow_public_dashboard": cfg["security"].get("allow_public_dashboard", False),
        },
    }

    write_json(terraform_env_dir / "terraform.tfvars.json", terraform_payload)

    database_lookup = backend_url
    if backend_url.startswith("arn:"):
        database_lookup = "{{ lookup('aws_secretsmanager', '%s') }}" % backend_url

    service_environment = {
        "BACKEND": backend_type,
        "DATABASE_URL": database_lookup,
        "GRPC_PORT": str(event_cfg["grpc_port"]),
        "HTTP_PORT": str(event_cfg["http_port"]),
        "METRICS_PORT": str(event_cfg["metrics_port"]),
        "ENVIRONMENT": metadata_cfg["environment"],
    }

    environment_overrides = {}
    environment_overrides.update(_stringify_map(postgres_cfg.get("extra_env", {})))
    environment_overrides.update(_stringify_map(ansible_cfg.get("environment_overrides", {})))

    group_vars_payload = {
        "service_environment": service_environment,
        "environment_overrides": environment_overrides,
        "binary_url": ansible_cfg["binary"]["url"],
        "binary_checksum": ansible_cfg["binary"].get("checksum", ""),
        "service_user": ansible_cfg["service"]["user"],
        "service_group": ansible_cfg["service"]["group"],
        "service_name": ansible_cfg["service"]["name"],
        "install_dir": ansible_cfg["service"].get("install_dir", "/opt/event-store"),
        "service_description": ansible_cfg["service"].get("description", "Event Store Service"),
    }

    write_yaml(ansible_env_dir / "group_vars" / "all.yml", group_vars_payload)

    inventory_line = (
        f"{ansible_cfg['inventory_hostname']} "
        f"ansible_host={ansible_cfg['host']} "
        f"ansible_user={ansible_cfg['ssh_user']} "
        f"ansible_ssh_private_key_file={ansible_cfg['ssh_private_key_path']}"
    )

    inventory_content = (
        f"[{ansible_cfg['inventory_host_group']}]\n" f"{inventory_line}\n"
    )

    write_text(ansible_env_dir / "inventory.ini", inventory_content)

    playbook_path = ansible_env_dir / "playbook.yml"
    if not playbook_path.exists():
        playbook_content = (
            "---\n"
            "- name: Configure event store on AWS EC2 instance\n"
            "  hosts: {host_group}\n"
            "  become: true\n"
            "  vars_files:\n"
            "    - group_vars/all.yml\n"
            "  roles:\n"
            "    - role: ../../../shared/configure/ansible/roles/event-store\n"
        ).format(host_group=ansible_cfg["inventory_host_group"])
        write_text(playbook_path, playbook_content)

    return [
        str(terraform_env_dir / "terraform.tfvars.json"),
        str(ansible_env_dir / "group_vars" / "all.yml"),
        str(ansible_env_dir / "inventory.ini"),
    ]


def render_proxmox(env: str, cfg: dict) -> list[str]:
    proxmox_cfg = cfg["proxmox"]
    metadata_cfg = cfg["metadata"]

    token_secret_env = proxmox_cfg.get("token_secret_env")
    if token_secret_env:
        token_secret = os.environ.get(token_secret_env)
        if not token_secret:
            raise SystemExit(
                f"Environment variable '{token_secret_env}' (referenced in config) is not set"
            )
    else:
        token_secret = proxmox_cfg.get("token_secret", "")

    terraform_env_dir = ROOT / "proxmox" / "provision" / "terraform" / "envs" / env
    ansible_env_dir = ROOT / "proxmox" / "configure" / "ansible" / "envs" / env

    terraform_payload = {
        "metadata": {
            "environment": metadata_cfg["environment"],
            "owner": metadata_cfg["owner"],
            "extra_tags": metadata_cfg.get("extra_tags", {}),
        },
        "proxmox": {
            "endpoint": proxmox_cfg["endpoint"],
            "insecure": proxmox_cfg.get("insecure", False),
            "user": proxmox_cfg["user"],
            "token_id": proxmox_cfg["token_id"],
            "token_secret": token_secret,
            "node": proxmox_cfg["node"],
            "template_name": proxmox_cfg["template_name"],
            "template_id": proxmox_cfg.get("template_id", 9000),
            "storage_pool": proxmox_cfg["storage_pool"],
            "network_bridge": proxmox_cfg["network_bridge"],
            "vlan_tag": proxmox_cfg.get("vlan_tag", 0),
            "pool": proxmox_cfg.get("pool", ""),
            "ciuser": proxmox_cfg.get("ciuser", "ubuntu"),
            "ssh_public_key_path": proxmox_cfg["ssh_public_key_path"],
            "clone_timeout": proxmox_cfg.get("clone_timeout", 600),
        },
        "network": {
            "ip_address": cfg["network"]["ip_address"],
            "gateway": cfg["network"]["gateway"],
            "dns_servers": cfg["network"].get("dns_servers", []),
        },
        "compute": {
            "vm_name": cfg["compute"]["vm_name"],
            "cores": cfg["compute"]["cores"],
            "sockets": cfg["compute"]["sockets"],
            "memory_mb": cfg["compute"]["memory_mb"],
            "disk_gb": cfg["compute"]["disk_gb"],
        },
    }

    write_json(terraform_env_dir / "terraform.tfvars.json", terraform_payload)

    # Generate Ansible configuration files
    ansible_cfg = cfg.get("ansible", {})
    
    # Generate inventory.ini
    inventory_content = f"""[{ansible_cfg.get('inventory_host_group', 'event_store')}]
{ansible_cfg.get('host', '192.168.0.100')} ansible_user={ansible_cfg.get('ssh_user', 'ubuntu')} ansible_ssh_private_key_file={ansible_cfg.get('ssh_private_key_path', '~/.ssh/nuc-proxmox')}

[{ansible_cfg.get('inventory_host_group', 'event_store')}:vars]
ansible_python_interpreter=/usr/bin/python3
"""
    write_text(ansible_env_dir / "inventory.ini", inventory_content)
    
    # Generate group_vars/all.yml
    postgres_cfg = ansible_cfg.get("postgres", {})
    eventstore_cfg = ansible_cfg.get("eventstore", {})
    service_cfg = ansible_cfg.get("service", {})
    
    ansible_vars = {
        "# PostgreSQL configuration": None,
        "postgres_container_name": postgres_cfg.get("container_name", "eventstore-postgres"),
        "postgres_db": postgres_cfg.get("db", "eventstore"),
        "postgres_user": postgres_cfg.get("user", "eventstore"),
        "postgres_password": postgres_cfg.get("password", "changeme"),
        "postgres_port": postgres_cfg.get("port", 5432),
        "postgres_data_dir": postgres_cfg.get("data_dir", "/var/lib/eventstore/postgres"),
        
        "# Event Store configuration": None,
        "eventstore_grpc_port": eventstore_cfg.get("grpc_port", 50051),
        "eventstore_backend": eventstore_cfg.get("backend", "postgres"),
        "binary_url": eventstore_cfg.get("binary_url", ""),
        
        "# Service configuration": None,
        "service_user": service_cfg.get("user", "eventstore"),
        "service_group": service_cfg.get("group", "eventstore"),
        "service_name": service_cfg.get("name", "eventstore"),
        "install_dir": service_cfg.get("install_dir", "/opt/event-store"),
        "docker_compose_dir": ansible_cfg.get("docker_compose_dir", "/opt/eventstore/docker"),
        
        "# Environment variables": None,
        "service_environment": {
            "BACKEND": eventstore_cfg.get("backend", "postgres"),
            "DATABASE_URL": f"postgres://{postgres_cfg.get('user', 'eventstore')}:{postgres_cfg.get('password', 'changeme')}@localhost:{postgres_cfg.get('port', 5432)}/{postgres_cfg.get('db', 'eventstore')}",
            "GRPC_PORT": str(eventstore_cfg.get("grpc_port", 50051)),
            "RUST_LOG": eventstore_cfg.get("rust_log", "info"),
        },
    }
    
    write_yaml(ansible_env_dir / "group_vars" / "all.yml", ansible_vars)
    
    return [
        str(terraform_env_dir / "terraform.tfvars.json"),
        str(ansible_env_dir / "inventory.ini"),
        str(ansible_env_dir / "group_vars" / "all.yml"),
    ]


def main() -> None:
    parser = argparse.ArgumentParser(description="Render IaC configuration for Terraform and Ansible")
    parser.add_argument("--target", required=True, choices=["aws", "proxmox"], help="Deployment target")
    parser.add_argument("--env", required=True, help="Environment name (e.g., prod, local)")
    parser.add_argument(
        "--config",
        help="Path to environment YAML (defaults to infra-as-code/<target>/provision/config/<env>.yml)",
    )
    args = parser.parse_args()

    config_path = (
        Path(args.config)
        if args.config
        else ROOT / args.target / "provision" / "config" / f"{args.env}.yml"
    )

    if not config_path.exists():
        raise SystemExit(f"Configuration file not found: {config_path}")

    cfg = load_yaml(config_path)

    if args.target == "aws":
        outputs = render_aws(args.env, cfg)
    else:
        outputs = render_proxmox(args.env, cfg)

    for output in outputs:
        print(f"rendered: {output}")


if __name__ == "__main__":
    main()
