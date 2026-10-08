"""One contract for every event store client: the same assertions, both backends.

``MemoryEventStoreClient`` stands in for the ESP server in every consumer's
unit tests, so anything it does differently is a bug those tests certify.
It keyed streams by the whole ``Type-id`` name while the server keys them by
aggregate id alone, and two aggregates sharing an id shared a stream in
production while every unit test passed (#344, syntropic137#1641).

The memory client runs as ``unit``. The gRPC client runs the SAME assertions
as ``integration`` against a live server at ``ESP_EVENT_STORE_ADDRESS``; it is
never replaced by the memory client. Locally an unset address skips it. Under
CI (``CI`` set) an unset address FAILS: a silent skip there would leave the
contract unverified, which is the gap #344 was.
"""

from __future__ import annotations

import os
import sys
import uuid
from typing import TYPE_CHECKING

import pytest

from event_sourcing import AggregateRoot, RepositoryFactory
from event_sourcing.client.grpc_client import GrpcEventStoreClient
from event_sourcing.client.memory import MemoryEventStoreClient
from event_sourcing.core.errors import (
    ConcurrencyConflictError,
    EventStoreError,
    StreamAlreadyExistsError,
)
from event_sourcing.core.event import DomainEvent, EventEnvelope, EventMetadata
from event_sourcing.decorators import event_sourcing_handler

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
            reason = "ESP_EVENT_STORE_ADDRESS not set: no live event store to hold the contract to"
            if os.getenv("CI"):
                pytest.fail(f"{reason} (CI is set, so this is an error, not a skip)")
            pytest.skip(reason)
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


# --- Paging (#405) -----------------------------------------------------------
#
# ReadStream returns pages; a client that reads one page and stops loads a
# long stream truncated, and its aggregate rehydrates with the wrong state and
# a stale version. Past the server's 1000-event page here on purpose.

LONG_STREAM = 2501


async def _append_many(client: EventStoreClient, aid: str, count: int) -> None:
    batch = 500
    for start in range(1, count + 1, batch):
        envelopes = [
            _envelope("Order", aid, nonce) for nonce in range(start, min(start + batch, count + 1))
        ]
        await client.append_events(f"Order-{aid}", envelopes, expected_version=start - 1)


class _Counter(AggregateRoot[_Happened]):
    def __init__(self) -> None:
        super().__init__()
        self.seen = 0

    def get_aggregate_type(self) -> str:
        return "Order"

    def happen(self) -> None:
        self._raise_event(_Happened(value="more"))

    @event_sourcing_handler("Happened")
    def on_happened(self, event: _Happened) -> None:
        self.seen += 1


async def test_stream_past_one_page_reads_whole(client: EventStoreClient) -> None:
    aid = _fresh_id()
    await _append_many(client, aid, LONG_STREAM)

    events = await client.read_events(f"Order-{aid}")
    assert [e.metadata.aggregate_nonce for e in events] == list(range(1, LONG_STREAM + 1))

    tail = await client.read_events(f"Order-{aid}", from_version=1000)
    assert [e.metadata.aggregate_nonce for e in tail] == list(range(1000, LONG_STREAM + 1))
    assert await client.stream_exists(f"Order-{aid}")


async def test_long_aggregate_loads_whole_and_saves_at_its_version(
    client: EventStoreClient,
) -> None:
    aid = _fresh_id()
    await _append_many(client, aid, LONG_STREAM)
    repository = RepositoryFactory(client).create_repository(_Counter, "Order")

    counter = await repository.load(aid)
    assert counter is not None
    assert (counter.seen, counter.version) == (LONG_STREAM, LONG_STREAM)

    counter.happen()
    await repository.save(counter)  # a truncated load fails OCC here
    reloaded = await repository.load(aid)
    assert reloaded is not None
    assert reloaded.version == LONG_STREAM + 1


async def test_read_from_version_is_inclusive(client: EventStoreClient) -> None:
    aid = _fresh_id()
    await _append_many(client, aid, 3)

    tail = await client.read_events(f"Order-{aid}", from_version=2)
    assert [e.metadata.aggregate_nonce for e in tail] == [2, 3]
    assert await client.read_events(f"Order-{_fresh_id()}", from_version=1) == []


async def test_non_consecutive_aggregate_nonces_are_refused(client: EventStoreClient) -> None:
    """A gap would make a from_version read and the OCC head disagree."""
    aid = _fresh_id()
    with pytest.raises(EventStoreError):
        await client.append_events(
            f"Order-{aid}", [_envelope("Order", aid, 10)], expected_version=0
        )
    await _append_many(client, aid, 2)
    with pytest.raises(EventStoreError):
        await client.append_events(f"Order-{aid}", [_envelope("Order", aid, 4)], expected_version=2)
    assert len(await client.read_events(f"Order-{aid}")) == 2


async def test_stream_name_sets_aggregate_id_and_type(client: EventStoreClient) -> None:
    """The store keys and labels an event by its stream, not the envelope's claim."""
    aid = _fresh_id()
    await client.append_events(f"Order-{aid}", [_envelope("Invoice", "other", 1)], 0)

    [event] = await client.read_events(f"Order-{aid}")
    assert (event.metadata.aggregate_id, event.metadata.aggregate_type) == (aid, "Order")


async def test_omitted_expected_version_means_new_stream(client: EventStoreClient) -> None:
    aid = _fresh_id()
    await client.append_events(f"Order-{aid}", [_envelope("Order", aid, 1)])

    with pytest.raises(StreamAlreadyExistsError):
        await client.append_events(f"Order-{aid}", [_envelope("Order", aid, 2)])


async def test_store_assigns_global_nonces(client: EventStoreClient) -> None:
    """A caller-supplied global nonce is ignored: the store numbers events from 1."""
    first, second = _fresh_id(), _fresh_id()
    claimed = _envelope("Order", first, 1)
    claimed = EventEnvelope(
        event=claimed.event, metadata=claimed.metadata.model_copy(update={"global_nonce": 0})
    )
    await client.append_events(f"Order-{first}", [claimed], expected_version=0)
    await client.append_events(
        f"Order-{second}",
        [_envelope("Order", second, 1), _envelope("Order", second, 2)],
        expected_version=0,
    )

    events, _, _ = await client.read_all(from_global_nonce=0, max_count=10)
    nonces = [e.metadata.global_nonce for e in events]
    assert len(nonces) == 3
    assert all(n is not None and n >= 1 for n in nonces)
    assert nonces == sorted(set(nonces))  # unique, ascending


@pytest.mark.parametrize("forward", [True, False])
async def test_read_all_size_one_pages_visit_every_event_once(
    client: EventStoreClient, forward: bool
) -> None:
    for _ in range(3):
        await _append_many(client, _fresh_id(), 2)

    cursor = 0 if forward else sys.maxsize
    seen: list[int | None] = []
    for _ in range(20):  # 6 events; a cursor that never ends fails here
        page, is_end, cursor = await client.read_all(
            from_global_nonce=cursor, max_count=1, forward=forward
        )
        seen.extend(e.metadata.global_nonce for e in page)
        if is_end:
            break
    else:
        pytest.fail("read_all never reported is_end")

    assert len(seen) == 6
    assert seen == sorted(set(seen), reverse=not forward)  # no repeats, in order
