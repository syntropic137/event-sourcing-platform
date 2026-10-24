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


def _open_private(path: Path):
    """Open `path` for writing as owner-only (0600).

    Rendered artifacts carry secrets (Postgres/gateway passwords, Proxmox
    token), so they must not inherit a umask-022 world-readable mode. chmod
    also covers files that already existed with a looser mode.
    """
    path.parent.mkdir(parents=True, exist_ok=True)
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    os.fchmod(fd, 0o600)
    return os.fdopen(fd, "w", encoding="utf-8")


def write_json(path: Path, data: dict) -> None:
    with _open_private(path) as handle:
        json.dump(data, handle, indent=2)
        handle.write("\n")


def write_text(path: Path, content: str) -> None:
    with _open_private(path) as handle:
        handle.write(content)


def write_yaml(path: Path, data: dict) -> None:
    with _open_private(path) as handle:
        yaml.safe_dump(data, handle, sort_keys=False)


def _stringify_map(values: dict) -> dict:
    return {key: str(value) for key, value in values.items()}


_GATEWAY_TLS_KEYS = {
    "enabled": "esp_gateway_tls",
    "dir": "esp_gateway_tls_dir",
    "cert_src": "esp_gateway_tls_cert_src",
    "key_src": "esp_gateway_tls_key_src",
    "self_signed": "esp_gateway_tls_self_signed",
    "names": "esp_gateway_tls_names",
    "ca_fetch_dest": "esp_gateway_tls_ca_fetch_dest",
    "reload_interval": "esp_gateway_tls_reload_interval",
}


def _gateway_tls_vars(tls_cfg: dict | None) -> dict:
    """Map a `gateway.tls` config block to event-store role vars (#301).

    TLS is on unless `enabled: false` is explicit; unset keys fall back to the
    role defaults. Unknown keys and a half-specified cert/key pair fail
    rendering instead of deploying something unexpected.
    """
    tls_cfg = tls_cfg or {}
    unknown = set(tls_cfg) - set(_GATEWAY_TLS_KEYS)
    if unknown:
        raise SystemExit(f"config error: unknown gateway.tls keys: {sorted(unknown)}")
    if bool(tls_cfg.get("cert_src")) != bool(tls_cfg.get("key_src")):
        raise SystemExit(
            "config error: gateway.tls.cert_src and gateway.tls.key_src must be set together"
        )
    out = {"esp_gateway_tls": bool(tls_cfg.get("enabled", True))}
    for key, var in _GATEWAY_TLS_KEYS.items():
        if key != "enabled" and tls_cfg.get(key) not in (None, ""):
            out[var] = tls_cfg[key]
    return out


def _build_aws_terraform_payload(cfg: dict) -> dict:
    """Build the Terraform tfvars payload for AWS."""
    provider_cfg = cfg["aws"]
    compute_cfg = cfg["compute"]
    metadata_cfg = cfg["metadata"]
    postgres_cfg = cfg["postgres"]
    event_cfg = cfg["event_store"]

    return {
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
            "type": postgres_cfg.get("type", "postgres"),
            "database_url": postgres_cfg.get("database_url_secret_arn", ""),
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


def _build_aws_ansible_config(cfg: dict, ansible_env_dir: Path) -> None:
    """Generate Ansible group_vars, inventory, and playbook for AWS."""
    metadata_cfg = cfg["metadata"]
    postgres_cfg = cfg["postgres"]
    event_cfg = cfg["event_store"]
    ansible_cfg = cfg["ansible"]
    gateway_cfg = cfg.get("gateway", {})

    backend_type = postgres_cfg.get("type", "postgres")
    backend_url = postgres_cfg.get("database_url_secret_arn", "")
    database_lookup = backend_url
    if backend_url.startswith("arn:"):
        database_lookup = "{{ lookup('aws_secretsmanager', '%s') }}" % backend_url

    # Gateway credentials (ADR-024) - eventstore-bin has no auth of its own;
    # the nginx gateway enforces Basic Auth using this credential, and it is
    # the only component this deployment publishes to the network. A missing
    # secret_arn must fail rendering, not silently fall back to the role's
    # default "changeme" password on a network-exposed service.
    gateway_secret_arn = gateway_cfg.get("secret_arn", "")
    if not gateway_secret_arn:
        raise SystemExit(
            "config error: 'gateway.secret_arn' is required (ADR-024) - "
            "the gateway is the only publicly reachable port in this "
            "deployment and must not fall back to a default password"
        )
    gateway_password_lookup = "{{ lookup('aws_secretsmanager', '%s') }}" % gateway_secret_arn

    service_environment = {
        "BACKEND": backend_type,
        "DATABASE_URL": database_lookup,
        "GRPC_PORT": str(event_cfg["grpc_port"]),
        "HTTP_PORT": str(event_cfg["http_port"]),
        "METRICS_PORT": str(event_cfg["metrics_port"]),
        "ENVIRONMENT": metadata_cfg["environment"],
    }

    environment_overrides: dict[str, str] = {}
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
        "esp_gateway_user": gateway_cfg.get("user", "admin"),
        "esp_gateway_password": gateway_password_lookup,
        **_gateway_tls_vars(gateway_cfg.get("tls")),
    }
    write_yaml(ansible_env_dir / "group_vars" / "all.yml", group_vars_payload)

    inventory_line = (
        f"{ansible_cfg['inventory_hostname']} "
        f"ansible_host={ansible_cfg['host']} "
        f"ansible_user={ansible_cfg['ssh_user']} "
        f"ansible_ssh_private_key_file={ansible_cfg['ssh_private_key_path']}"
    )
    write_text(
        ansible_env_dir / "inventory.ini",
        f"[{ansible_cfg['inventory_host_group']}]\n{inventory_line}\n",
    )

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
            "    - role: ../../../../../shared/configure/ansible/roles/event-store\n"
        ).format(host_group=ansible_cfg["inventory_host_group"])
        write_text(playbook_path, playbook_content)


