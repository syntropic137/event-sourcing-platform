"""Connection and call credentials for the gRPC client (ADR-024, #302).

The ADR-024 nginx gateway checks HTTP Basic Auth on every gRPC call and
answers ``UNAUTHENTICATED`` when it fails. :class:`GrpcEventStoreClient` adds
an ``authorization`` header to every call, unary and streaming (including
``subscribe``), through channel interceptors, so it works on plaintext and TLS
channels alike. Semantics mirror the Rust client (``sdk-rs`` ``ClientConfig``):

- :class:`BasicAuth`, a fixed :class:`BearerToken`, or :class:`TokenProviderAuth`
  (token read per call, so it can rotate without reconnecting)
- credentials are refused over plaintext to a non-loopback host unless
  ``allow_insecure_credentials=True``
- secrets never appear in ``repr``/``str`` or error messages
"""

from __future__ import annotations

import base64
import inspect
import ipaddress
import re
from collections.abc import AsyncIterable, AsyncIterator, Awaitable, Callable, Iterable
from dataclasses import dataclass, field
from typing import TypeAlias, TypeVar

import grpc
import grpc.aio

from event_sourcing.core.errors import ClientConfigError

AUTHORIZATION = "authorization"
"""gRPC metadata key the gateway reads (HTTP/2 header names are lowercase)."""

TokenProvider: TypeAlias = Callable[[], str | Awaitable[str]]
"""Returns the current bearer token, without the ``Bearer `` prefix.

Called on every RPC, so return a cached token and refresh it elsewhere (see
:class:`SharedToken`). Sync or async. An exception fails the call with
``UNAUTHENTICATED`` before anything is sent; do not put the token in it.
"""


class SharedToken:
    """A bearer token that can be replaced while clients are using it.

    Pass it as the provider (``TokenProviderAuth(shared)``) and call
    :meth:`set` from your refresh task.
    """

    __slots__ = ("_token",)

    def __init__(self, token: str) -> None:
        self._token = token

    def set(self, token: str) -> None:
        """Replace the token. Takes effect on the next call."""
        self._token = token

    def __call__(self) -> str:
        return self._token

    def __repr__(self) -> str:
        return "SharedToken(<redacted>)"


@dataclass(frozen=True, repr=False)
class BasicAuth:
    """``authorization: Basic base64(username:password)``, what the ADR-024 gateway expects."""

    username: str
    password: str

    def __repr__(self) -> str:
        return f"BasicAuth(username={self.username!r}, password=<redacted>)"


@dataclass(frozen=True, repr=False)
class BearerToken:
    """``authorization: Bearer <token>`` with a fixed token."""

    token: str

    def __repr__(self) -> str:
        return "BearerToken(<redacted>)"


@dataclass(frozen=True, repr=False)
class TokenProviderAuth:
    """``authorization: Bearer <token>`` with the token read per call."""

    provider: TokenProvider

    def __repr__(self) -> str:
        return "TokenProviderAuth(..)"


Credentials: TypeAlias = BasicAuth | BearerToken | TokenProviderAuth


@dataclass(frozen=True, repr=False)
class TlsConfig:
    """TLS settings. Server certificates are always verified.

    Attributes:
        root_certificates: PEM CA bundle; default is gRPC's bundled roots.
        private_key: PEM client key for mutual TLS (with ``certificate_chain``).
        certificate_chain: PEM client certificate chain for mutual TLS.
        server_name: verify the server certificate against this name (and
            send it as SNI) instead of the endpoint host.
    """

    root_certificates: bytes | None = None
    private_key: bytes | None = None
    certificate_chain: bytes | None = None
    server_name: str | None = None

    def __repr__(self) -> str:
        return (
            f"TlsConfig(custom_ca={self.root_certificates is not None}, "
            f"client_identity={'<redacted>' if self.private_key else None}, "
            f"server_name={self.server_name!r})"
        )


