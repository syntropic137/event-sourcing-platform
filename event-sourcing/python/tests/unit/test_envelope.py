"""Cross-language envelope (ADR-027): write the real version and a clean
payload; read by (event_type, event_version) with upcasting; typed errors."""

import json
from typing import Any, ClassVar

import pytest

from event_sourcing import (
    DomainEvent,
    EventEnvelope,
    EventMetadata,
    EventPayloadError,
    GenericDomainEvent,
    UndecodableEventError,
    UnknownEventTypeError,
    UnknownEventVersionError,
    UnsupportedContentTypeError,
    UpcastError,
    Upcasters,
    decode_event,
    event,
    registered_event_versions,
    resolve_event_class,
    resolve_event_type,
)
from event_sourcing.client.grpc_client import GrpcEventStoreClient
from event_sourcing.core.errors import EventStoreError
from event_sourcing.proto.eventstore.v1 import eventstore_pb2


@event("EnvDeposited", "v1")
class EnvDepositedV1(DomainEvent):
    event_type: ClassVar[str] = "EnvDeposited"
    amount: int


@event("EnvDeposited", "v2")
class EnvDepositedV2(DomainEvent):
    event_type: ClassVar[str] = "EnvDeposited"
    schema_version: ClassVar[int] = 2
    amount: int
    currency: str


@event("EnvClosed", "v3")
class EnvClosedV3(DomainEvent):
    event_type: ClassVar[str] = "EnvClosed"
    schema_version: ClassVar[int] = 3
    reason: str


# Decorator version string is metadata only; schema_version is the wire version.
@event("EnvDecoratorOnly", "v7")
class EnvDecoratorOnly(DomainEvent):
    event_type: ClassVar[str] = "EnvDecoratorOnly"
    value: str


def data(
    event_type: str,
    payload: dict[str, Any] | bytes,
    *,
    version: int = 1,
    content_type: str = "application/json",
    global_nonce: int = 42,
) -> eventstore_pb2.EventData:
    raw = payload if isinstance(payload, bytes) else json.dumps(payload).encode()
    return eventstore_pb2.EventData(
        meta=eventstore_pb2.EventMetadata(
            event_id="00000000-0000-4000-8000-000000000001",
            aggregate_id="a-1",
            aggregate_type="Account",
            aggregate_nonce=1,
            event_type=event_type,
            event_version=version,
            content_type=content_type,
            global_nonce=global_nonce,
            tenant_id="t",
        ),
        payload=raw,
    )


def encode(ev: DomainEvent) -> eventstore_pb2.EventData:
    client = GrpcEventStoreClient(tenant_id="t")
    env = EventEnvelope(
        event=ev,
        metadata=EventMetadata(aggregate_id="a-1", aggregate_type="Account", aggregate_nonce=1),
    )
    return client._envelope_to_proto(env, "a-1", "Account")


def decode(
    d: eventstore_pb2.EventData, upcasters: Upcasters | None = None
) -> EventEnvelope[DomainEvent]:
    return GrpcEventStoreClient(tenant_id="t", upcasters=upcasters)._proto_to_envelope(d)


class TestRegistry:
    def test_every_version_is_registered(self) -> None:
        assert registered_event_versions("EnvDeposited") == (1, 2)
        assert resolve_event_class("EnvDeposited", 1) is EnvDepositedV1
        assert resolve_event_class("EnvDeposited", 2) is EnvDepositedV2
        # The single-class lookup returns the highest version.
        assert resolve_event_type("EnvDeposited") is EnvDepositedV2

    def test_registry_key_is_schema_version_not_decorator_string(self) -> None:
        assert registered_event_versions("EnvDecoratorOnly") == (1,)

    def test_invalid_schema_version_is_rejected_at_decoration(self) -> None:
        with pytest.raises(ValueError, match="schema_version"):

            @event("EnvBadVersion", "v1")
            class EnvBadVersion(DomainEvent):  # pyright: ignore[reportUnusedClass]
                event_type: ClassVar[str] = "EnvBadVersion"
                schema_version: ClassVar[int] = 0


