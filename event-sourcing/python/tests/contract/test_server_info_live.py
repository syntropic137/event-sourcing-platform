"""GetServerInfo against the live event store built from this commit (#343).

Runs against ``ESP_EVENT_STORE_ADDRESS`` (both backends in CI). Locally an
unset address skips; under CI it fails, matching the contract suite.
"""

from __future__ import annotations

import os
from typing import TYPE_CHECKING

import pytest

from event_sourcing.client.grpc_client import GrpcEventStoreClient
from event_sourcing.client.server_info import Capabilities

if TYPE_CHECKING:
    from collections.abc import AsyncIterator

pytestmark = pytest.mark.integration


@pytest.fixture
async def live() -> AsyncIterator[GrpcEventStoreClient]:
    address = os.getenv("ESP_EVENT_STORE_ADDRESS")
    if not address:
        reason = "ESP_EVENT_STORE_ADDRESS not set: no live event store"
        if os.getenv("CI"):
            pytest.fail(f"{reason} (CI is set, so this is an error, not a skip)")
        pytest.skip(reason)
    client = GrpcEventStoreClient(address=address)
    await client.connect()
    yield client
    await client.disconnect()


async def test_live_server_reports_info_and_commit_order_guarantee(
    live: GrpcEventStoreClient,
) -> None:
    info = await live.require_capabilities([Capabilities.COMMIT_ORDERED_GLOBAL_NONCE])
    assert not info.is_legacy
    assert info.api_version == "eventstore.v1"
    assert info.backend in {"memory", "postgres"}
    assert info.version_at_least("0.17.0")