@dataclass(frozen=True)
class ResolvedConnection:
    """What the client needs to open a channel. Holds no secrets in its repr."""

    target: str
    tls: bool
    channel_credentials: grpc.ChannelCredentials | None
    options: list[tuple[str, str]] = field(default_factory=lambda: [])
    interceptors: list[grpc.aio.ClientInterceptor] = field(default_factory=lambda: [])


_SCHEME = re.compile(r"^([A-Za-z][A-Za-z0-9+.-]*)://(.*)$", re.S)
_GRPC_SCHEMES = frozenset({"dns", "unix", "unix-abstract", "ipv4", "ipv6", "vsock"})
_HEADER_SAFE = re.compile(r"^[\x20-\x7e]+$")


def resolve_connection(
    address: str,
    *,
    tls: TlsConfig | bool | None = None,
    channel_credentials: grpc.ChannelCredentials | None = None,
    auth: Credentials | None = None,
    allow_insecure_credentials: bool = False,
) -> ResolvedConnection:
    """Validate ``address`` and connection options.

    Endpoint forms:

    - ``host:port``: plaintext, or TLS when ``tls``/``channel_credentials`` is set
    - ``http://host:port``: plaintext; combining it with TLS is an error
    - ``https://host:port``: TLS (default roots unless ``tls`` says otherwise)
    - other gRPC targets (``dns:///...``, ``unix:...``) pass through unchanged

    Raises:
        ClientConfigError: bad endpoint, TLS or credentials, or credentials
            over plaintext to a non-loopback host. Never contains a secret.
    """
    raw = (address or "").strip()
    if not raw:
        raise ClientConfigError("endpoint is empty")
    if _has_userinfo(raw):
        # Never echo the endpoint here: it contains a secret.
        raise ClientConfigError("endpoint must not contain user:password@; use auth=BasicAuth(...)")
    if tls and channel_credentials is not None:
        raise ClientConfigError("set either tls or channel_credentials, not both")
    tls_requested = bool(tls) or (
        channel_credentials is not None and _is_secure(channel_credentials)
    )

    target = raw
    use_tls = tls_requested
    m = _SCHEME.match(raw)
    if m and m.group(1).lower() not in _GRPC_SCHEMES:
        scheme, rest = m.group(1).lower(), m.group(2)
        if scheme == "http":
            if tls_requested:
                raise ClientConfigError(
                    "endpoint uses http:// but TLS is configured; use https:// or a bare host:port"
                )
            use_tls = False
        elif scheme == "https":
            if channel_credentials is not None and not tls_requested:
                raise ClientConfigError(
                    "endpoint uses https:// but the channel credentials are not TLS"
                )
            use_tls = True
        else:
            raise ClientConfigError(f"unsupported endpoint scheme '{scheme}' (use http or https)")
        target = rest
        if not target:
            raise ClientConfigError(f"endpoint '{raw}' has no host")
        if re.search(r"[/?#]", target):
            raise ClientConfigError(f"endpoint '{raw}' must not have a path")

    interceptors: list[grpc.aio.ClientInterceptor] = []
    if auth is not None:
        if not use_tls and not allow_insecure_credentials:
            host = _host_of(target)
            if host is None or not _is_loopback(host):
                raise ClientConfigError(
                    f"refusing to send credentials over plaintext to '{host or target}'; "
                    "use https:// or allow_insecure_credentials=True"
                )
        interceptors = auth_interceptors(auth)

    options: list[tuple[str, str]] = []
    creds = channel_credentials
    if creds is None and use_tls:
        t = tls if isinstance(tls, TlsConfig) else TlsConfig()
        if (t.private_key is None) != (t.certificate_chain is None):
            raise ClientConfigError("private_key and certificate_chain must be set together")
        creds = grpc.ssl_channel_credentials(
            root_certificates=t.root_certificates,
            private_key=t.private_key,
            certificate_chain=t.certificate_chain,
        )
        if t.server_name:
            options += [
                ("grpc.ssl_target_name_override", t.server_name),
                ("grpc.default_authority", t.server_name),
            ]
    return ResolvedConnection(target, use_tls, creds, options, interceptors)


