"""Connection and call credentials (ADR-024, #302).

The ADR-024 nginx gateway checks HTTP Basic Auth on every gRPC call and
answers ``UNAUTHENTICATED`` when it fails. :class:`EventStoreClientRT` adds an
``authorization`` header to every call, unary and streaming (including
``subscribe``), through a channel interceptor, so it works on plaintext and
TLS channels alike. Semantics mirror the Rust client (``sdk-rs``
``ClientConfig``):

- :class:`BasicAuth`, a fixed :class:`BearerToken`, or :class:`TokenProviderAuth`
  (token read per call, so it can rotate without reconnecting)
- credentials are refused over plaintext to a non-loopback host unless
  ``allow_insecure_credentials=True``
- secrets never appear in ``repr``/``str`` or error messages
"""

from __future__ import annotations

import base64
import ipaddress
import re
from collections.abc import Callable, Iterator
from dataclasses import dataclass, field
from typing import Any, Union

import grpc

AUTHORIZATION = "authorization"
"""gRPC metadata key the gateway reads (HTTP/2 header names are lowercase)."""

TokenProvider = Callable[[], str]
"""Returns the current bearer token, without the ``Bearer `` prefix.

Called on every RPC (synchronously), so return a cached token and refresh it
elsewhere (see :class:`SharedToken`). An exception fails the call with
:class:`UnauthenticatedError` before anything is sent; do not put the token in it.
"""


class ClientConfigError(ValueError):
    """Invalid client config: endpoint, TLS or credentials. Never contains a secret."""

    def __init__(self, message: str) -> None:
        super().__init__(f"invalid event store client config: {message}")


class UnauthenticatedError(grpc.RpcError):
    """The server, or the ADR-024 gateway, rejected the credentials (gRPC
    ``UNAUTHENTICATED``), or a token provider failed.

    A ``grpc.RpcError`` with ``code()``/``details()``, so existing
    ``except grpc.RpcError`` handlers keep working.
    """

    def __init__(self, details: str) -> None:
        super().__init__(f"event store rejected the credentials (UNAUTHENTICATED): {details}")
        self._details = details

    def code(self) -> grpc.StatusCode:
        return grpc.StatusCode.UNAUTHENTICATED

    def details(self) -> str:
        return self._details


def map_rpc_error(error: grpc.RpcError) -> grpc.RpcError:
    """:class:`UnauthenticatedError` for gRPC ``UNAUTHENTICATED``; ``error`` otherwise."""
    if isinstance(error, UnauthenticatedError):
        return error
    code = getattr(error, "code", None)
    if callable(code) and code() == grpc.StatusCode.UNAUTHENTICATED:
        details = getattr(error, "details", None)
        mapped = UnauthenticatedError(str(details() or "") if callable(details) else "")
        mapped.__cause__ = error
        return mapped
    return error


class SharedToken:
    """A bearer token that can be replaced while clients are using it.

    Pass it as the provider (``TokenProviderAuth(shared)``) and call
    :meth:`set` from your refresh thread.
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


Credentials = Union[BasicAuth, BearerToken, TokenProviderAuth]


@dataclass(frozen=True, repr=False)
class TlsConfig:
    """TLS settings. Server certificates are always verified.

    ``root_certificates``: PEM CA bundle (default: gRPC's bundled roots).
    ``private_key``/``certificate_chain``: PEM client identity for mutual TLS.
    ``server_name``: verify the certificate against (and send as SNI) this
    name instead of the endpoint host.
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
    target: str
    tls: bool
    channel_credentials: grpc.ChannelCredentials | None
    options: list[tuple[str, str]] = field(default_factory=list)
    interceptor: _AuthInterceptor | None = None


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

    Endpoint forms: ``host:port`` (plaintext, or TLS when ``tls`` /
    ``channel_credentials`` is set), ``http://host:port`` (plaintext),
    ``https://host:port`` (TLS). Other gRPC targets (``dns:///...``,
    ``unix:...``) pass through unchanged.

    Raises ClientConfigError (never containing a secret) on a bad endpoint,
    TLS or credentials, or credentials over plaintext to a non-loopback host.
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

    interceptor = None
    if auth is not None:
        if not use_tls and not allow_insecure_credentials:
            host = _host_of(target)
            if host is None or not _is_loopback(host):
                raise ClientConfigError(
                    f"refusing to send credentials over plaintext to '{host or target}'; "
                    "use https:// or allow_insecure_credentials=True"
                )
        interceptor = _AuthInterceptor(_header_source(auth))

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
    return ResolvedConnection(target, use_tls, creds, options, interceptor)


