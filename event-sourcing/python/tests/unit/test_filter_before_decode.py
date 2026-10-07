"""A subscriber filters by event type before decoding (ADR-027 reading step 5).

An event type that evolves to v2 with no upcaster is undecodable for this
reader. A projection that subscribes only to other types must not be halted
by it: the coordinator skips such events (advancing the checkpoint as for any
skipped type) without decoding them. Only a projection that handles the type
may stop on it, and then it halts as ADR-026 says.

The tests run a real ``GrpcEventStoreClient`` against an in-process gRPC
server, so the decode happens where it does in production.
"""

from __future__ import annotations

import asyncio
import contextlib
import json
from typing import TYPE_CHECKING

import grpc
import pytest

from event_sourcing import DomainEvent, event
from event_sourcing.client.grpc_client import GrpcEventStoreClient
from event_sourcing.core.checkpoint import AutoDispatchProjection
from event_sourcing.core.errors import SubscriptionHaltedError, UnknownEventVersionError
from event_sourcing.core.event import GenericDomainEvent
from event_sourcing.core.upcast import Upcasters
from event_sourcing.proto.eventstore.v1 import eventstore_pb2, eventstore_pb2_grpc
from event_sourcing.stores.memory_checkpoint import MemoryCheckpointStore
from event_sourcing.subscriptions.coordinator import SubscriptionCoordinator

if TYPE_CHECKING:
    from collections.abc import AsyncIterator

pytestmark = pytest.mark.unit

TIMEOUT_S = 5.0


@event("FbdWanted", "v1")
class Wanted(DomainEvent):
    event_type = "FbdWanted"
    n: int


@event("FbdEvolved", "v1")
class EvolvedV1(DomainEvent):
    """Registered at v1 only: a stored v2 is UnknownEventVersionError."""

    event_type = "FbdEvolved"
    n: int


@event("FbdRenamedNew", "v1")
class RenamedNew(DomainEvent):
    event_type = "FbdRenamedNew"
    n: int


def _event(nonce: int, event_type: str, version: int = 1) -> eventstore_pb2.EventData:
    meta = eventstore_pb2.EventMetadata(
        event_id=f"evt-{nonce}",
        aggregate_id="agg-1",
        aggregate_type="Agg",
        aggregate_nonce=nonce,
        global_nonce=nonce,
        event_type=event_type,
        event_version=version,
        content_type="application/json",
    )
    return eventstore_pb2.EventData(meta=meta, payload=json.dumps({"n": nonce}).encode())


#: 2 is an evolved event this reader cannot decode.
HISTORY = [_event(1, "FbdWanted"), _event(2, "FbdEvolved", version=2), _event(3, "FbdWanted")]


class _Store(eventstore_pb2_grpc.EventStoreServicer):
    def __init__(self, events: list[eventstore_pb2.EventData]) -> None:
        self.events = events

    async def ReadAll(  # noqa: N802 - generated gRPC method name
        self,
        request: eventstore_pb2.ReadAllRequest,
        context: grpc.aio.ServicerContext[
            eventstore_pb2.ReadAllRequest, eventstore_pb2.ReadAllResponse
        ],
    ) -> eventstore_pb2.ReadAllResponse:
        # Only the head probe (a backwards read of one event) is used here.
        return eventstore_pb2.ReadAllResponse(events=self.events[-1:], is_end=True)

    async def Subscribe(  # noqa: N802 - generated gRPC method name
        self,
        request: eventstore_pb2.SubscribeRequest,
        context: grpc.aio.ServicerContext[
            eventstore_pb2.SubscribeRequest, eventstore_pb2.SubscribeResponse
        ],
    ) -> AsyncIterator[eventstore_pb2.SubscribeResponse]:
        for data in self.events:
            if data.meta.global_nonce >= request.from_global_nonce:
                yield eventstore_pb2.SubscribeResponse(event=data)
        await asyncio.Event().wait()  # a live subscription stays open


@contextlib.asynccontextmanager
async def _serving(
    events: list[eventstore_pb2.EventData], upcasters: Upcasters | None = None
) -> AsyncIterator[GrpcEventStoreClient]:
    server = grpc.aio.server()
    eventstore_pb2_grpc.add_EventStoreServicer_to_server(_Store(events), server)
    port = server.add_insecure_port("127.0.0.1:0")
    await server.start()
    grpc_client = GrpcEventStoreClient(f"127.0.0.1:{port}", upcasters=upcasters)
    await grpc_client.connect()
    try:
        yield grpc_client
    finally:
        await grpc_client.disconnect()
        await server.stop(None)


@pytest.fixture
async def client() -> AsyncIterator[GrpcEventStoreClient]:
    async with _serving(HISTORY) as grpc_client:
        yield grpc_client


async def _take(client: GrpcEventStoreClient, count: int, **kwargs: frozenset[str]) -> list:
    taken = []
    async with asyncio.timeout(TIMEOUT_S):
        async for envelope in client.subscribe(from_global_nonce=0, **kwargs):
            taken.append(envelope)
            if len(taken) == count:
                break
    return taken


