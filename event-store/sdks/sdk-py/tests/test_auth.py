"""Gateway credentials, TLS and the plaintext guard for EventStoreClientRT (#302).

An in-process gRPC server stands in for the ADR-024 gateway: every call must
carry an accepted ``authorization`` header, else ``UNAUTHENTICATED``.

    make -C event-store test-sdk-py
"""

from __future__ import annotations

import base64
import shutil
import subprocess
from concurrent import futures

import grpc
import pytest

from sdk_py.auth import (
    BasicAuth,
    BearerToken,
    ClientConfigError,
    SharedToken,
    TlsConfig,
    TokenProviderAuth,
    UnauthenticatedError,
    resolve_connection,
)
from sdk_py.client_rt import EventStoreClientRT
from sdk_py.gen.eventstore.v1 import eventstore_pb2 as pb
from sdk_py.gen.eventstore.v1 import eventstore_pb2_grpc as pbg

BASIC = "Basic " + base64.b64encode(b"admin:s3cret").decode()
READ = {"tenant_id": "t", "aggregate_id": "a", "from_aggregate_nonce": 1, "max_count": 1, "forward": True}
SUB = {"tenant_id": "t", "aggregate_id_prefix": "", "from_global_nonce": 0}


class Gateway(pbg.EventStoreServicer):
    def __init__(self, accept):
        self.accept = accept
        self.seen = []

    def _check(self, method, context):
        header = dict(context.invocation_metadata()).get("authorization")
        self.seen.append((method, header))
        if not self.accept(header):
            context.abort(grpc.StatusCode.UNAUTHENTICATED, "basic auth failed")

    def GetServerInfo(self, request, context):  # noqa: N802
        self._check("GetServerInfo", context)
        return pb.GetServerInfoResponse(server_version="0.17.0")

    def ReadStream(self, request, context):  # noqa: N802
        self._check("ReadStream", context)
        return pb.ReadStreamResponse(is_end=True)

    def ReadAll(self, request, context):  # noqa: N802
        self._check("ReadAll", context)
        return pb.ReadAllResponse(is_end=True)

    def Append(self, request, context):  # noqa: N802
        self._check("Append", context)
        return pb.AppendResponse(last_global_nonce=1)

    def Subscribe(self, request, context):  # noqa: N802
        self._check("Subscribe", context)
        yield pb.SubscribeResponse()


def serve(accept, server_credentials=None):
    servicer = Gateway(accept)
    server = grpc.server(futures.ThreadPoolExecutor(max_workers=4))
    pbg.add_EventStoreServicer_to_server(servicer, server)
    if server_credentials is None:
        port = server.add_insecure_port("127.0.0.1:0")
    else:
        port = server.add_secure_port("127.0.0.1:0", server_credentials)
    server.start()
    return servicer, server, port


@pytest.fixture
def gateway():
    servicer, server, port = serve(lambda h: h in (BASIC, "Bearer one", "Bearer two"))
    yield servicer, port
    server.stop(None)


def exercise(client):
    client.server_info()
    client.read_stream(READ)
    client.append({"tenant_id": "t", "aggregate_id": "a", "aggregate_type": "A"})
    client.read_all({"tenant_id": "t", "from_global_nonce": 0, "max_count": 1, "forward": True})
    assert len(list(client.subscribe(SUB))) == 1


def test_basic_auth_on_unary_and_streaming_calls(gateway):
    servicer, port = gateway
    exercise(EventStoreClientRT(f"127.0.0.1:{port}", auth=BasicAuth("admin", "s3cret")))
    assert [m for m, _ in servicer.seen] == ["GetServerInfo", "ReadStream", "Append", "ReadAll", "Subscribe"]
    assert {h for _, h in servicer.seen} == {BASIC}


@pytest.mark.parametrize("auth", [BasicAuth("admin", "wrong-pw"), None])
def test_rejected_credentials_raise_unauthenticated(gateway, auth):
    servicer, port = gateway
    client = EventStoreClientRT(f"127.0.0.1:{port}", auth=auth)
    for call in (client.server_info, lambda: client.read_stream(READ), lambda: list(client.subscribe(SUB))):
        with pytest.raises(UnauthenticatedError) as info:
            call()
        assert isinstance(info.value, grpc.RpcError)
        assert info.value.code() == grpc.StatusCode.UNAUTHENTICATED
        assert info.value.details() == "basic auth failed"
        assert "wrong-pw" not in str(info.value)


def test_token_provider_rotates(gateway):
    servicer, port = gateway
    shared = SharedToken("one")
    client = EventStoreClientRT(f"127.0.0.1:{port}", auth=TokenProviderAuth(shared))
    client.server_info()
    shared.set("two")
    client.server_info()
    list(client.subscribe(SUB))
    assert [h for _, h in servicer.seen] == ["Bearer one", "Bearer two", "Bearer two"]


