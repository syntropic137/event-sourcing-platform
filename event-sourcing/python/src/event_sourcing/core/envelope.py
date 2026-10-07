"""Encode and decode events in the cross-language envelope (ADR-027).

The store's ``EventData`` is metadata plus an opaque payload. Every SDK writes
the payload as a JSON object holding only the event's own fields, and puts the
type and schema version in ``event_type`` / ``event_version``. Readers decode
by ``(event_type, event_version)`` after running the upcaster chain.
"""

from __future__ import annotations

import json
import logging
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any, Literal, cast

from pydantic import ValidationError

from event_sourcing.core.errors import (
    EventPayloadError,
    UnknownEventTypeError,
    UnknownEventVersionError,
    UnsupportedContentTypeError,
    UpcastError,
)
from event_sourcing.core.event import DomainEvent, GenericDomainEvent
from event_sourcing.core.upcast import Upcasters, normalize_version
from event_sourcing.decorators.events import registered_event_versions, resolve_event_class

if TYPE_CHECKING:
    from collections.abc import Collection

logger = logging.getLogger(__name__)

CONTENT_TYPE_JSON = "application/json"

#: Keys some writers put in the payload that duplicate envelope metadata:
#: ``eventType``/``schemaVersion`` (TypeScript SDK <= 0.17 serialized its
#: event class fields) and ``event_type`` (older Python producers). The
#: payload reaches upcasters unchanged; right before validation, the keys the
#: selected model does not declare (by name or alias) are dropped, so strict
#: (``extra="forbid"``) models accept payloads already stored by those
#: writers. A ``GenericDomainEvent`` keeps them (only ``event_type`` is moved
#: to the attribute).
ENVELOPE_ECHO_KEYS: tuple[str, ...] = ("eventType", "schemaVersion", "event_type")

InvalidPayloadPolicy = Literal["raise", "generic"]


def is_json_content_type(content_type: str) -> bool:
    """Empty (unset) counts as JSON; parameters and case are ignored."""
    essence = content_type.split(";", 1)[0].strip()
    return essence == "" or essence.lower() == CONTENT_TYPE_JSON


@dataclass(frozen=True)
class DecodedEvent:
    """Result of :func:`decode_event`.

    ``event_type``/``event_version`` are what the event was decoded as (after
    upcasting); ``stored_event_type``/``stored_event_version`` are as written.
    """

    event: DomainEvent
    event_type: str
    event_version: int
    stored_event_type: str
    stored_event_version: int


def event_version_of(event: DomainEvent) -> int:
    """The ``event_version`` to write for ``event``: its class ``schema_version``,
    or for a ``GenericDomainEvent`` the version it was read at."""
    if isinstance(event, GenericDomainEvent):
        return event.event_version
    version: Any = type(event).schema_version  # runtime-checked
    if isinstance(version, bool) or not isinstance(version, int) or version < 1:
        msg = (
            f"{type(event).__name__}.schema_version must be an int >= 1 (ADR-027), got {version!r}"
        )
        raise ValueError(msg)
    return version


def event_type_of(event: DomainEvent) -> str:
    """The ``event_type`` to write for ``event``."""
    event_type = getattr(event, "event_type", None)
    return event_type if isinstance(event_type, str) and event_type else type(event).__name__


def encode_payload(event: DomainEvent) -> bytes:
    """The payload bytes for ``event``: a JSON object of its own fields only."""
    body: dict[str, Any] = event.model_dump(mode="json")
    if isinstance(event, GenericDomainEvent):
        # The type travels in metadata; it is an attribute here only so
        # aggregates can dispatch on it.
        body.pop("event_type", None)
    return json.dumps(body).encode("utf-8")


def _strip_echo_keys(body: dict[str, Any], keep: frozenset[str]) -> dict[str, Any]:
    if not any(k in body for k in ENVELOPE_ECHO_KEYS):
        return body
    return {k: v for k, v in body.items() if k not in ENVELOPE_ECHO_KEYS or k in keep}


def _model_fields(cls: type[DomainEvent]) -> frozenset[str]:
    """Payload keys ``cls`` declares: field names and their string aliases."""
    names: set[str] = set()
    for name, info in cls.model_fields.items():
        names.add(name)
        for alias in (info.alias, info.validation_alias):
            if isinstance(alias, str):
                names.add(alias)
    return frozenset(names)


