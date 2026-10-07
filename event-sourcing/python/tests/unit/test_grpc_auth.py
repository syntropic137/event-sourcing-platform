"""Gateway credentials, TLS and the plaintext-credentials guard (ADR-024, #302).

An in-process gRPC server stands in for the nginx gateway: every call must
carry an accepted ``authorization`` header, else ``UNAUTHENTICATED``, which
is what the gateway answers. It records the header each call carried.
"""

from __future__ import annotations

import asyncio
import base64
import shutil
import subprocess
from typing import TYPE_CHECKING, TypeVar

import grpc
import pytest

from event_sourcing import (
    BasicAuth,
    BearerToken,
    ClientConfigError,
    EventStoreAuthenticationError,
    EventStoreClientFactory,
    EventStoreError,
    GrpcEventStoreClient,
    SharedToken,
    TlsConfig,
    TokenProviderAuth,
)
from event_sourcing.client.auth import resolve_connection
from event_sourcing.proto.eventstore.v1 import eventstore_pb2, eventstore_pb2_grpc

if TYPE_CHECKING:
    from collections.abc import AsyncIterator, Callable
    from pathlib import Path

BASIC = "Basic " + base64.b64encode(b"admin:s3cret").decode()

_Req = TypeVar("_Req")
_Resp = TypeVar("_Resp")


class _GatewayServicer(eventstore_pb2_grpc.EventStoreServicer):
    def __init__(self, accept: Callable[[str | None], bool]) -> None:
        self.accept = accept
        self.seen: list[tuple[str, str | None]] = []

    async def _check(self, method: str, context: grpc.aio.ServicerContext[_Req, _Resp]) -> None:
        header = None
        for key, value in context.invocation_metadata() or ():
            if key == "authorization":
                header = value if isinstance(value, str) else value.decode()
        self.seen.append((method, header))
        if not self.accept(header):
            await context.abort(grpc.StatusCode.UNAUTHENTICATED, "basic auth failed")

    async def GetServerInfo(  # noqa: N802 - generated gRPC method name
        self,
        request: eventstore_pb2.GetServerInfoRequest,
        context: grpc.aio.ServicerContext[
            eventstore_pb2.GetServerInfoRequest, eventstore_pb2.GetServerInfoResponse
        ],
    ) -> eventstore_pb2.GetServerInfoResponse:
        await self._check("GetServerInfo", context)
        return eventstore_pb2.GetServerInfoResponse(server_version="0.17.0")

    async def ReadStream(  # noqa: N802 - generated gRPC method name
        self,
        request: eventstore_pb2.ReadStreamRequest,
        context: grpc.aio.ServicerContext[
            eventstore_pb2.ReadStreamRequest, eventstore_pb2.ReadStreamResponse
        ],
    ) -> eventstore_pb2.ReadStreamResponse:
        await self._check("ReadStream", context)
        return eventstore_pb2.ReadStreamResponse(is_end=True)

    async def ReadAll(  # noqa: N802 - generated gRPC method name
        self,
        request: eventstore_pb2.ReadAllRequest,
        context: grpc.aio.ServicerContext[
            eventstore_pb2.ReadAllRequest, eventstore_pb2.ReadAllResponse
        ],
    ) -> eventstore_pb2.ReadAllResponse:
        await self._check("ReadAll", context)
        return eventstore_pb2.ReadAllResponse(is_end=True)

    async def Append(  # noqa: N802 - generated gRPC method name
        self,
        request: eventstore_pb2.AppendRequest,
        context: grpc.aio.ServicerContext[
            eventstore_pb2.AppendRequest, eventstore_pb2.AppendResponse
        ],
    ) -> eventstore_pb2.AppendResponse:
        await self._check("Append", context)
        return eventstore_pb2.AppendResponse(last_global_nonce=1, last_aggregate_nonce=1)

    async def Subscribe(  # noqa: N802 - generated gRPC method name
        self,
        request: eventstore_pb2.SubscribeRequest,
        context: grpc.aio.ServicerContext[
            eventstore_pb2.SubscribeRequest, eventstore_pb2.SubscribeResponse
        ],
    ) -> AsyncIterator[eventstore_pb2.SubscribeResponse]:
        await self._check("Subscribe", context)
        yield eventstore_pb2.SubscribeResponse()  # keepalive, skipped by the client