def test_failing_provider_fails_before_sending(gateway):
    servicer, port = gateway

    def broken():
        raise RuntimeError("leaked-secret")

    for provider in (broken, lambda: "bad\r\nvalue"):
        client = EventStoreClientRT(f"127.0.0.1:{port}", auth=TokenProviderAuth(provider))
        for call in (client.server_info, lambda: list(client.subscribe(SUB))):
            with pytest.raises(UnauthenticatedError) as info:
                call()
            assert "leaked-secret" not in str(info.value)
    assert servicer.seen == []


@pytest.mark.parametrize(
    "addr", ["es.example.com:8081", "http://10.0.0.5:8081", "dns:///es.example.com:8081", "0.0.0.0:1"]
)
def test_credentials_refused_over_plaintext_to_remote(addr):
    with pytest.raises(ClientConfigError, match="plaintext"):
        EventStoreClientRT(addr, auth=BasicAuth("u", "p"))


def test_credentials_allowed_with_opt_in_tls_or_loopback():
    auth = BasicAuth("u", "p")
    EventStoreClientRT("es.example.com:8081", auth=auth, allow_insecure_credentials=True)
    EventStoreClientRT("https://es.example.com", auth=BearerToken("t"))
    EventStoreClientRT("es.example.com:443", auth=auth, tls=True)
    for local in ["localhost:1", "127.0.0.1:1", "[::1]:1", "http://localhost:1"]:
        EventStoreClientRT(local, auth=auth)


def test_endpoint_forms_and_validation():
    def r(addr, tls=None):
        c = resolve_connection(addr, tls=tls)
        return c.target, c.tls

    assert r("127.0.0.1:50051") == ("127.0.0.1:50051", False)
    assert r("http://es:50051") == ("es:50051", False)
    assert r("https://es:443") == ("es:443", True)
    assert r("es:443", tls=True) == ("es:443", True)
    for bad in ["   ", "grpc://es:1", "http://", "http://es:1/path"]:
        with pytest.raises(ClientConfigError):
            r(bad)
    with pytest.raises(ClientConfigError):
        r("http://es:443", tls=True)
    with pytest.raises(ClientConfigError) as info:
        r("https://admin:hunter2@es:443")
    assert "hunter2" not in str(info.value)
    for bad_auth in [BasicAuth("a:b", "p"), BearerToken(""), BearerToken("sec\nret")]:
        with pytest.raises(ClientConfigError) as info:
            EventStoreClientRT("localhost:1", auth=bad_auth)
        assert "sec" not in str(info.value)


def test_secrets_never_in_repr():
    text = " ".join(
        f"{v!r} {v}"
        for v in [
            BasicAuth("user", "hunter2"),
            BearerToken("tok-secret"),
            SharedToken("shared-secret"),
            TlsConfig(private_key=b"key-secret", certificate_chain=b"c"),
            resolve_connection("localhost:1", auth=BasicAuth("user", "hunter2")),
        ]
    )
    assert "user" in text
    for secret in ["hunter2", "tok-secret", "shared-secret", "key-secret"]:
        assert secret not in text, text
    assert base64.b64encode(b"user:hunter2").decode() not in text


@pytest.mark.skipif(shutil.which("openssl") is None, reason="openssl not found")
def test_tls_with_custom_ca(tmp_path):
    subprocess.run(
        ["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1",
         "-keyout", str(tmp_path / "key.pem"), "-out", str(tmp_path / "cert.pem"),
         "-subj", "/CN=es.test", "-addext", "subjectAltName=DNS:es.test"],
        check=True, capture_output=True,
    )  # fmt: skip
    cert = (tmp_path / "cert.pem").read_bytes()
    key = (tmp_path / "key.pem").read_bytes()
    servicer, server, port = serve(lambda h: h == BASIC, grpc.ssl_server_credentials([(key, cert)]))
    try:
        ok = EventStoreClientRT(
            f"https://127.0.0.1:{port}",
            tls=TlsConfig(root_certificates=cert, server_name="es.test"),
            auth=BasicAuth("admin", "s3cret"),
        )
        ok.server_info()
        assert len(list(ok.subscribe(SUB))) == 1
        assert servicer.seen == [("GetServerInfo", BASIC), ("Subscribe", BASIC)]
        untrusted = EventStoreClientRT(
            f"https://127.0.0.1:{port}", tls=TlsConfig(server_name="es.test"), auth=BasicAuth("admin", "s3cret")
        )
        with pytest.raises(grpc.RpcError) as info:
            untrusted.server_info()
        assert info.value.code() == grpc.StatusCode.UNAVAILABLE
    finally:
        server.stop(None)