def render_aws(env: str, cfg: dict) -> list[str]:
    terraform_env_dir = ROOT / "aws" / "provision" / "terraform" / "envs" / env
    ansible_env_dir = ROOT / "aws" / "configure" / "ansible" / "envs" / env

    write_json(terraform_env_dir / "terraform.tfvars.json", _build_aws_terraform_payload(cfg))
    _build_aws_ansible_config(cfg, ansible_env_dir)

    return [
        str(terraform_env_dir / "terraform.tfvars.json"),
        str(ansible_env_dir / "group_vars" / "all.yml"),
        str(ansible_env_dir / "inventory.ini"),
    ]


def _resolve_proxmox_token(proxmox_cfg: dict) -> str:
    """Resolve the Proxmox API token secret from env or config."""
    token_secret_env = proxmox_cfg.get("token_secret_env")
    if token_secret_env:
        token_secret = os.environ.get(token_secret_env)
        if not token_secret:
            raise SystemExit(
                f"Environment variable '{token_secret_env}' (referenced in config) is not set"
            )
        return token_secret
    return proxmox_cfg.get("token_secret", "")


def _build_proxmox_terraform_payload(cfg: dict, token_secret: str) -> dict:
    """Build the Terraform tfvars payload for Proxmox."""
    proxmox_cfg = cfg["proxmox"]
    metadata_cfg = cfg["metadata"]

    return {
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


def _build_proxmox_ansible_config(cfg: dict, ansible_env_dir: Path) -> None:
    """Generate Ansible inventory and group_vars for Proxmox."""
    ansible_cfg = cfg.get("ansible", {})

    inventory_content = f"""[{ansible_cfg.get('inventory_host_group', 'event_store')}]
{ansible_cfg.get('host', '192.168.0.100')} ansible_user={ansible_cfg.get('ssh_user', 'ubuntu')} ansible_ssh_private_key_file={ansible_cfg.get('ssh_private_key_path', '~/.ssh/nuc-proxmox')}

[{ansible_cfg.get('inventory_host_group', 'event_store')}:vars]
ansible_python_interpreter=/usr/bin/python3
"""
    write_text(ansible_env_dir / "inventory.ini", inventory_content)

    postgres_cfg = ansible_cfg.get("postgres", {})
    eventstore_cfg = ansible_cfg.get("eventstore", {})
    gateway_cfg = ansible_cfg.get("gateway", {})
    service_cfg = ansible_cfg.get("service", {})

    # Gateway credentials (ADR-024) - eventstore-bin has no auth of its own;
    # the gateway is the only component this deployment publishes to the
    # network. A missing/default password must fail rendering, not silently
    # deploy a network-exposed service with a known credential.
    gateway_password = gateway_cfg.get("password", "")
    if not gateway_password or gateway_password == "changeme":
        raise SystemExit(
            "config error: 'ansible.gateway.password' is missing or left as "
            "the default 'changeme' (ADR-024) - the gateway is the only "
            "publicly reachable port in this deployment and must not use a "
            "predictable credential. Set ESP_GATEWAY_PASSWORD in .env and "
            "regenerate via generate-config.sh."
        )

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
        "# Gateway configuration (ADR-024)": None,
        "esp_gateway_user": gateway_cfg.get("user", "admin"),
        "esp_gateway_password": gateway_password,
        **_gateway_tls_vars(gateway_cfg.get("tls")),
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


def render_proxmox(env: str, cfg: dict) -> list[str]:
    token_secret = _resolve_proxmox_token(cfg["proxmox"])

    terraform_env_dir = ROOT / "proxmox" / "provision" / "terraform" / "envs" / env
    ansible_env_dir = ROOT / "proxmox" / "configure" / "ansible" / "envs" / env

    write_json(
        terraform_env_dir / "terraform.tfvars.json",
        _build_proxmox_terraform_payload(cfg, token_secret),
    )
    _build_proxmox_ansible_config(cfg, ansible_env_dir)

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