def decode_event(
    event_type: str,
    event_version: int,
    payload: bytes,
    *,
    content_type: str = "",
    upcasters: Upcasters | None = None,
    global_nonce: int = 0,
    require_registered: bool = False,
    on_invalid_payload: InvalidPayloadPolicy = "raise",
) -> DecodedEvent:
    """Decode a stored event (ADR-027 reading steps).

    1. ``content_type`` must be empty or ``application/json``.
    2. ``event_version`` 0 is read as 1.
    3. The payload must be a JSON object.
    4. The upcaster chain runs on ``(event_type, event_version, payload)``.
    5. The result, without the :data:`ENVELOPE_ECHO_KEYS` the class does not
       declare, is validated by the class registered for the final
       ``(event_type, event_version)``.

    Errors are typed (:class:`~event_sourcing.core.errors.EventDecodeError`
    subclasses), never skipped:

    * registered type, no class at that version: ``UnknownEventVersionError``;
    * payload not a JSON object (or rejected by the class): ``EventPayloadError``
      (``on_invalid_payload="generic"`` instead returns a ``GenericDomainEvent``,
      the pre-ADR-027 behaviour, with a warning);
    * unregistered type: a ``GenericDomainEvent`` carrying the type and every
      payload field (ADR-023), or ``UnknownEventTypeError`` when
      ``require_registered``.
    """
    stored_version = normalize_version(event_version)
    if not is_json_content_type(content_type):
        raise UnsupportedContentTypeError(
            event_type, stored_version, f"content type '{content_type}' is not JSON", global_nonce
        )
    try:
        body: Any = json.loads(payload.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as e:
        raise EventPayloadError(
            event_type, stored_version, f"payload is not JSON: {e}", global_nonce, e
        ) from e
    if not isinstance(body, dict):
        raise EventPayloadError(
            event_type, stored_version, "payload is not a JSON object", global_nonce
        )
    # Upcasters see the stored body as is (as in the Rust SDK): which echo
    # keys are data is only known once the target model is.
    obj = cast("dict[str, Any]", body)

    ty, version = event_type, stored_version
    if upcasters is not None and upcasters.handles(ty, version):
        try:
            ty, version, obj = upcasters.upcast(ty, version, obj)
        except UpcastError as e:
            # Same error at the reader's position, so a coordinator halts there.
            raise UpcastError(
                e.event_type, e.event_version, e.reason, global_nonce, e.original_error
            ) from e

    def done(event: DomainEvent) -> DecodedEvent:
        return DecodedEvent(event, ty, version, event_type, stored_version)

    cls = resolve_event_class(ty, version) if ty else None
    if cls is None:
        known = registered_event_versions(ty) if ty else ()
        if known:
            raise UnknownEventVersionError(
                ty,
                version,
                f"registered versions are {list(known)}; register a class for v{version} "
                f"or an upcaster from v{version}",
                global_nonce,
            )
        if require_registered:
            raise UnknownEventTypeError(ty, version, "no event class registered", global_nonce)
        return done(_generic(ty, version, obj))

    try:
        return done(cls.model_validate(_strip_echo_keys(obj, _model_fields(cls))))
    except ValidationError as e:
        if on_invalid_payload == "generic":
            logger.warning(
                "Payload of %s v%d does not validate as %s; returning GenericDomainEvent "
                "(on_invalid_payload='generic')",
                ty,
                version,
                cls.__name__,
                extra={"global_nonce": global_nonce},
            )
            return done(_generic(ty, version, obj))
        raise EventPayloadError(
            ty,
            version,
            f"payload does not validate as {cls.__name__}: {e}",
            global_nonce,
            e,
        ) from e


def skip_unless_wanted(
    event_type: str,
    event_version: int,
    wanted: Collection[str],
    *,
    upcasters: Upcasters | None = None,
) -> DecodedEvent | None:
    """Filter before decoding (ADR-027 reading step 5).

    ``None`` when the event decodes to a type in ``wanted`` (the caller
    decodes it, and its errors are raised). Otherwise the event undecoded:
    a ``GenericDomainEvent`` with no payload fields, carrying the type and
    version it would decode as, so a consumer can skip it and move on. The
    type is the one after upcasting, which needs no payload: steps are keyed
    by type and version. A looping chain is decoded, so it raises.
    """
    stored_version = normalize_version(event_version)
    target = (
        upcasters.target(event_type, stored_version)
        if upcasters is not None
        else (event_type, stored_version)
    )
    if target is None or target[0] in wanted:
        return None
    ty, version = target
    return DecodedEvent(_generic(ty, version, {}), ty, version, event_type, stored_version)


def _generic(event_type: str, event_version: int, body: dict[str, Any]) -> GenericDomainEvent:
    fields = {k: v for k, v in body.items() if k != "event_type"}
    if event_type:
        # An instance attribute, so aggregates can dispatch on it (ADR-023).
        fields["event_type"] = event_type
    event = GenericDomainEvent.model_validate(fields)
    event._event_version = event_version  # pyright: ignore[reportPrivateUsage]
    return event