class TestWrite:
    def test_writes_schema_version(self) -> None:
        d = encode(EnvClosedV3(reason="done"))
        assert d.meta.event_type == "EnvClosed"
        assert d.meta.event_version == 3
        assert d.meta.content_type == "application/json"

    def test_default_schema_version_is_one(self) -> None:
        assert encode(EnvDecoratorOnly(value="x")).meta.event_version == 1

    def test_payload_holds_only_event_fields(self) -> None:
        d = encode(EnvDepositedV2(amount=5, currency="EUR"))
        assert json.loads(d.payload) == {"amount": 5, "currency": "EUR"}

    def test_generic_event_type_is_not_written_into_the_payload(self) -> None:
        generic = GenericDomainEvent.model_validate({"event_type": "Unregistered", "x": 1})
        d = encode(generic)
        assert d.meta.event_type == "Unregistered"
        assert json.loads(d.payload) == {"x": 1}

    def test_invalid_schema_version_is_refused_on_write(self) -> None:
        class Broken(DomainEvent):
            event_type: ClassVar[str] = "Broken"
            schema_version: ClassVar[int] = 0

        with pytest.raises(EventStoreError, match="schema_version"):
            encode(Broken())


class TestReadByVersion:
    def test_each_version_decodes_as_its_own_class(self) -> None:
        v1 = decode(data("EnvDeposited", {"amount": 5}, version=1))
        v2 = decode(data("EnvDeposited", {"amount": 5, "currency": "USD"}, version=2))
        assert type(v1.event) is EnvDepositedV1
        assert type(v2.event) is EnvDepositedV2

    def test_version_zero_reads_as_one(self) -> None:
        env = decode(data("EnvDeposited", {"amount": 5}, version=0))
        assert type(env.event) is EnvDepositedV1
        assert env.metadata.stored_event_version == 1

    def test_unknown_version_is_a_typed_error(self) -> None:
        with pytest.raises(UnknownEventVersionError) as exc:
            decode(data("EnvClosed", {"reason": "done"}, version=1))
        err = exc.value
        assert (err.event_type, err.event_version, err.global_nonce) == ("EnvClosed", 1, 42)
        # A coordinator halts on it instead of retrying (ADR-026).
        assert isinstance(err, UndecodableEventError)

    def test_newer_version_than_registered_is_a_typed_error(self) -> None:
        with pytest.raises(UnknownEventVersionError):
            decode(data("EnvDeposited", {"amount": 5}, version=9))

    def test_metadata_exposes_stored_and_decoded_version(self) -> None:
        env = decode(data("EnvDeposited", {"amount": 5, "currency": "USD"}, version=2))
        m = env.metadata
        assert (m.event_type, m.event_version) == ("EnvDeposited", 2)
        assert (m.stored_event_type, m.stored_event_version) == ("EnvDeposited", 2)
        assert m.content_type == "application/json"
        assert m.tenant_id == "t"


class TestUpcasting:
    up = Upcasters().register("EnvDeposited", 1, 2, lambda b: {**b, "currency": "EUR"})

    def test_upcaster_runs_before_decoding(self) -> None:
        # Only v2 is wanted: decode a stored v1 as v2 through the upcaster.
        env = decode(data("EnvDeposited", {"amount": 5}, version=1), upcasters=self.up)
        assert env.event == EnvDepositedV2(amount=5, currency="EUR")
        m = env.metadata
        assert (m.event_type, m.event_version) == ("EnvDeposited", 2)
        assert (m.stored_event_type, m.stored_event_version) == ("EnvDeposited", 1)

    def test_rename_dispatches_on_the_new_type(self) -> None:
        up = Upcasters().rename("EnvRefunded", 1, "EnvDeposited", 1, lambda b: b)
        env = decode(data("EnvRefunded", {"amount": 3}), upcasters=up)
        assert type(env.event) is EnvDepositedV1
        assert env.metadata.event_type == "EnvDeposited"
        assert env.metadata.stored_event_type == "EnvRefunded"

    def test_upcaster_sees_payload_without_envelope_echo_keys(self) -> None:
        seen: list[dict[str, Any]] = []

        def step(b: dict[str, Any]) -> dict[str, Any]:
            seen.append(dict(b))
            return {**b, "currency": "EUR"}

        up = Upcasters().register("EnvDeposited", 1, 2, step)
        legacy_ts = {"eventType": "EnvDeposited", "schemaVersion": 1, "amount": 5}
        decode(data("EnvDeposited", legacy_ts, version=1), upcasters=up)
        assert seen == [{"amount": 5}]

    def test_failing_upcaster_is_typed_and_positioned(self) -> None:
        def boom(_: dict[str, Any]) -> dict[str, Any]:
            raise ValueError("nope")

        up = Upcasters().register("EnvDeposited", 1, 2, boom)
        with pytest.raises(UpcastError) as exc:
            decode(data("EnvDeposited", {"amount": 5}, version=1), upcasters=up)
        assert exc.value.global_nonce == 42


