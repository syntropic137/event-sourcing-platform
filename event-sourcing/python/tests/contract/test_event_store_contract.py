"""One contract for every event store client: the same assertions, both backends.

``MemoryEventStoreClient`` stands in for the ESP server in every consumer's
unit tests, so anything it does differently is a bug those tests certify.
It keyed streams by the whole ``Type-id`` name while the server keys them by
aggregate id alone, and two aggregates sharing an id shared a stream in
production while every unit test passed (#344, syntropic137#1641).

The memory client runs as ``unit``. The gRPC client runs the SAME assertions
as ``integration`` against a live server at ``ESP_EVENT_STORE_ADDRESS``; it is
skipped when that is unset, never replaced by the memory client.
"""

from __future__ import annotations

import os
import uuid
from typing import TYPE_CHECKING

import pytest

from event_sourcing.client.grpc_client import GrpcEventStoreClient
from event_sourcing.client.memory import MemoryEventStoreClient
from event_sourcing.core.errors import ConcurrencyConflictError, StreamAlreadyExistsError
from event_sourcing.core.event import DomainEvent, EventEnvelope, EventMetadata

if TYPE_CHECKING:
    from collections.abc import AsyncIterator

EventStoreClient = MemoryEventStoreClient | GrpcEventStoreClient


class _Happened(DomainEvent):
    event_type = "Happened"
    value: str = "x"


def _envelope(aggregate_type: str, aggregate_id: str, nonce: int) -> EventEnvelope[DomainEvent]:
    return EventEnvelope(
        event=_Happened(value=f"{aggregate_type}-{nonce}"),
        metadata=EventMetadata(
            event_id=str(uuid.uuid4()),
            aggregate_id=aggregate_id,
            aggregate_type=aggregate_type,
            aggregate_nonce=nonce,
            event_type="Happened",
        ),
    )


@pytest.fixture(
    params=[
        pytest.param("memory", marks=pytest.mark.unit),
        pytest.param("grpc", marks=pytest.mark.integration),
    ]
)
async def client(request: pytest.FixtureRequest) -> AsyncIterator[EventStoreClient]:
    if request.param == "memory":
        os.environ.setdefault("APP_ENVIRONMENT", "test")
        store: EventStoreClient = MemoryEventStoreClient()
    else:
        address = os.getenv("ESP_EVENT_STORE_ADDRESS")
        if not address:
            pytest.skip(
                "ESP_EVENT_STORE_ADDRESS not set: no live event store to hold the contract to"
            )
        # A fresh tenant per test: the server keeps streams across runs.
        store = GrpcEventStoreClient(address=address, tenant_id=f"contract-{uuid.uuid4().hex}")
    await store.connect()
    yield store
    await store.disconnect()


def _fresh_id() -> str:
    return uuid.uuid4().hex


async def test_appended_events_read_back_in_order(client: EventStoreClient) -> None:
    aid = _fresh_id()
    await client.append_events(f"Order-{aid}", [_envelope("Order", aid, 1)], expected_version=0)
    await client.append_events(f"Order-{aid}", [_envelope("Order", aid, 2)], expected_version=1)

    events = await client.read_events(f"Order-{aid}")

    assert [e.metadata.aggregate_nonce for e in events] == [1, 2]
    assert await client.stream_exists(f"Order-{aid}")


async def test_new_stream_twice_is_refused(client: EventStoreClient) -> None:
    aid = _fresh_id()
    await client.append_events(f"Order-{aid}", [_envelope("Order", aid, 1)], expected_version=0)

    with pytest.raises(StreamAlreadyExistsError):
        await client.append_events(f"Order-{aid}", [_envelope("Order", aid, 1)], expected_version=0)


async def test_stale_expected_version_is_refused(client: EventStoreClient) -> None:
    aid = _fresh_id()
    await client.append_events(f"Order-{aid}", [_envelope("Order", aid, 1)], expected_version=0)
    await client.append_events(f"Order-{aid}", [_envelope("Order", aid, 2)], expected_version=1)

    with pytest.raises(ConcurrencyConflictError):
        await client.append_events(f"Order-{aid}", [_envelope("Order", aid, 2)], expected_version=1)


async def test_two_types_sharing_an_id_share_one_stream(client: EventStoreClient) -> None:
    """The keyspace property behind #344: the type is not part of the key."""
    aid = _fresh_id()
    await client.append_events(f"Order-{aid}", [_envelope("Order", aid, 1)], expected_version=0)

    assert await client.stream_exists(f"Invoice-{aid}")
    with pytest.raises(StreamAlreadyExistsError):
        await client.append_events(
            f"Invoice-{aid}", [_envelope("Invoice", aid, 1)], expected_version=0
        )
    assert [e.metadata.aggregate_type for e in await client.read_events(f"Invoice-{aid}")] == [
        "Order"
    ]


async def test_unknown_stream_reads_empty(client: EventStoreClient) -> None:
    aid = _fresh_id()

    assert await client.read_events(f"Order-{aid}") == []
    assert not await client.stream_exists(f"Order-{aid}")