def open_channel(conn: ResolvedConnection) -> grpc.Channel:
    """A (lazy) channel for ``conn`` with the auth interceptor applied."""
    if conn.channel_credentials is not None:
        channel = grpc.secure_channel(conn.target, conn.channel_credentials, options=conn.options)
    else:
        channel = grpc.insecure_channel(conn.target, options=conn.options)
    if conn.interceptor is not None:
        channel = grpc.intercept_channel(channel, conn.interceptor)
    return channel


def _header_value(scheme: str, credential: Any) -> str:
    if not isinstance(credential, str) or not credential:
        raise ClientConfigError("credential must be a non-empty string")
    if not _HEADER_SAFE.fullmatch(credential):
        raise ClientConfigError("credential contains characters not allowed in a header")
    return f"{scheme} {credential}"


def _header_source(auth: Credentials) -> Callable[[], str]:
    if isinstance(auth, BasicAuth):
        if not isinstance(auth.username, str) or not isinstance(auth.password, str):
            raise ClientConfigError("basic auth username and password must be strings")
        if ":" in auth.username:
            raise ClientConfigError("basic auth username must not contain ':'")
        encoded = base64.b64encode(f"{auth.username}:{auth.password}".encode()).decode("ascii")
        basic = _header_value("Basic", encoded)
        return lambda: basic
    if isinstance(auth, BearerToken):
        bearer = _header_value("Bearer", auth.token)
        return lambda: bearer
    if isinstance(auth, TokenProviderAuth) and callable(auth.provider):
        provider = auth.provider
        return lambda: _header_value("Bearer", provider())
    raise ClientConfigError("auth must be BasicAuth, BearerToken or TokenProviderAuth")


class _CallDetails(grpc.ClientCallDetails):
    def __init__(self, base: grpc.ClientCallDetails, metadata: list[tuple[str, str]]) -> None:
        self.method = base.method
        self.timeout = base.timeout
        self.metadata = metadata
        self.credentials = base.credentials
        self.wait_for_ready = getattr(base, "wait_for_ready", None)
        self.compression = getattr(base, "compression", None)


class _AuthInterceptor(
    grpc.UnaryUnaryClientInterceptor,
    grpc.UnaryStreamClientInterceptor,
    grpc.StreamUnaryClientInterceptor,
    grpc.StreamStreamClientInterceptor,
):
    """Adds the ``authorization`` header to every call type."""

    def __init__(self, source: Callable[[], str]) -> None:
        self._source = source

    def __repr__(self) -> str:
        return "_AuthInterceptor(<redacted>)"

    def _details(self, details: grpc.ClientCallDetails) -> grpc.ClientCallDetails:
        try:
            value = self._source()
        except ClientConfigError as e:
            raise UnauthenticatedError(str(e)) from None
        except Exception as e:
            # The provider's message may contain anything: report only its type.
            raise UnauthenticatedError(f"token provider failed ({type(e).__name__})") from None
        metadata = [(k, v) for k, v in (details.metadata or ()) if k != AUTHORIZATION]
        metadata.append((AUTHORIZATION, value))
        return _CallDetails(details, metadata)

    def intercept_unary_unary(self, continuation, client_call_details, request):  # type: ignore[no-untyped-def]
        return continuation(self._details(client_call_details), request)

    def intercept_unary_stream(self, continuation, client_call_details, request):  # type: ignore[no-untyped-def]
        return continuation(self._details(client_call_details), request)

    def intercept_stream_unary(self, continuation, client_call_details, request_iterator):  # type: ignore[no-untyped-def]
        return continuation(self._details(client_call_details), request_iterator)

    def intercept_stream_stream(self, continuation, client_call_details, request_iterator):  # type: ignore[no-untyped-def]
        return continuation(self._details(client_call_details), request_iterator)


class MappedStream:
    """A server-streaming call whose errors go through :func:`map_rpc_error`.

    Iterate it like the gRPC call; ``cancel()`` and other call methods are
    passed through.
    """

    def __init__(self, call: Any) -> None:
        self._call = call

    def __iter__(self) -> Iterator[Any]:
        return self

    def __next__(self) -> Any:
        try:
            return next(self._call)
        except grpc.RpcError as e:
            mapped = map_rpc_error(e)
            if mapped is e:
                raise
            raise mapped from e

    def __getattr__(self, name: str) -> Any:
        return getattr(self._call, name)


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