class TestBackwardCompatibility:
    """Payloads already stored by TypeScript SDK <= 0.17 echo class fields."""

    def test_strict_model_accepts_typescript_echo_keys(self) -> None:
        legacy = {"eventType": "EnvDeposited", "schemaVersion": 2, "amount": 5, "currency": "EUR"}
        env = decode(data("EnvDeposited", legacy, version=2))
        assert env.event == EnvDepositedV2(amount=5, currency="EUR")

    def test_strict_model_accepts_legacy_python_event_type_key(self) -> None:
        env = decode(data("EnvDeposited", {"event_type": "EnvDeposited", "amount": 5}))
        assert env.event == EnvDepositedV1(amount=5)

    def test_generic_event_drops_echo_keys(self) -> None:
        legacy = {"eventType": "EnvUnregistered", "schemaVersion": 1, "x": 1}
        env = decode(data("EnvUnregistered", legacy))
        assert isinstance(env.event, GenericDomainEvent)
        assert env.event.model_dump() == {"event_type": "EnvUnregistered", "x": 1}

    def test_declared_field_with_an_echo_name_is_kept(self) -> None:
        @event("EnvHasSchemaVersion", "v1")
        class EnvHasSchemaVersion(DomainEvent):
            event_type: ClassVar[str] = "EnvHasSchemaVersion"
            schemaVersion: str  # noqa: N815 - a domain field that happens to share the name

        env = decode(data("EnvHasSchemaVersion", {"schemaVersion": "draft-7"}))
        assert env.event == EnvHasSchemaVersion(schemaVersion="draft-7")


class TestTypedErrors:
    def test_invalid_payload(self) -> None:
        with pytest.raises(EventPayloadError):
            decode(data("EnvDeposited", {"amount": "not-a-number"}))

    @pytest.mark.parametrize("payload", [b"[1]", b"null", b"3", b"{not json", b"\xff"])
    def test_non_object_payload(self, payload: bytes) -> None:
        with pytest.raises(EventPayloadError):
            decode(data("EnvDeposited", payload))

    def test_non_object_payload_of_unregistered_type_is_not_skipped(self) -> None:
        with pytest.raises(EventPayloadError):
            decode(data("EnvUnregistered", b"[]"))

    def test_unsupported_content_type(self) -> None:
        with pytest.raises(UnsupportedContentTypeError):
            decode(data("EnvDeposited", {"amount": 5}, content_type="application/protobuf"))

    @pytest.mark.parametrize("ct", ["", "application/json; charset=utf-8", "Application/JSON"])
    def test_json_content_types(self, ct: str) -> None:
        assert type(decode(data("EnvDeposited", {"amount": 5}, content_type=ct)).event) is (
            EnvDepositedV1
        )

    def test_unregistered_type_is_generic_not_skipped(self) -> None:
        env = decode(data("EnvUnregistered", {"x": 1}, version=4))
        assert isinstance(env.event, GenericDomainEvent)
        assert env.metadata.event_type == "EnvUnregistered"
        assert env.metadata.stored_event_version == 4

    def test_require_registered_raises_for_unregistered_type(self) -> None:
        with pytest.raises(UnknownEventTypeError):
            decode_event("EnvUnregistered", 1, b"{}", require_registered=True)

    def test_decode_event_returns_types_and_versions(self) -> None:
        up = Upcasters().register("EnvDeposited", 1, 2, lambda b: {**b, "currency": "EUR"})
        d = decode_event("EnvDeposited", 0, b'{"amount": 1}', upcasters=up)
        assert d.event == EnvDepositedV2(amount=1, currency="EUR")
        assert (d.event_type, d.event_version) == ("EnvDeposited", 2)
        assert (d.stored_event_type, d.stored_event_version) == ("EnvDeposited", 1)
