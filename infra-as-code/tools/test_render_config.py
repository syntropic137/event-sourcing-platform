"""Tests for render_config.py IaC renderer."""

from __future__ import annotations

import json
import sys
from pathlib import Path

import pytest
import yaml

# Ensure the tools directory is importable.
sys.path.insert(0, str(Path(__file__).resolve().parent))

import render_config  # noqa: E402


# ---------------------------------------------------------------------------
# Sample configs
# ---------------------------------------------------------------------------

AWS_CONFIG: dict = {
    "aws": {"region": "us-east-1", "profile": "default"},
    "compute": {
        "instance_type": "t3.medium",
        "ami_id": "ami-123",
        "key_pair_name": "mykey",
        "root_volume_gb": 30,
    },
    "metadata": {"environment": "test", "owner": "ci"},
    "postgres": {
        "type": "postgres",
        "database_url_secret_arn": "arn:aws:secretsmanager:us-east-1:123:secret/db",
    },
    "event_store": {
        "version": "0.1.0",
        "grpc_port": 50051,
        "http_port": 8080,
        "metrics_port": 9090,
    },
    "ansible": {
        "binary": {"url": "https://example.com/binary"},
        "service": {"user": "es", "group": "es", "name": "eventstore"},
        "inventory_hostname": "es-1",
        "host": "10.0.0.1",
        "ssh_user": "ubuntu",
        "ssh_private_key_path": "~/.ssh/id_rsa",
        "inventory_host_group": "event_store",
    },
    "network": {
        "vpc_id": "vpc-123",
        "public_subnet_ids": ["sub-1"],
        "ssh_allowed_cidrs": ["0.0.0.0/0"],
    },
    "security": {"iam_instance_profile": "es-profile"},
}

PROXMOX_CONFIG: dict = {
    "proxmox": {
        "endpoint": "https://pve.local:8006",
        "user": "root@pam",
        "token_id": "terraform",
        "token_secret": "secret-value-here",
        "node": "pve",
        "template_name": "ubuntu-cloud",
        "storage_pool": "local-lvm",
        "network_bridge": "vmbr0",
        "ssh_public_key_path": "~/.ssh/id_rsa.pub",
    },
    "metadata": {"environment": "local", "owner": "dev"},
    "network": {
        "ip_address": "192.168.1.100/24",
        "gateway": "192.168.1.1",
    },
    "compute": {
        "vm_name": "eventstore-local",
        "cores": 2,
        "sockets": 1,
        "memory_mb": 4096,
        "disk_gb": 40,
    },
    "ansible": {
        "host": "192.168.1.100",
        "ssh_user": "ubuntu",
        "ssh_private_key_path": "~/.ssh/nuc-proxmox",
        "inventory_host_group": "event_store",
        "postgres": {
            "container_name": "es-postgres",
            "db": "esdb",
            "user": "esuser",
            "password": "s3cret",
            "port": 5432,
            "data_dir": "/data/pg",
        },
        "eventstore": {
            "grpc_port": 50051,
            "backend": "postgres",
            "binary_url": "https://example.com/es-bin",
            "rust_log": "debug",
        },
        "service": {
            "user": "eventstore",
            "group": "eventstore",
            "name": "eventstore",
            "install_dir": "/opt/es",
        },
        "docker_compose_dir": "/opt/es/docker",
    },
}


# ---------------------------------------------------------------------------
# _build_aws_terraform_payload
# ---------------------------------------------------------------------------


class TestBuildAwsTerraformPayload:
    def test_produces_correct_structure(self) -> None:
        result = render_config._build_aws_terraform_payload(AWS_CONFIG)

        assert result["aws_region"] == "us-east-1"
        assert result["aws_profile"] == "default"
        assert result["metadata"]["environment"] == "test"
        assert result["metadata"]["owner"] == "ci"
        assert result["event_store"]["version"] == "0.1.0"
        assert result["event_store"]["grpc_port"] == 50051
        assert result["backend"]["type"] == "postgres"
        assert result["network"]["vpc_id"] == "vpc-123"
        assert result["compute"]["instance_type"] == "t3.medium"
        assert result["compute"]["ami_id"] == "ami-123"
        assert result["security"]["iam_instance_profile"] == "es-profile"

    def test_defaults_for_optional_fields(self) -> None:
        cfg = {
            **AWS_CONFIG,
            "aws": {"region": "eu-west-1"},  # no profile
            "security": {"iam_instance_profile": "p"},
        }
        result = render_config._build_aws_terraform_payload(cfg)

        assert result["aws_profile"] == "default"
        assert result["security"]["allow_public_grpc"] is False
        assert result["security"]["allow_public_dashboard"] is False
        assert result["metadata"]["extra_tags"] == {}
        assert result["compute"]["ssh_user"] == "ubuntu"


# ---------------------------------------------------------------------------
# _resolve_proxmox_token
# ---------------------------------------------------------------------------


