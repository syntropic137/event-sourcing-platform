from __future__ import annotations
import os
from typing import AsyncIterator
import grpc
from google.protobuf import json_format

# Runtime client using dynamic proto loading via grpcio-tools generated modules
# After running `make gen-py`, we can import generated classes.

class EventStoreClientRT:
    def __init__(self, addr: str | None = None):
        self.addr = addr or os.environ.get("EVENTSTORE_ADDR", "localhost:50051")
        # Lazy import after codegen; use relative package imports
        from .gen.eventstore.v1 import eventstore_pb2_grpc as es_grpc
        channel = grpc.insecure_channel(self.addr)
        self.stub = es_grpc.EventStoreStub(channel)
        from .gen.eventstore.v1 import eventstore_pb2 as es_pb
        self.pb = es_pb

    def append(self, req: dict):
        # Convert dict to protobuf using json_format for convenience
        message = json_format.ParseDict(req, self.pb.AppendRequest())
        return self.stub.Append(message)

    def read_stream(self, req: dict):
        message = json_format.ParseDict(req, self.pb.ReadStreamRequest())
        return self.stub.ReadStream(message)

    def subscribe(self, req: dict):
        message = json_format.ParseDict(req, self.pb.SubscribeRequest())
        return self.stub.Subscribe(message)

    def read_all(self, req: dict):
        """Read all events from a global position (for projections/catch-up)."""
        message = json_format.ParseDict(req, self.pb.ReadAllRequest())
        return self.stub.ReadAll(message)

    def server_info(self) -> dict:
        """Server version, backend, and capability flags.

        A server older than v0.17.0 answers UNIMPLEMENTED; that returns a
        legacy result (``legacy: True``, no capabilities) instead of raising.
        Other errors propagate.
        """
        try:
            resp = self.stub.GetServerInfo(self.pb.GetServerInfoRequest())
        except grpc.RpcError as e:
            if e.code() == grpc.StatusCode.UNIMPLEMENTED:
                return {
                    "server_version": None,
                    "api_version": None,
                    "backend": None,
                    "capabilities": [],
                    "legacy": True,
                }
            raise
        return {
            "server_version": resp.server_version,
            "api_version": resp.api_version,
            "backend": resp.backend,
            "capabilities": list(resp.capabilities),
            "legacy": False,
        }

    def require_capabilities(self, required: list[str]) -> dict:
        """Raise RuntimeError unless the server advertises every capability.

        Legacy (pre-0.17.0) servers advertise none, so they always fail a
        non-empty requirement.
        """
        info = self.server_info()
        missing = [c for c in required if c not in info["capabilities"]]
        if missing:
            version = info["server_version"] or "< 0.17.0 (no GetServerInfo)"
            raise RuntimeError(
                f"event store server {version} lacks required capabilities: {', '.join(missing)}"
            )
        return info