def _header_value(scheme: str, credential: str) -> str:
    # Values come from user code (providers) at runtime: check the type too.
    if not isinstance(credential, str) or not credential:  # pyright: ignore[reportUnnecessaryIsInstance]
        raise ClientConfigError("credential must be a non-empty string")
    if not _HEADER_SAFE.fullmatch(credential):
        raise ClientConfigError("credential contains characters not allowed in a header")
    return f"{scheme} {credential}"


HeaderSource: TypeAlias = Callable[[], Awaitable[str]]


def _header_source(auth: Credentials) -> HeaderSource:
    """Validate ``auth`` and return an async header resolver."""
    if isinstance(auth, BasicAuth):
        if not isinstance(auth.username, str) or not isinstance(auth.password, str):  # pyright: ignore[reportUnnecessaryIsInstance]
            raise ClientConfigError("basic auth username and password must be strings")
        if ":" in auth.username:
            raise ClientConfigError("basic auth username must not contain ':'")
        encoded = base64.b64encode(f"{auth.username}:{auth.password}".encode()).decode("ascii")
        basic = _header_value("Basic", encoded)

        async def static_basic() -> str:
            return basic

        return static_basic
    if isinstance(auth, BearerToken):
        bearer = _header_value("Bearer", auth.token)

        async def static_bearer() -> str:
            return bearer

        return static_bearer
    if isinstance(auth, TokenProviderAuth) and callable(auth.provider):  # pyright: ignore[reportUnnecessaryIsInstance]
        provider = auth.provider

        async def from_provider() -> str:
            token = provider()
            if inspect.isawaitable(token):
                token = await token
            return _header_value("Bearer", token)

        return from_provider
    raise ClientConfigError("auth must be BasicAuth, BearerToken or TokenProviderAuth")


def _unauthenticated(details: str) -> grpc.aio.AioRpcError:
    return grpc.aio.AioRpcError(
        grpc.StatusCode.UNAUTHENTICATED,
        grpc.aio.Metadata(),
        grpc.aio.Metadata(),
        details=details,
    )


async def _with_auth(
    source: HeaderSource, details: grpc.aio.ClientCallDetails
) -> grpc.aio.ClientCallDetails:
    try:
        value = await source()
    except ClientConfigError as e:
        raise _unauthenticated(e.message) from None
    except Exception as e:
        # The provider's message may contain anything: report only its type.
        raise _unauthenticated(f"token provider failed ({type(e).__name__})") from None
    metadata = grpc.aio.Metadata()
    for key, val in details.metadata or ():
        if key != AUTHORIZATION:
            metadata.add(key, val)
    metadata.add(AUTHORIZATION, value)
    return grpc.aio.ClientCallDetails(
        method=details.method,
        timeout=details.timeout,
        metadata=metadata,
        credentials=details.credentials,
        wait_for_ready=details.wait_for_ready,
    )


# grpc.aio sorts interceptors by type with if/elif, so an object that
# implements several interceptor interfaces is only used for the first one:
# one class per call type.

_Req = TypeVar("_Req")
_Resp = TypeVar("_Resp")
_ReqStream: TypeAlias = AsyncIterable[_Req] | Iterable[_Req]


class _UnaryUnary(grpc.aio.UnaryUnaryClientInterceptor):
    def __init__(self, source: HeaderSource) -> None:
        self._source = source

    async def intercept_unary_unary(
        self,
        continuation: Callable[
            [grpc.aio.ClientCallDetails, _Req], Awaitable[grpc.aio.UnaryUnaryCall[_Req, _Resp]]
        ],
        client_call_details: grpc.aio.ClientCallDetails,
        request: _Req,
    ) -> _Resp | grpc.aio.UnaryUnaryCall[_Req, _Resp]:
        return await continuation(await _with_auth(self._source, client_call_details), request)