async def test_unfiltered_subscribe_still_raises_on_the_evolved_event(
    client: GrpcEventStoreClient,
) -> None:
    with pytest.raises(UnknownEventVersionError):
        await _take(client, 3)


async def test_a_type_outside_the_filter_is_yielded_undecoded(
    client: GrpcEventStoreClient,
) -> None:
    first, skipped, third = await _take(client, 3, event_types=frozenset({"FbdWanted"}))

    assert isinstance(first.event, Wanted)
    assert isinstance(third.event, Wanted)
    # Position and type only, so a consumer can skip it and move on.
    assert isinstance(skipped.event, GenericDomainEvent)
    assert skipped.metadata.global_nonce == 2
    assert skipped.metadata.event_type == "FbdEvolved"
    assert skipped.metadata.event_version == 2


async def test_a_type_inside_the_filter_still_raises(client: GrpcEventStoreClient) -> None:
    with pytest.raises(UnknownEventVersionError):
        await _take(client, 3, event_types=frozenset({"FbdWanted", "FbdEvolved"}))


RENAMES = Upcasters().rename("FbdRenamedOld", 1, "FbdRenamedNew", 1, lambda body: body)


async def test_the_filter_applies_to_the_type_after_upcasting() -> None:
    async with _serving([_event(1, "FbdRenamedOld")], RENAMES) as client:
        (renamed,) = await _take(client, 1, event_types=frozenset({"FbdRenamedNew"}))
        assert isinstance(renamed.event, RenamedNew)

        (skipped,) = await _take(client, 1, event_types=frozenset({"FbdWanted"}))
        assert isinstance(skipped.event, GenericDomainEvent)
        assert skipped.metadata.event_type == "FbdRenamedNew"
        assert skipped.metadata.stored_event_type == "FbdRenamedOld"


class WantedProjection(AutoDispatchProjection):
    def __init__(self) -> None:
        self.applied: list[int] = []

    def get_name(self) -> str:
        return "wanted"

    def get_version(self) -> int:
        return 1

    async def clear_all_data(self) -> None:
        self.applied.clear()

    async def on_fbd_wanted(self, data: dict[str, int]) -> None:
        self.applied.append(data["n"])


class EvolvedProjection(WantedProjection):
    def get_name(self) -> str:
        return "evolved"

    async def on_fbd_evolved(self, data: dict[str, int]) -> None:
        self.applied.append(data["n"])


async def test_a_projection_not_handling_the_evolved_type_is_not_halted(
    client: GrpcEventStoreClient,
) -> None:
    projection = WantedProjection()
    checkpoints = MemoryCheckpointStore()
    coordinator = SubscriptionCoordinator(
        event_store=client, checkpoint_store=checkpoints, projections=[projection]
    )
    running = asyncio.create_task(coordinator.start())
    try:
        async with asyncio.timeout(TIMEOUT_S):
            while 3 not in projection.applied:
                assert not running.done(), running
                await asyncio.sleep(0.01)
        assert coordinator.halted is None
        assert coordinator.is_healthy
    finally:
        await coordinator.stop()
        running.cancel()
        await asyncio.gather(running, return_exceptions=True)

    assert projection.applied == [1, 3]
    checkpoint = await checkpoints.get_checkpoint("wanted")
    assert checkpoint is not None
    assert checkpoint.global_position == 3


async def test_a_projection_handling_the_evolved_type_halts_there(
    client: GrpcEventStoreClient,
) -> None:
    projection = EvolvedProjection()
    coordinator = SubscriptionCoordinator(
        event_store=client, checkpoint_store=MemoryCheckpointStore(), projections=[projection]
    )
    with pytest.raises(SubscriptionHaltedError) as halted:
        await asyncio.wait_for(coordinator.start(), TIMEOUT_S)
    assert halted.value.global_nonce == 2
    assert projection.applied == [1]


class FailingEvolvedProjection(EvolvedProjection):
    """Handles the evolved type, but fails event 1 every time: held below it for good."""

    def get_name(self) -> str:
        return "failing"

    async def on_fbd_wanted(self, data: dict[str, int]) -> None:
        raise ConnectionError("projection store unavailable")


async def test_a_projection_held_off_the_track_no_longer_widens_its_filter(
    client: GrpcEventStoreClient,
) -> None:
    """Held below 1 and taken off the shared track, its types stop being decoded there."""
    failing, wanted = FailingEvolvedProjection(), WantedProjection()
    coordinator = SubscriptionCoordinator(
        event_store=client,
        checkpoint_store=MemoryCheckpointStore(),
        projections=[failing, wanted],
        replay_concurrency=1,  # both replay on one track
    )
    running = asyncio.create_task(coordinator.start())
    try:
        async with asyncio.timeout(TIMEOUT_S):
            while 3 not in wanted.applied:
                assert not running.done(), running
                await asyncio.sleep(0.01)
        assert coordinator.halted is None
        assert set(coordinator.held_projections) == {"failing"}
    finally:
        await coordinator.stop()
        running.cancel()
        await asyncio.gather(running, return_exceptions=True)

    assert wanted.applied == [1, 3]
