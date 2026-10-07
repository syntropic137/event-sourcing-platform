"""Upcasters: migrate stored events to the schema the code knows (ADR-007, ADR-027).

Stored events are immutable. When an event schema changes, bump its
``schema_version`` and register an upcaster from the old version. Upcasters
run on the stored JSON payload *before* decoding. Same semantics as the Rust
SDK's ``Upcasters``::

    upcasters = Upcasters().register(
        "MoneyDeposited", 1, 2, lambda body: {**body, "currency": "EUR"}
    )
    client = GrpcEventStoreClient(address, tenant_id, upcasters=upcasters)

Rules:

* A step maps one ``(event_type, version)`` to a newer version of the same
  type, or (:meth:`Upcasters.rename`) to another type. Steps chain until no
  step matches.
* Events without a matching step pass through untouched.
* The result is decoded by dispatching on the final type and version; a
  registered type at a version with no class is
  :class:`~event_sourcing.core.errors.UnknownEventVersionError`, never skipped.
* A step that raises, does not return a ``dict``, or a chain that loops is
  :class:`~event_sourcing.core.errors.UpcastError`.
"""

from __future__ import annotations

import re
from collections.abc import Callable
from dataclasses import dataclass
from typing import Any, cast

from event_sourcing.core.errors import UpcastError

JsonBody = dict[str, Any]
UpcastFn = Callable[[JsonBody], JsonBody]

#: Upper bound on chained steps; more means a rename cycle.
MAX_STEPS = 64

# ADR-027: non-empty printable ASCII without spaces.
_EVENT_TYPE = re.compile(r"^[\x21-\x7e]+$")


def is_valid_event_type(event_type: str) -> bool:
    """True if ``event_type`` is a valid ADR-027 event type name."""
    return bool(_EVENT_TYPE.match(event_type))


def normalize_version(event_version: int) -> int:
    """Proto3 ``0`` is "unset" and means version 1 (ADR-027)."""
    return max(event_version, 1)


@dataclass(frozen=True)
class _Step:
    to_type: str
    to_version: int
    fn: UpcastFn


class Upcasters:
    """A set of upcasting steps keyed by ``(event_type, from_version)``.

    ``register`` and ``rename`` return ``self`` so calls chain. Invalid
    registrations raise ``ValueError`` (programming errors caught at startup).
    """

    def __init__(self) -> None:
        self._steps: dict[tuple[str, int], _Step] = {}

    def register(
        self, event_type: str, from_version: int, to_version: int, fn: UpcastFn
    ) -> Upcasters:
        """Migrate ``event_type`` from ``from_version`` to a newer ``to_version``."""
        if to_version <= from_version:
            raise ValueError(
                f"upcaster for '{event_type}' must go to a newer version "
                f"({from_version} -> {to_version})"
            )
        return self._insert(event_type, from_version, event_type, to_version, fn)

    def rename(
        self,
        from_type: str,
        from_version: int,
        to_type: str,
        to_version: int,
        fn: UpcastFn,
    ) -> Upcasters:
        """Migrate ``(from_type, from_version)`` to another event type."""
        if from_type == to_type:
            raise ValueError(f"rename of '{from_type}' must change the event type; use register")
        return self._insert(from_type, from_version, to_type, to_version, fn)

    def _insert(
        self, from_type: str, from_version: int, to_type: str, to_version: int, fn: UpcastFn
    ) -> Upcasters:
        if not (is_valid_event_type(from_type) and is_valid_event_type(to_type)):
            raise ValueError(f"invalid event type in upcaster '{from_type}' -> '{to_type}'")
        if from_version < 1 or to_version < 1:
            raise ValueError("event versions start at 1")
        key = (from_type, from_version)
        if key in self._steps:
            raise ValueError(f"duplicate upcaster for '{from_type}' v{from_version}")
        self._steps[key] = _Step(to_type, to_version, fn)
        return self

    def is_empty(self) -> bool:
        """True if no step is registered."""
        return not self._steps

    def handles(self, event_type: str, event_version: int) -> bool:
        """True if a step starts at ``(event_type, event_version)``."""
        return (event_type, normalize_version(event_version)) in self._steps

    def upcast(
        self, event_type: str, event_version: int, payload: JsonBody
    ) -> tuple[str, int, JsonBody]:
        """Run the chain; return the final type, version and payload.

        The payload is returned unchanged when no step matches. Steps may
        mutate the body they receive.
        """
        ty, version = event_type, normalize_version(event_version)
        body = payload
        unchecked: Any = payload  # runtime-checked: callers may not honour the type
        if (ty, version) in self._steps and not isinstance(unchecked, dict):
            raise UpcastError(ty, version, "stored payload is not a JSON object")
        steps = 0
        while (step := self._steps.get((ty, version))) is not None:
            steps += 1
            if steps > MAX_STEPS:
                raise UpcastError(
                    event_type,
                    normalize_version(event_version),
                    f"more than {MAX_STEPS} chained steps (rename cycle?)",
                )
            try:
                result: Any = step.fn(body)
            except Exception as e:
                raise UpcastError(
                    ty, version, f"step to v{step.to_version} failed: {e}", original_error=e
                ) from e
            if not isinstance(result, dict):
                raise UpcastError(ty, version, "step did not return a JSON object")
            body = cast("JsonBody", result)
            ty, version = step.to_type, step.to_version
        return ty, version, body

    def __repr__(self) -> str:
        steps = sorted(
            f"{t} v{v} -> {s.to_type} v{s.to_version}" for (t, v), s in self._steps.items()
        )
        return f"Upcasters({steps})"