async def _serve(
    accept: Callable[[str | None], bool],
    server_credentials: grpc.ServerCredentials | None = None,
) -> tuple[_GatewayServicer, grpc.aio.Server, int]:
    servicer = _GatewayServicer(accept)
    server = grpc.aio.server()
    eventstore_pb2_grpc.add_EventStoreServicer_to_server(servicer, server)
    if server_credentials is None:
        port = server.add_insecure_port("127.0.0.1:0")
    else:
        port = server.add_secure_port("127.0.0.1:0", server_credentials)
    await server.start()
    return servicer, server, port


@pytest.fixture
async def gateway() -> AsyncIterator[tuple[_GatewayServicer, int]]:
    servicer, server, port = await _serve(lambda h: h in (BASIC, "Bearer one", "Bearer two"))
    yield servicer, port
    await server.stop(None)


async def _exercise(client: GrpcEventStoreClient) -> None:
    """One call of every kind the client makes, unary and streaming."""
    await client.server_info()
    await client.read_events("Order-1")
    from event_sourcing.core.event import EventEnvelope, EventMetadata, GenericDomainEvent

    envelope = EventEnvelope(
        event=GenericDomainEvent(event_type="OrderPlaced"),
        metadata=EventMetadata(aggregate_nonce=1, aggregate_type="Order", aggregate_id="1"),
    )
    await client.append_events("Order-1", [envelope], expected_version=0)
    await client.read_all()
    async for _ in client.subscribe(0):
        pass


ALL_METHODS = ["GetServerInfo", "ReadStream", "Append", "ReadAll", "Subscribe"]


async def test_basic_auth_sent_on_unary_and_streaming_calls(
    gateway: tuple[_GatewayServicer, int],
) -> None:
    servicer, port = gateway
    client = GrpcEventStoreClient(f"127.0.0.1:{port}", auth=BasicAuth("admin", "s3cret"))
    await client.connect()
    try:
        await _exercise(client)
    finally:
        await client.disconnect()
    assert [m for m, _ in servicer.seen] == ALL_METHODS
    assert all(h == BASIC for _, h in servicer.seen), servicer.seen


async def test_factory_passes_credentials(gateway: tuple[_GatewayServicer, int]) -> None:
    servicer, port = gateway
    client = EventStoreClientFactory.create_grpc_client(
        "127.0.0.1", port, auth=BasicAuth("admin", "s3cret")
    )
    await client.connect()
    try:
        assert await client.read_events("Order-1") == []
    finally:
        await client.disconnect()
    assert servicer.seen == [("ReadStream", BASIC)]


@pytest.mark.parametrize("auth", [BasicAuth("admin", "wrong-pw"), None])
async def test_rejected_credentials_raise_typed_error(
    gateway: tuple[_GatewayServicer, int], auth: BasicAuth | None
) -> None:
    servicer, port = gateway
    client = GrpcEventStoreClient(f"127.0.0.1:{port}", auth=auth)
    await client.connect()
    try:
        calls = [
            client.server_info,
            lambda: client.read_events("Order-1"),
            client.read_all,
            # Rejected credentials must not read as "stream does not exist".
            lambda: client.stream_exists("Order-1"),
        ]
        for call in calls:
            with pytest.raises(EventStoreAuthenticationError) as info:
                await call()
            assert isinstance(info.value, EventStoreError)
            assert "wrong-pw" not in f"{info.value} {info.value.details}"
        with pytest.raises(EventStoreAuthenticationError):
            async for _ in client.subscribe(0):
                pass
    finally:
        await client.disconnect()
    expected = None if auth is None else "Basic " + base64.b64encode(b"admin:wrong-pw").decode()
    assert {h for _, h in servicer.seen} == {expected}


