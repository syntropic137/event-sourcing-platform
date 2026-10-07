"""Connect-time correctness floor: what the event store server guarantees.

Servers from v0.17.0 implement the ``GetServerInfo`` RPC. Older servers answer
it with gRPC ``UNIMPLEMENTED``, which maps to :data:`LEGACY_SERVER_INFO`: an
unknown version that advertises no capabilities. Requirement checks therefore
fail closed against old servers.

See the event store compatibility docs for each capability's minimum version.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from typing import TYPE_CHECKING

from event_sourcing.core.errors import EventSourcingError

if TYPE_CHECKING:
    from collections.abc import Sequence

SERVER_INFO_MIN_VERSION = "0.17.0"
"""First server version that implements ``GetServerInfo``."""


class Capabilities:
    """Registry of capability flag names. Names are stable and never reused."""

    COMMIT_ORDERED_GLOBAL_NONCE = "commit_ordered_global_nonce"
    """Global nonces become visible in commit order, so a reader paging
    ``ReadAll`` or subscribing by global nonce never skips a nonce that commits
    after the cursor has passed it (#337, v0.16.0)."""

    SUBSCRIPTION_ERRORS_SURFACED = "subscription_errors_surfaced"
    """A subscription that cannot keep delivering ends with an error status
    naming the resume position, never an empty result or silent end
    (#350, v0.17.0)."""

    UNDECODABLE_EVENTS_SURFACED = "undecodable_events_surfaced"
    """An undecodable stored event ends the subscription or read with
    ``DATA_LOSS`` at its position; it is never skipped (#351, v0.17.0)."""

    LITERAL_SUBSCRIPTION_PREFIX = "literal_subscription_prefix"
    """The subscription aggregate id prefix is matched literally: ``\\``,
    ``%`` and ``_`` are not wildcards (#361, v0.17.0)."""


@dataclass(frozen=True)
class ServerInfo:
    """What the server reported about itself.

    All of ``server_version``, ``api_version`` and ``backend`` are ``None``
    for a legacy server (one that predates ``GetServerInfo``).
    """

    server_version: str | None
    api_version: str | None
    backend: str | None
    capabilities: tuple[str, ...] = field(default_factory=tuple)

    @property
    def is_legacy(self) -> bool:
        """True when the server predates ``GetServerInfo`` (< v0.17.0)."""
        return self.server_version is None

    def has_capability(self, name: str) -> bool:
        return name in self.capabilities

    def missing_capabilities(self, required: Sequence[str]) -> list[str]:
        """The subset of ``required`` the server does not advertise, in order."""
        if isinstance(required, str):
            # A bare string is a Sequence[str] of characters; treat it as one name.
            required = [required]
        return [c for c in required if c not in self.capabilities]

    def version_at_least(self, minimum: str) -> bool:
        """True when the version is known and ``>= minimum``.

        SemVer 2.0 precedence: pre-releases sort below their release and are
        compared identifier by identifier; build metadata is ignored. Always
        False for legacy servers, whose version is unknown.
        """
        if self.server_version is None:
            return False
        have = _parse_semver(self.server_version)
        want = _parse_semver(minimum)
        if have is None or want is None:
            return False
        return have >= want


LEGACY_SERVER_INFO = ServerInfo(server_version=None, api_version=None, backend=None)
"""Info for a server that answered ``GetServerInfo`` with ``UNIMPLEMENTED``."""


class CompatibilityError(EventSourcingError):
    """Raised when the server does not meet a client's stated floor."""

    def __init__(
        self,
        message: str,
        info: ServerInfo,
        missing: list[str] | None = None,
        required_version: str | None = None,
    ) -> None:
        details: dict[str, str | int | list[str]] = {
            "server_version": info.server_version or "",
        }
        if missing:
            details["missing"] = list(missing)
        if required_version:
            details["required_version"] = required_version
        super().__init__(message, details)
        self.info = info
        self.missing = list(missing or [])
        self.required_version = required_version


def _describe_version(info: ServerInfo) -> str:
    return info.server_version or f"< {SERVER_INFO_MIN_VERSION} (no GetServerInfo)"


def assert_capabilities(info: ServerInfo, required: Sequence[str]) -> None:
    """Raise :class:`CompatibilityError` unless every capability is advertised."""
    missing = info.missing_capabilities(required)
    if missing:
        raise CompatibilityError(
            f"event store server {_describe_version(info)} lacks required "
            f"capabilities: {', '.join(missing)}",
            info,
            missing=missing,
        )


def assert_min_version(info: ServerInfo, minimum: str) -> None:
    """Raise :class:`CompatibilityError` unless the version is known and >= minimum."""
    if not info.version_at_least(minimum):
        raise CompatibilityError(
            f"event store server {_describe_version(info)} is older than required {minimum}",
            info,
            required_version=minimum,
        )


_SEMVER = re.compile(
    r"v?([0-9]+)(?:\.([0-9]+))?(?:\.([0-9]+))?"
    r"(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?(?:\+.*)?"
)

# (is_alphanumeric, numeric value, text): numeric identifiers sort below
# alphanumeric ones, numerics compare numerically, alphanumerics in ASCII order.
_PreId = tuple[int, int, str]
_SemverKey = tuple[int, int, int, int, tuple[_PreId, ...]]


def _parse_semver(v: str) -> _SemverKey | None:
    """Parse into a key whose tuple order is SemVer 2.0 precedence."""
    m = _SEMVER.fullmatch(v.strip())
    if m is None:
        return None
    major, minor, patch, pre = m.groups()
    ids: tuple[_PreId, ...] = ()
    if pre:
        ids = tuple((0, int(i), "") if i.isdigit() else (1, 0, i) for i in pre.split("."))
    # A release (flag 1) outranks any of its pre-releases (flag 0); among
    # pre-releases a shorter identifier prefix sorts first (tuple order).
    return (int(major), int(minor or 0), int(patch or 0), 0 if pre else 1, ids)