class TestResolveProxmoxToken:
    def test_returns_token_from_env(self, monkeypatch: pytest.MonkeyPatch) -> None:
        monkeypatch.setenv("MY_PVE_TOKEN", "env-secret")
        result = render_config._resolve_proxmox_token(
            {"token_secret_env": "MY_PVE_TOKEN"}
        )
        assert result == "env-secret"

    def test_returns_config_value_when_no_env_key(self) -> None:
        result = render_config._resolve_proxmox_token(
            {"token_secret": "cfg-secret"}
        )
        assert result == "cfg-secret"

    def test_returns_empty_when_nothing_set(self) -> None:
        result = render_config._resolve_proxmox_token({})
        assert result == ""

    def test_raises_when_env_var_not_set(
        self, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        monkeypatch.delenv("MISSING_VAR", raising=False)
        with pytest.raises(SystemExit, match="MISSING_VAR"):
            render_config._resolve_proxmox_token(
                {"token_secret_env": "MISSING_VAR"}
            )


# ---------------------------------------------------------------------------
# render_aws (integration)
# ---------------------------------------------------------------------------


class TestRenderAws:
    def test_creates_all_output_files(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        monkeypatch.setattr(render_config, "ROOT", tmp_path)

        outputs = render_config.render_aws("staging", AWS_CONFIG)

        assert len(outputs) == 3

        tfvars_path = tmp_path / "aws" / "provision" / "terraform" / "envs" / "staging" / "terraform.tfvars.json"
        assert tfvars_path.exists()
        tfvars = json.loads(tfvars_path.read_text())
        assert tfvars["aws_region"] == "us-east-1"

        group_vars_path = tmp_path / "aws" / "configure" / "ansible" / "envs" / "staging" / "group_vars" / "all.yml"
        assert group_vars_path.exists()
        gv = yaml.safe_load(group_vars_path.read_text())
        assert gv["binary_url"] == "https://example.com/binary"
        assert gv["service_user"] == "es"

        inventory_path = tmp_path / "aws" / "configure" / "ansible" / "envs" / "staging" / "inventory.ini"
        assert inventory_path.exists()
        inv = inventory_path.read_text()
        assert "[event_store]" in inv
        assert "es-1" in inv
        assert "ansible_host=10.0.0.1" in inv

    def test_playbook_created_when_missing(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        monkeypatch.setattr(render_config, "ROOT", tmp_path)
        render_config.render_aws("dev", AWS_CONFIG)

        playbook = tmp_path / "aws" / "configure" / "ansible" / "envs" / "dev" / "playbook.yml"
        assert playbook.exists()
        content = playbook.read_text()
        assert "hosts: event_store" in content

    def test_playbook_not_overwritten_when_exists(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        monkeypatch.setattr(render_config, "ROOT", tmp_path)

        playbook_dir = tmp_path / "aws" / "configure" / "ansible" / "envs" / "dev"
        playbook_dir.mkdir(parents=True)
        playbook = playbook_dir / "playbook.yml"
        playbook.write_text("# custom playbook\n")

        render_config.render_aws("dev", AWS_CONFIG)

        assert playbook.read_text() == "# custom playbook\n"

    def test_aws_arn_gets_lookup_template(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        monkeypatch.setattr(render_config, "ROOT", tmp_path)
        render_config.render_aws("prod", AWS_CONFIG)

        gv_path = tmp_path / "aws" / "configure" / "ansible" / "envs" / "prod" / "group_vars" / "all.yml"
        gv = yaml.safe_load(gv_path.read_text())
        db_url = gv["service_environment"]["DATABASE_URL"]
        assert db_url.startswith("{{ lookup('aws_secretsmanager'")


# ---------------------------------------------------------------------------
# render_proxmox (integration)
# ---------------------------------------------------------------------------


class TestRenderProxmox:
    def test_creates_all_output_files(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        monkeypatch.setattr(render_config, "ROOT", tmp_path)

        outputs = render_config.render_proxmox("local", PROXMOX_CONFIG)

        assert len(outputs) == 3

        tfvars_path = tmp_path / "proxmox" / "provision" / "terraform" / "envs" / "local" / "terraform.tfvars.json"
        assert tfvars_path.exists()
        tfvars = json.loads(tfvars_path.read_text())
        assert tfvars["proxmox"]["endpoint"] == "https://pve.local:8006"
        assert tfvars["proxmox"]["token_secret"] == "secret-value-here"
        assert tfvars["compute"]["vm_name"] == "eventstore-local"
        assert tfvars["network"]["ip_address"] == "192.168.1.100/24"

        inv_path = tmp_path / "proxmox" / "configure" / "ansible" / "envs" / "local" / "inventory.ini"
        assert inv_path.exists()
        inv = inv_path.read_text()
        assert "[event_store]" in inv
        assert "192.168.1.100" in inv

        gv_path = tmp_path / "proxmox" / "configure" / "ansible" / "envs" / "local" / "group_vars" / "all.yml"
        assert gv_path.exists()
        gv = yaml.safe_load(gv_path.read_text())
        assert gv["postgres_db"] == "esdb"
        assert gv["eventstore_grpc_port"] == 50051

    def test_token_from_env_var(
        self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        monkeypatch.setattr(render_config, "ROOT", tmp_path)
        monkeypatch.setenv("PVE_SECRET", "from-env")

        cfg = {**PROXMOX_CONFIG}
        cfg["proxmox"] = {
            **cfg["proxmox"],
            "token_secret_env": "PVE_SECRET",
        }
        # Remove static token_secret so we confirm env is used
        del cfg["proxmox"]["token_secret"]

        render_config.render_proxmox("env-test", cfg)

        tfvars_path = tmp_path / "proxmox" / "provision" / "terraform" / "envs" / "env-test" / "terraform.tfvars.json"
        tfvars = json.loads(tfvars_path.read_text())
        assert tfvars["proxmox"]["token_secret"] == "from-env"


# ---------------------------------------------------------------------------
# _build_proxmox_terraform_payload
# ---------------------------------------------------------------------------


class TestBuildProxmoxTerraformPayload:
    def test_produces_correct_structure(self) -> None:
        result = render_config._build_proxmox_terraform_payload(
            PROXMOX_CONFIG, "my-token"
        )

        assert result["proxmox"]["token_secret"] == "my-token"
        assert result["proxmox"]["endpoint"] == "https://pve.local:8006"
        assert result["proxmox"]["node"] == "pve"
        assert result["compute"]["cores"] == 2
        assert result["compute"]["memory_mb"] == 4096
        assert result["metadata"]["environment"] == "local"

    def test_defaults_for_optional_fields(self) -> None:
        result = render_config._build_proxmox_terraform_payload(
            PROXMOX_CONFIG, ""
        )

        assert result["proxmox"]["insecure"] is False
        assert result["proxmox"]["template_id"] == 9000
        assert result["proxmox"]["vlan_tag"] == 0
        assert result["proxmox"]["pool"] == ""
        assert result["proxmox"]["ciuser"] == "ubuntu"
        assert result["proxmox"]["clone_timeout"] == 600
        assert result["network"]["dns_servers"] == []


# ---------------------------------------------------------------------------
# _build_proxmox_ansible_config
# ---------------------------------------------------------------------------


class TestBuildProxmoxAnsibleConfig:
    def test_writes_inventory_and_group_vars(self, tmp_path: Path) -> None:
        ansible_dir = tmp_path / "ansible" / "envs" / "local"
        render_config._build_proxmox_ansible_config(PROXMOX_CONFIG, ansible_dir)

        inv = (ansible_dir / "inventory.ini").read_text()
        assert "[event_store]" in inv
        assert "ansible_user=ubuntu" in inv
        assert "ansible_python_interpreter=/usr/bin/python3" in inv

        gv = yaml.safe_load((ansible_dir / "group_vars" / "all.yml").read_text())
        assert gv["postgres_container_name"] == "es-postgres"
        assert gv["postgres_user"] == "esuser"
        assert gv["binary_url"] == "https://example.com/es-bin"
        assert gv["service_environment"]["RUST_LOG"] == "debug"
        assert gv["docker_compose_dir"] == "/opt/es/docker"


# ---------------------------------------------------------------------------
# _build_aws_ansible_config
# ---------------------------------------------------------------------------


class TestBuildAwsAnsibleConfig:
    def test_writes_group_vars_and_inventory(self, tmp_path: Path) -> None:
        ansible_dir = tmp_path / "ansible" / "envs" / "staging"
        render_config._build_aws_ansible_config(AWS_CONFIG, ansible_dir)

        gv = yaml.safe_load(
            (ansible_dir / "group_vars" / "all.yml").read_text()
        )
        assert gv["service_name"] == "eventstore"
        assert gv["service_user"] == "es"
        assert gv["service_environment"]["GRPC_PORT"] == "50051"

        inv = (ansible_dir / "inventory.ini").read_text()
        assert "es-1" in inv
        assert "ansible_host=10.0.0.1" in inv

    def test_non_arn_database_url_passed_verbatim(self, tmp_path: Path) -> None:
        cfg = {**AWS_CONFIG}
        cfg["postgres"] = {
            "type": "postgres",
            "database_url_secret_arn": "postgres://user:pass@host/db",
        }
        ansible_dir = tmp_path / "ansible" / "envs" / "plain"
        render_config._build_aws_ansible_config(cfg, ansible_dir)

        gv = yaml.safe_load(
            (ansible_dir / "group_vars" / "all.yml").read_text()
        )
        assert gv["service_environment"]["DATABASE_URL"] == "postgres://user:pass@host/db"