async def test_token_provider_rotates_without_reconnect(
    gateway: tuple[_GatewayServicer, int],
) -> None:
    servicer, port = gateway
    shared = SharedToken("one")
    client = GrpcEventStoreClient(f"127.0.0.1:{port}", auth=TokenProviderAuth(shared))

    async def async_provider() -> str:
        await asyncio.sleep(0.005)
        return "two"

    async_client = GrpcEventStoreClient(f"127.0.0.1:{port}", auth=TokenProviderAuth(async_provider))
    await client.connect()
    await async_client.connect()
    try:
        await client.server_info()
        shared.set("two")
        await client.server_info()
        async for _ in client.subscribe(0):
            pass
        await async_client.server_info()
        async for _ in async_client.subscribe(0):
            pass
    finally:
        await client.disconnect()
        await async_client.disconnect()
    assert [h for _, h in servicer.seen] == ["Bearer one"] + ["Bearer two"] * 4


async def test_failing_token_provider_fails_call_before_sending(
    gateway: tuple[_GatewayServicer, int],
) -> None:
    servicer, port = gateway

    def broken() -> str:
        raise RuntimeError("refresh failed: leaked-secret")

    async def broken_async() -> str:
        raise RuntimeError("leaked-secret")

    def bad_value() -> str:
        return "bad\r\nvalue"

    for provider in (broken, broken_async, bad_value):
        client = GrpcEventStoreClient(f"127.0.0.1:{port}", auth=TokenProviderAuth(provider))
        await client.connect()
        try:
            with pytest.raises(EventStoreAuthenticationError) as info:
                await client.server_info()
            assert "leaked-secret" not in f"{info.value} {info.value.details}"
            with pytest.raises(EventStoreAuthenticationError):
                async for _ in client.subscribe(0):
                    pass
        finally:
            await client.disconnect()
    assert servicer.seen == []


@pytest.mark.parametrize(
    "address",
    ["es.example.com:8081", "http://10.0.0.5:8081", "dns:///es.example.com:8081", "0.0.0.0:1"],
)
def test_credentials_refused_over_plaintext_to_remote(address: str) -> None:
    with pytest.raises(ClientConfigError, match="plaintext"):
        GrpcEventStoreClient(address, auth=BasicAuth("u", "p"))


def test_credentials_allowed_with_opt_in_tls_or_loopback() -> None:
    auth = BasicAuth("u", "p")
    GrpcEventStoreClient("es.example.com:8081", auth=auth, allow_insecure_credentials=True)
    GrpcEventStoreClient("https://es.example.com", auth=BearerToken("t"))
    GrpcEventStoreClient("es.example.com:443", auth=auth, tls=True)
    for local in ["localhost:1", "127.0.0.1:1", "127.8.9.10:1", "[::1]:1", "http://localhost:1"]:
        GrpcEventStoreClient(local, auth=auth)
    GrpcEventStoreClient("es.example.com:8081")


def test_endpoint_forms() -> None:
    def r(address: str, tls: TlsConfig | bool | None = None) -> tuple[str, bool]:
        c = resolve_connection(address, tls=tls)
        return c.target, c.tls

    assert r("127.0.0.1:50051") == ("127.0.0.1:50051", False)
    assert r("http://es:50051") == ("es:50051", False)
    assert r("https://es.example.com:443") == ("es.example.com:443", True)
    assert r("HTTPS://es:443") == ("es:443", True)
    assert r("es:443", tls=True) == ("es:443", True)
    with pytest.raises(ClientConfigError):
        r("http://es:443", tls=True)
    for bad in ["", "   ", "grpc://es:1", "http://", "http://es:1/path"]:
        with pytest.raises(ClientConfigError):
            r(bad)
    with pytest.raises(ClientConfigError) as info:
        r("https://admin:hunter2@es:443")
    assert "hunter2" not in str(info.value)
    with pytest.raises(ClientConfigError) as info:
        GrpcEventStoreClient("dns:///admin:hunter2@es:443", auth=BasicAuth("u", "p"))
    assert "hunter2" not in str(info.value)
    for bad_auth in [
        BasicAuth("a:b", "p"),
        BearerToken(""),
        BearerToken("sec\nret"),
        BearerToken("secret\n"),
    ]:
        with pytest.raises(ClientConfigError) as info:
            GrpcEventStoreClient("localhost:1", auth=bad_auth)
        assert "sec" not in str(info.value)