class _UnaryStream(grpc.aio.UnaryStreamClientInterceptor):
    def __init__(self, source: HeaderSource) -> None:
        self._source = source

    async def intercept_unary_stream(
        self,
        continuation: Callable[
            [grpc.aio.ClientCallDetails, _Req], Awaitable[grpc.aio.UnaryStreamCall[_Req, _Resp]]
        ],
        client_call_details: grpc.aio.ClientCallDetails,
        request: _Req,
    ) -> AsyncIterator[_Resp] | grpc.aio.UnaryStreamCall[_Req, _Resp]:
        return await continuation(await _with_auth(self._source, client_call_details), request)


class _StreamUnary(grpc.aio.StreamUnaryClientInterceptor):
    def __init__(self, source: HeaderSource) -> None:
        self._source = source

    async def intercept_stream_unary(
        self,
        continuation: Callable[
            [grpc.aio.ClientCallDetails, _ReqStream[_Req]],
            Awaitable[grpc.aio.StreamUnaryCall[_Req, _Resp]],
        ],
        client_call_details: grpc.aio.ClientCallDetails,
        request_iterator: _ReqStream[_Req],
    ) -> _Resp | grpc.aio.StreamUnaryCall[_Req, _Resp]:
        details = await _with_auth(self._source, client_call_details)
        return await continuation(details, request_iterator)


class _StreamStream(grpc.aio.StreamStreamClientInterceptor):
    def __init__(self, source: HeaderSource) -> None:
        self._source = source

    async def intercept_stream_stream(
        self,
        continuation: Callable[
            [grpc.aio.ClientCallDetails, _ReqStream[_Req]],
            Awaitable[grpc.aio.StreamStreamCall[_Req, _Resp]],
        ],
        client_call_details: grpc.aio.ClientCallDetails,
        request_iterator: _ReqStream[_Req],
    ) -> AsyncIterator[_Resp] | grpc.aio.StreamStreamCall[_Req, _Resp]:
        details = await _with_auth(self._source, client_call_details)
        return await continuation(details, request_iterator)


def auth_interceptors(auth: Credentials) -> list[grpc.aio.ClientInterceptor]:
    """grpc.aio interceptors adding the ``authorization`` header to every call type.

    Raises:
        ClientConfigError: invalid static credentials (message has no secret).
    """
    source = _header_source(auth)
    return [_UnaryUnary(source), _UnaryStream(source), _StreamUnary(source), _StreamStream(source)]


# Channel credential kinds that encrypt the connection. Anything else
# (insecure, local, unknown) does not count as TLS for the plaintext guard.
_SECURE_CREDENTIAL_KINDS = frozenset(
    {"SSLChannelCredentials", "CompositeChannelCredentials", "ALTSChannelCredentials"}
)


def _is_secure(creds: grpc.ChannelCredentials) -> bool:
    inner = getattr(creds, "_credentials", None)
    return type(inner).__name__ in _SECURE_CREDENTIAL_KINDS


def _has_userinfo(endpoint: str) -> bool:
    rest = endpoint.split("://", 1)[1] if "://" in endpoint else endpoint
    authority = re.split(r"[/?#]", rest, maxsplit=1)[0]
    return "@" in authority


def _host_of(target: str) -> str | None:
    """Host of ``host:port``/``[v6]:port``/``host``; None for other gRPC target forms."""
    if not target.startswith("[") and re.match(r"^[A-Za-z][A-Za-z0-9+.-]*:(?!\d+$)", target):
        return None  # dns:..., unix:..., ipv4:...: fail closed
    if target.startswith("["):
        end = target.find("]")
        return target[1:end] if end > 0 else None
    parts = target.split(":")
    if len(parts) > 2:
        return target  # bare IPv6
    return parts[0] or None


def _is_loopback(host: str) -> bool:
    if host.lower() == "localhost":
        return True
    try:
        return ipaddress.ip_address(host).is_loopback
    except ValueError:
        return False
