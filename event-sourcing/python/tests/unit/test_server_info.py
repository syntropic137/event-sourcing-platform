"""GetServerInfo client helpers (#343).

Behaviour against current and legacy servers is exercised over real gRPC with
in-process servers: a legacy server is simply one that does not register the
GetServerInfo method, so it answers UNIMPLEMENTED exactly as a pre-0.17.0
event store does.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

import grpc
import pytest

from event_sourcing.client.grpc_client import GrpcEventStoreClient
from event_sourcing.client.server_info import (
    LEGACY_SERVER_INFO,
    Capabilities,
    CompatibilityError,
    ServerInfo,
)
from event_sourcing.core.errors import EventStoreError
from event_sourcing.proto.eventstore.v1 import eventstore_pb2, eventstore_pb2_grpc

if TYPE_CHECKING:
    from collections.abc import AsyncIterator

COMMIT_ORDERED = Capabilities.COMMIT_ORDERED_GLOBAL_NONCE


def _reported(version: str, *caps: str) -> ServerInfo:
    return ServerInfo(
        server_version=version, api_version="eventstore.v1", backend="postgres", capabilities=caps
    )


class TestServerInfoHelpers:
    def test_version_at_least(self) -> None:
        info = _reported("0.17.0")
        assert info.version_at_least("0.16.0")
        assert info.version_at_least("0.17.0")
        assert info.version_at_least("v0.17")
        assert info.version_at_least("0.17.0-rc.1")
        assert _reported("0.10.0").version_at_least("0.9.0")
        assert not info.version_at_least("0.17.1")
        assert not info.version_at_least("garbage")
        assert not _reported("0.17.0-rc.1").version_at_least("0.17.0")
        assert not LEGACY_SERVER_INFO.version_at_least("0.0.1")

    def test_prerelease_precedence_follows_semver(self) -> None:
        # SemVer 2.0 section 11 example, ascending.
        ordered = [
            "1.0.0-alpha",
            "1.0.0-alpha.1",
            "1.0.0-alpha.beta",
            "1.0.0-beta",
            "1.0.0-beta.2",
            "1.0.0-beta.11",
            "1.0.0-rc.1",
            "1.0.0",
        ]
        for i, have in enumerate(ordered):
            for j, want in enumerate(ordered):
                assert _reported(have).version_at_least(want) == (i >= j), f"{have} >= {want}"
        assert not _reported("0.17.0-alpha.1").version_at_least("0.17.0-rc.2")
        assert _reported("0.17.0-rc.2").version_at_least("0.17.0-rc.1")
        assert not _reported("0.17.0-rc.1").version_at_least("0.17.0-rc.2")
        assert _reported("0.17.0-rc.10").version_at_least("0.17.0-rc.9")
        assert _reported("0.17.0-rc.2+build.5").version_at_least("0.17.0-rc.2")
        assert not _reported("0.17.0-rc..1").version_at_least("0.0.0")
        assert not _reported("0.17.0-").version_at_least("0.0.0")
        # Numeric identifiers beyond 64 bits still compare numerically.
        assert _reported("1.0.0").version_at_least("1.0.0-rc.18446744073709551616")
        assert _reported("1.0.0-rc.18446744073709551617").version_at_least(
            "1.0.0-rc.18446744073709551616"
        )
        assert not _reported("1.0.0-rc.18446744073709551616").version_at_least(
            "1.0.0-rc.18446744073709551617"
        )
        assert _reported("1.0.0-rc.99999999999999999999").version_at_least("1.0.0-rc.9")
        assert _reported("18446744073709551616.0.0").version_at_least("18446744073709551615.9.9")
        assert not _reported("1.0.0-rc.99999999999999999999").version_at_least("1.0.0-rc.a")

    def test_missing_capabilities(self) -> None:
        info = _reported("0.17.0", COMMIT_ORDERED)
        assert info.missing_capabilities([COMMIT_ORDERED]) == []
        assert info.missing_capabilities(["future_flag", COMMIT_ORDERED]) == ["future_flag"]
        assert info.missing_capabilities("future_flag") == ["future_flag"]
        assert info.missing_capabilities(COMMIT_ORDERED) == []
        assert LEGACY_SERVER_INFO.is_legacy
        assert LEGACY_SERVER_INFO.missing_capabilities([COMMIT_ORDERED]) == [COMMIT_ORDERED]


class _CurrentServicer(eventstore_pb2_grpc.EventStoreServicer):
    async def GetServerInfo(  # noqa: N802 - generated gRPC method name
        self,
        request: eventstore_pb2.GetServerInfoRequest,
        context: grpc.aio.ServicerContext[
            eventstore_pb2.GetServerInfoRequest, eventstore_pb2.GetServerInfoResponse
        ],
    ) -> eventstore_pb2.GetServerInfoResponse:
        return eventstore_pb2.GetServerInfoResponse(
            server_version="0.17.0",
            api_version="eventstore.v1",
            backend="memory",
            capabilities=[COMMIT_ORDERED],
        )


class _UnavailableServicer(eventstore_pb2_grpc.EventStoreServicer):
    async def GetServerInfo(  # noqa: N802 - generated gRPC method name
        self,
        request: eventstore_pb2.GetServerInfoRequest,
        context: grpc.aio.ServicerContext[
            eventstore_pb2.GetServerInfoRequest, eventstore_pb2.GetServerInfoResponse
        ],
    ) -> eventstore_pb2.GetServerInfoResponse:
        await context.abort(grpc.StatusCode.UNAVAILABLE, "down")
        raise AssertionError("unreachable")


async def _client_for(
    servicer: eventstore_pb2_grpc.EventStoreServicer | None,
) -> tuple[GrpcEventStoreClient, grpc.aio.Server]:
    server = grpc.aio.server()
    if servicer is not None:
        eventstore_pb2_grpc.add_EventStoreServicer_to_server(servicer, server)
    port = server.add_insecure_port("127.0.0.1:0")
    await server.start()
    client = GrpcEventStoreClient(address=f"127.0.0.1:{port}")
    await client.connect()
    return client, server


@pytest.fixture
async def current() -> AsyncIterator[GrpcEventStoreClient]:
    client, server = await _client_for(_CurrentServicer())
    yield client
    await client.disconnect()
    await server.stop(None)


@pytest.fixture
async def legacy() -> AsyncIterator[GrpcEventStoreClient]:
    # No EventStore service registered with GetServerInfo: every call to it
    # gets UNIMPLEMENTED, as from a pre-0.17.0 server.
    client, server = await _client_for(None)
    yield client
    await client.disconnect()
    await server.stop(None)


async def test_current_server_meets_floor(current: GrpcEventStoreClient) -> None:
    info = await current.server_info()
    assert not info.is_legacy
    assert info.server_version == "0.17.0"
    assert info.backend == "memory"
    assert info.has_capability(COMMIT_ORDERED)
    assert await current.require_capabilities([COMMIT_ORDERED]) == info
    await current.require_min_version("0.16.0")
    with pytest.raises(CompatibilityError) as exc:
        await current.require_capabilities(["not_real"])
    assert exc.value.missing == ["not_real"]


async def test_legacy_server_lacks_every_capability(legacy: GrpcEventStoreClient) -> None:
    info = await legacy.server_info()
    assert info == LEGACY_SERVER_INFO
    with pytest.raises(CompatibilityError, match="no GetServerInfo") as exc:
        await legacy.require_capabilities([COMMIT_ORDERED])
    assert exc.value.missing == [COMMIT_ORDERED]
    with pytest.raises(CompatibilityError):
        await legacy.require_min_version("0.16.0")
    assert await legacy.require_capabilities([]) == LEGACY_SERVER_INFO


async def test_other_errors_are_not_mistaken_for_legacy() -> None:
    client, server = await _client_for(_UnavailableServicer())
    try:
        with pytest.raises(EventStoreError, match="server info"):
            await client.server_info()
    finally:
        await client.disconnect()
        await server.stop(None)


async def test_requires_connection() -> None:
    with pytest.raises(EventStoreError, match="not connected"):
        await GrpcEventStoreClient().server_info()
