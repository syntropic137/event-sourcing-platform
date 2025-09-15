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
