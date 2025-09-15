import os
from sdk_py.client_rt import EventStoreClientRT

addr = os.environ.get("EVENTSTORE_ADDR", "localhost:50051")
client = EventStoreClientRT(addr)

append_resp = client.append({
    "aggregate_id": "Order-PY-1",
    "aggregate_type": "Order",
    "expected_any": "NO_AGGREGATE",
    "events": [ { "meta": { "event_type": "OrderCreated" }, "payload": "aGVsbG8=" } ]
})
print("append:", append_resp)

read_resp = client.read_stream({ "aggregate_id": "Order-PY-1", "from_aggregate_nonce": 1, "max_count": 100, "forward": True })
print("read:", read_resp)
