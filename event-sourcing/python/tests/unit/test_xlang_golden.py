"""Golden cross-language fixtures (ADR-027), decoded by the Python SDK.

``event-sourcing/rust/tests/fixtures/xlang/*.json`` hold the protobuf
``AppendRequest`` bytes written by the real TypeScript, Python and Rust
encoders, plus ``typescript-legacy.json`` (TypeScript SDK 0.17, payloads echo
``eventType``/``schemaVersion``). The Rust golden tests check that the
encoders agree byte for byte; this checks that Python's reader decodes every
one of them into its strict (``extra="forbid"``) models. Regenerate with
``make -C event-sourcing/rust test-xlang-fixtures``.
"""

import base64
import json
from datetime import UTC, datetime
from pathlib import Path
from typing import Any, ClassVar

import pytest

from event_sourcing import DomainEvent, UnknownEventVersionError, Upcasters, event
from event_sourcing.client.grpc_client import GrpcEventStoreClient
from event_sourcing.core.event import EventEnvelope, EventMetadata
from event_sourcing.decorators import events as registry
from event_sourcing.proto.eventstore.v1 import eventstore_pb2

FIXTURES = Path(__file__).resolve().parents[3] / "rust" / "tests" / "fixtures" / "xlang"
PRODUCERS = ["typescript", "python", "rust", "typescript-legacy"]
NOTE = 'café ☕ "quoted"'


# Fixture domain (mirrors event-sourcing/rust/tests/common/xlang.rs).
@event("AccountOpened", "v1")
class AccountOpened(DomainEvent):
    event_type: ClassVar[str] = "AccountOpened"
    account_id: str
    owner: str


@event("MoneyDeposited", "v1")
class MoneyDeposited(DomainEvent):
    event_type: ClassVar[str] = "MoneyDeposited"
    amount: int
    note: str


@event("AccountClosed", "v2")
class AccountClosed(DomainEvent):
    event_type: ClassVar[str] = "AccountClosed"
    schema_version: ClassVar[int] = 2
    reason: str
    tags: list[str]


def load(producer: str) -> tuple[dict[str, Any], eventstore_pb2.AppendRequest]:
    fixture = json.loads((FIXTURES / f"{producer}.json").read_text(encoding="utf-8"))
    assert fixture["producer"] == producer
    request = eventstore_pb2.AppendRequest()
    request.ParseFromString(base64.b64decode(fixture["append_request_base64"]))
    return fixture, request


def expected(aggregate_id: str) -> list[DomainEvent]:
    return [
        AccountOpened(account_id=aggregate_id, owner="alice"),
        MoneyDeposited(amount=125, note=NOTE),
        AccountClosed(reason="done", tags=["a", "b"]),
    ]


@pytest.mark.parametrize("producer", PRODUCERS)
def test_python_decodes_fixture_into_strict_models(producer: str) -> None:
    _, request = load(producer)
    client = GrpcEventStoreClient(tenant_id=request.tenant_id)
    envelopes = [client._proto_to_envelope(e) for e in request.events]
    assert [e.event for e in envelopes] == expected(request.aggregate_id)
    assert [type(e.event) for e in envelopes] == [AccountOpened, MoneyDeposited, AccountClosed]
    for env, data in zip(envelopes, request.events, strict=True):
        m = env.metadata
        assert m.stored_event_type == m.event_type == data.meta.event_type
        assert m.stored_event_version == m.event_version == data.meta.event_version
        assert m.aggregate_type == "Account"


@pytest.mark.parametrize("producer", PRODUCERS)
def test_python_upcasts_fixture_v1_to_v2(producer: str) -> None:
    class MoneyDepositedV2(DomainEvent):
        event_type: ClassVar[str] = "MoneyDeposited"
        schema_version: ClassVar[int] = 2
        amount: int
        note: str
        currency: str

    _, request = load(producer)
    deposit = request.events[1]
    up = Upcasters().register("MoneyDeposited", 1, 2, lambda b: {**b, "currency": "EUR"})
    saved = dict(registry._EVENT_VERSION_REGISTRY["MoneyDeposited"])
    try:
        # Only v2 is known: the stored v1 needs the upcaster.
        registry._EVENT_VERSION_REGISTRY["MoneyDeposited"] = {2: MoneyDepositedV2}
        env = GrpcEventStoreClient(upcasters=up)._proto_to_envelope(deposit)
        assert env.event == MoneyDepositedV2(amount=125, note=NOTE, currency="EUR")
        assert env.metadata.stored_event_version == 1
        with pytest.raises(UnknownEventVersionError):
            GrpcEventStoreClient()._proto_to_envelope(deposit)
    finally:
        registry._EVENT_VERSION_REGISTRY["MoneyDeposited"] = saved


def test_python_fixture_matches_the_python_encoder() -> None:
    """python.json is what the Python client writes today (ids/clock pinned)."""
    fixture, request = load("python")
    client = GrpcEventStoreClient(tenant_id=request.tenant_id)
    for i, (ev, stored) in enumerate(
        zip(expected(request.aggregate_id), request.events, strict=True)
    ):
        env = EventEnvelope(
            event=ev,
            metadata=EventMetadata(
                event_id=stored.meta.event_id,
                timestamp=datetime.fromtimestamp(stored.meta.timestamp_unix_ms / 1000, tz=UTC),
                aggregate_id=request.aggregate_id,
                aggregate_type="Account",
                aggregate_nonce=i + 1,
            ),
        )
        ours = client._envelope_to_proto(env, request.aggregate_id, "Account")
        assert ours.SerializeToString(deterministic=True) == stored.SerializeToString(
            deterministic=True
        ), f"python.json is stale; run make -C event-sourcing/rust test-xlang-fixtures ({i})"
    assert fixture["payloads"] == [e.payload.decode() for e in request.events]