def test_secrets_never_in_repr() -> None:
    values = [
        BasicAuth("user", "hunter2"),
        BearerToken("tok-secret"),
        SharedToken("shared-secret"),
        TlsConfig(private_key=b"key-secret", certificate_chain=b"c"),
        GrpcEventStoreClient("localhost:1", auth=BasicAuth("user", "hunter2")),
    ]
    text = " ".join(f"{v!r} {v}" for v in values)
    assert "user" in text
    for secret in ["hunter2", "tok-secret", "shared-secret", "key-secret"]:
        assert secret not in text, text
    assert base64.b64encode(b"user:hunter2").decode() not in text


@pytest.mark.skipif(shutil.which("openssl") is None, reason="openssl not found")
async def test_tls_with_custom_ca_and_server_name(tmp_path: Path) -> None:
    subprocess.run(
        [
            "openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1",
            "-keyout", str(tmp_path / "key.pem"), "-out", str(tmp_path / "cert.pem"),
            "-subj", "/CN=es.test", "-addext", "subjectAltName=DNS:es.test",
        ],
        check=True,
        capture_output=True,
    )  # fmt: skip
    cert = (tmp_path / "cert.pem").read_bytes()
    key = (tmp_path / "key.pem").read_bytes()
    servicer, server, port = await _serve(
        lambda h: h == BASIC, grpc.ssl_server_credentials([(key, cert)])
    )
    ok = GrpcEventStoreClient(
        f"https://127.0.0.1:{port}",
        tls=TlsConfig(root_certificates=cert, server_name="es.test"),
        auth=BasicAuth("admin", "s3cret"),
    )
    # Default roots: the self-signed certificate must be rejected.
    untrusted = GrpcEventStoreClient(
        f"https://127.0.0.1:{port}",
        tls=TlsConfig(server_name="es.test"),
        auth=BasicAuth("admin", "s3cret"),
    )
    await ok.connect()
    await untrusted.connect()
    try:
        await ok.server_info()
        async for _ in ok.subscribe(0):
            pass
        assert servicer.seen == [("GetServerInfo", BASIC), ("Subscribe", BASIC)]
        with pytest.raises(EventStoreError) as info:
            await untrusted.server_info()
        assert not isinstance(info.value, EventStoreAuthenticationError)
    finally:
        await ok.disconnect()
        await untrusted.disconnect()
        await server.stop(None)


def test_insecure_channel_credentials_do_not_count_as_tls() -> None:
    import grpc.experimental

    insecure = grpc.experimental.insecure_channel_credentials()
    for address in ["es.example.com:8081", "https://es.example.com:443"]:
        with pytest.raises(ClientConfigError):
            GrpcEventStoreClient(address, auth=BasicAuth("u", "p"), credentials=insecure)
    composite = grpc.composite_channel_credentials(
        insecure, grpc.access_token_call_credentials("t")
    )
    with pytest.raises(ClientConfigError):
        GrpcEventStoreClient(
            "https://es.example.com:443", auth=BasicAuth("u", "p"), credentials=composite
        )
    GrpcEventStoreClient(
        "es.example.com:443",
        auth=BasicAuth("u", "p"),
        credentials=grpc.ssl_channel_credentials(),
    )


async def test_deadline_bounds_a_stalled_token_provider(
    gateway: tuple[_GatewayServicer, int],
) -> None:
    servicer, port = gateway

    async def stalled() -> str:
        await asyncio.sleep(3600)
        return "never"

    client = GrpcEventStoreClient(f"127.0.0.1:{port}", auth=TokenProviderAuth(stalled))
    await client.connect()
    try:
        assert client._stub is not None  # pyright: ignore[reportPrivateUsage]
        with pytest.raises(grpc.aio.AioRpcError) as info:
            await asyncio.wait_for(
                client._stub.GetServerInfo(eventstore_pb2.GetServerInfoRequest(), timeout=0.05),  # pyright: ignore[reportPrivateUsage]
                timeout=2,
            )
        assert info.value.code() == grpc.StatusCode.DEADLINE_EXCEEDED
    finally:
        await client.disconnect()
    assert servicer.seen == []
