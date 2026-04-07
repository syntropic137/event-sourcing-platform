"""Tests for GrpcEventStoreClient — connection idempotency.

Regression test for a bug where connect() unconditionally replaced the gRPC
channel and stub, silently killing any active Subscribe stream held by the
subscription coordinator.
"""

from unittest.mock import AsyncMock, patch

import pytest

from event_sourcing.client.grpc_client import GrpcEventStoreClient


@pytest.fixture
def client() -> GrpcEventStoreClient:
    return GrpcEventStoreClient(address="localhost:50051")


class TestConnectIdempotency:
    """connect() must be idempotent — calling it twice must not replace the channel."""

    @pytest.mark.asyncio
    async def test_second_connect_is_noop(self, client: GrpcEventStoreClient) -> None:
        """REGRESSION: repeated connect() must not replace channel/stub."""
        with patch("event_sourcing.client.grpc_client.grpc.aio.insecure_channel") as mock_channel:
            mock_channel.return_value = AsyncMock()

            await client.connect()
            first_channel = client._channel
            first_stub = client._stub

            await client.connect()

            assert client._channel is first_channel
            assert client._stub is first_stub
            # insecure_channel should only be called once
            mock_channel.assert_called_once()

    @pytest.mark.asyncio
    async def test_connect_after_disconnect_reconnects(self, client: GrpcEventStoreClient) -> None:
        """After disconnect(), connect() should create a new channel."""
        with patch("event_sourcing.client.grpc_client.grpc.aio.insecure_channel") as mock_channel:
            mock_ch = AsyncMock()
            mock_channel.return_value = mock_ch

            await client.connect()

            await client.disconnect()
            assert client._channel is None

            await client.connect()
            assert client._channel is not None
            assert mock_channel.call_count == 2
