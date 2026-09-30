"""stream_exists must not report an unreachable store as "no such stream".

`GrpcEventStoreClient.stream_exists` used to wrap its read in
`except Exception: return False`. An absent stream is not an error on the wire
(the store answers ReadStream with no events), so the only thing that except
could catch in practice was the store failing to answer - and it turned that
into the same False as "does not exist". Every existence check above it,
`EventStoreRepository.exists` included, then passed while it could not see.

Both halves run over real gRPC: a port nothing listens on for the hazard, and
an in-process server for the control, so a client that raised for everything
could not pass.
"""

from __future__ import annotations

import socket
from typing import TYPE_CHECKING, Literal

import grpc
import pytest

from event_sourcing import AggregateRoot, EventStoreRepository
from event_sourcing.client.grpc_client import GrpcEventStoreClient
from event_sourcing.core.errors import EventStoreError
from event_sourcing.core.event import DomainEvent, EventEnvelope, EventMetadata
from event_sourcing.proto.eventstore.v1 import eventstore_pb2, eventstore_pb2_grpc

if TYPE_CHECKING:
    from collections.abc import AsyncIterator


class Noted(DomainEvent):
    event_type: Literal["Noted"] = "Noted"


class Note(AggregateRoot[Noted]):
    def get_aggregate_type(self) -> str:
        return "Note"


class _OneTenantStore(eventstore_pb2_grpc.EventStoreServicer):
    """Answers Append and ReadStream the way the Rust store does, absent included."""

    def __init__(self) -> None:
        self._streams: dict[str, list[eventstore_pb2.EventData]] = {}

    async def Append(  # noqa: N802 - gRPC method name
        self, request: eventstore_pb2.AppendRequest, context: object
    ) -> eventstore_pb2.AppendResponse:
        stream = self._streams.setdefault(request.aggregate_id, [])
        stream.extend(request.events)
        return eventstore_pb2.AppendResponse(
            last_global_nonce=len(stream), last_aggregate_nonce=len(stream)
        )

    async def ReadStream(  # noqa: N802 - gRPC method name
        self, request: eventstore_pb2.ReadStreamRequest, context: object
    ) -> eventstore_pb2.ReadStreamResponse:
        return eventstore_pb2.ReadStreamResponse(
            events=self._streams.get(request.aggregate_id, []), is_end=True
        )


def _port_nothing_listens_on() -> int:
    """A port that was free a moment ago, so the connection is refused."""
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        port: int = s.getsockname()[1]
    return port


@pytest.fixture
async def unreachable() -> AsyncIterator[GrpcEventStoreClient]:
    client = GrpcEventStoreClient(address=f"127.0.0.1:{_port_nothing_listens_on()}")
    await client.connect()
    yield client
    await client.disconnect()


@pytest.fixture
async def reachable() -> AsyncIterator[GrpcEventStoreClient]:
    server = grpc.aio.server()
    eventstore_pb2_grpc.add_EventStoreServicer_to_server(_OneTenantStore(), server)  # type: ignore[no-untyped-call]
    port = server.add_insecure_port("127.0.0.1:0")
    await server.start()
    client = GrpcEventStoreClient(address=f"127.0.0.1:{port}")
    await client.connect()
    yield client
    await client.disconnect()
    await server.stop(None)


def _envelope(aggregate_id: str) -> EventEnvelope[DomainEvent]:
    return EventEnvelope(
        event=Noted(),
        metadata=EventMetadata(
            aggregate_nonce=1,
            aggregate_id=aggregate_id,
            aggregate_type="Note",
            event_type="Noted",
        ),
    )


@pytest.mark.unit
class TestUnreachableStoreIsNotAnAbsentStream:
    async def test_client_raises_instead_of_answering_false(
        self, unreachable: GrpcEventStoreClient
    ) -> None:
        with pytest.raises(EventStoreError):
            await unreachable.stream_exists("Note-n1")

    async def test_repository_exists_raises_instead_of_answering_false(
        self, unreachable: GrpcEventStoreClient
    ) -> None:
        """The consumer, not the client: this is what an existence check calls."""
        repo = EventStoreRepository(unreachable, Note, "Note")
        with pytest.raises(EventStoreError):
            await repo.exists("n1")


@pytest.mark.unit
class TestReachableStoreStillAnswers:
    """The control. Without it, a stream_exists that raised always would pass."""

    async def test_absent_stream_is_false(self, reachable: GrpcEventStoreClient) -> None:
        repo = EventStoreRepository(reachable, Note, "Note")
        assert await reachable.stream_exists("Note-never-written") is False
        assert await repo.exists("never-written") is False

    async def test_written_stream_is_true(self, reachable: GrpcEventStoreClient) -> None:
        await reachable.append_events("Note-n1", [_envelope("n1")], expected_version=0)
        repo = EventStoreRepository(reachable, Note, "Note")
        assert await reachable.stream_exists("Note-n1") is True
        assert await repo.exists("n1") is True
