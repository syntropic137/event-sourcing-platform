"""read_events follows ReadStream pages to the end, against any server (#405).

The contract suite proves this against the real store. These pin the edges a
real current server never shows: a pre-#404 server that sets ``is_end`` only
on an empty page, and a cursor that does not advance (fail, never spin).
"""

from __future__ import annotations

from typing import Literal

import pytest

from event_sourcing.client.grpc_client import GrpcEventStoreClient
from event_sourcing.core.errors import EventStoreError
from event_sourcing.core.event import DomainEvent
from event_sourcing.proto.eventstore.v1 import eventstore_pb2

pytestmark = pytest.mark.unit


class Paged(DomainEvent):
    event_type: Literal["Paged"] = "Paged"


def _event(nonce: int) -> eventstore_pb2.EventData:
    return eventstore_pb2.EventData(
        meta=eventstore_pb2.EventMetadata(
            event_id=f"e{nonce}",
            aggregate_id="a",
            aggregate_type="Paged",
            aggregate_nonce=nonce,
            event_type="Paged",
            event_version=1,
            content_type="application/json",
            global_nonce=nonce,
        ),
        payload=b"{}",
    )


class _PagingStub:
    """ReadStream over ``count`` events, ``page`` at a time."""

    def __init__(self, count: int, page: int, mode: str) -> None:
        self.count, self.page, self.mode = count, page, mode
        self.requests: list[tuple[int, int]] = []

    async def ReadStream(  # noqa: N802 - gRPC method name
        self, request: eventstore_pb2.ReadStreamRequest
    ) -> eventstore_pb2.ReadStreamResponse:
        self.requests.append((request.from_aggregate_nonce, request.max_count))
        start = max(request.from_aggregate_nonce, 1)
        nonces = list(range(start, min(start + self.page, self.count + 1)))
        last = nonces[-1] if nonces else None
        if self.mode == "stuck":
            next_from = start
        else:
            next_from = last + 1 if last is not None else start
        is_end = not nonces if self.mode == "pre404" else (last or 0) >= self.count
        return eventstore_pb2.ReadStreamResponse(
            events=[_event(n) for n in nonces],
            is_end=is_end,
            next_from_aggregate_nonce=next_from,
        )


def _client(stub: _PagingStub) -> GrpcEventStoreClient:
    client = GrpcEventStoreClient()
    client._stub = stub  # type: ignore[assignment]
    return client


@pytest.mark.parametrize("mode", ["current", "pre404"])
async def test_reads_every_page(mode: str) -> None:
    stub = _PagingStub(count=7, page=3, mode=mode)

    events = await _client(stub).read_events("Paged-a")

    assert [e.metadata.aggregate_nonce for e in events] == list(range(1, 8))
    assert [r[0] for r in stub.requests][:3] == [1, 4, 7]


async def test_from_version_starts_the_first_page() -> None:
    stub = _PagingStub(count=7, page=3, mode="current")

    events = await _client(stub).read_events("Paged-a", from_version=5)

    assert [e.metadata.aggregate_nonce for e in events] == [5, 6, 7]


async def test_a_cursor_that_does_not_advance_fails() -> None:
    with pytest.raises(EventStoreError, match="did not advance"):
        await _client(_PagingStub(count=7, page=3, mode="stuck")).read_events("Paged-a")


async def test_stream_exists_reads_one_event() -> None:
    stub = _PagingStub(count=5000, page=5000, mode="current")

    assert await _client(stub).stream_exists("Paged-a")
    assert stub.requests == [(1, 1)]
