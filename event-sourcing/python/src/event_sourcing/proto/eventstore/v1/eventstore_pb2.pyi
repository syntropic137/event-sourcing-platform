from google.protobuf.internal import containers as _containers
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class EventMetadata(_message.Message):
    __slots__ = ("event_id", "aggregate_id", "aggregate_type", "aggregate_nonce", "event_type", "event_version", "content_type", "content_schema", "correlation_id", "causation_id", "actor_id", "tenant_id", "timestamp_unix_ms", "recorded_time_unix_ms", "payload_sha256", "headers", "global_nonce")
    class HeadersEntry(_message.Message):
        __slots__ = ("key", "value")
        KEY_FIELD_NUMBER: _ClassVar[int]
        VALUE_FIELD_NUMBER: _ClassVar[int]
        key: str
        value: str
        def __init__(self, key: _Optional[str] = ..., value: _Optional[str] = ...) -> None: ...
    EVENT_ID_FIELD_NUMBER: _ClassVar[int]
    AGGREGATE_ID_FIELD_NUMBER: _ClassVar[int]
    AGGREGATE_TYPE_FIELD_NUMBER: _ClassVar[int]
    AGGREGATE_NONCE_FIELD_NUMBER: _ClassVar[int]
    EVENT_TYPE_FIELD_NUMBER: _ClassVar[int]
    EVENT_VERSION_FIELD_NUMBER: _ClassVar[int]
    CONTENT_TYPE_FIELD_NUMBER: _ClassVar[int]
    CONTENT_SCHEMA_FIELD_NUMBER: _ClassVar[int]
    CORRELATION_ID_FIELD_NUMBER: _ClassVar[int]
    CAUSATION_ID_FIELD_NUMBER: _ClassVar[int]
    ACTOR_ID_FIELD_NUMBER: _ClassVar[int]
    TENANT_ID_FIELD_NUMBER: _ClassVar[int]
    TIMESTAMP_UNIX_MS_FIELD_NUMBER: _ClassVar[int]
    RECORDED_TIME_UNIX_MS_FIELD_NUMBER: _ClassVar[int]
    PAYLOAD_SHA256_FIELD_NUMBER: _ClassVar[int]
    HEADERS_FIELD_NUMBER: _ClassVar[int]
    GLOBAL_NONCE_FIELD_NUMBER: _ClassVar[int]
    event_id: str
    aggregate_id: str
    aggregate_type: str
    aggregate_nonce: int
    event_type: str
    event_version: int
    content_type: str
    content_schema: str
    correlation_id: str
    causation_id: str
    actor_id: str
    tenant_id: str
    timestamp_unix_ms: int
    recorded_time_unix_ms: int
    payload_sha256: bytes
    headers: _containers.ScalarMap[str, str]
    global_nonce: int
    def __init__(self, event_id: _Optional[str] = ..., aggregate_id: _Optional[str] = ..., aggregate_type: _Optional[str] = ..., aggregate_nonce: _Optional[int] = ..., event_type: _Optional[str] = ..., event_version: _Optional[int] = ..., content_type: _Optional[str] = ..., content_schema: _Optional[str] = ..., correlation_id: _Optional[str] = ..., causation_id: _Optional[str] = ..., actor_id: _Optional[str] = ..., tenant_id: _Optional[str] = ..., timestamp_unix_ms: _Optional[int] = ..., recorded_time_unix_ms: _Optional[int] = ..., payload_sha256: _Optional[bytes] = ..., headers: _Optional[_Mapping[str, str]] = ..., global_nonce: _Optional[int] = ...) -> None: ...

class EventData(_message.Message):
    __slots__ = ("meta", "payload")
    META_FIELD_NUMBER: _ClassVar[int]
    PAYLOAD_FIELD_NUMBER: _ClassVar[int]
    meta: EventMetadata
    payload: bytes
    def __init__(self, meta: _Optional[_Union[EventMetadata, _Mapping]] = ..., payload: _Optional[bytes] = ...) -> None: ...

class AppendRequest(_message.Message):
    __slots__ = ("tenant_id", "aggregate_id", "aggregate_type", "expected_aggregate_nonce", "idempotency_key", "events")
    TENANT_ID_FIELD_NUMBER: _ClassVar[int]
    AGGREGATE_ID_FIELD_NUMBER: _ClassVar[int]
    AGGREGATE_TYPE_FIELD_NUMBER: _ClassVar[int]
    EXPECTED_AGGREGATE_NONCE_FIELD_NUMBER: _ClassVar[int]
    IDEMPOTENCY_KEY_FIELD_NUMBER: _ClassVar[int]
    EVENTS_FIELD_NUMBER: _ClassVar[int]
    tenant_id: str
    aggregate_id: str
    aggregate_type: str
    expected_aggregate_nonce: int
    idempotency_key: str
    events: _containers.RepeatedCompositeFieldContainer[EventData]
    def __init__(self, tenant_id: _Optional[str] = ..., aggregate_id: _Optional[str] = ..., aggregate_type: _Optional[str] = ..., expected_aggregate_nonce: _Optional[int] = ..., idempotency_key: _Optional[str] = ..., events: _Optional[_Iterable[_Union[EventData, _Mapping]]] = ...) -> None: ...

class AppendResponse(_message.Message):
    __slots__ = ("last_global_nonce", "last_aggregate_nonce")
    LAST_GLOBAL_NONCE_FIELD_NUMBER: _ClassVar[int]
    LAST_AGGREGATE_NONCE_FIELD_NUMBER: _ClassVar[int]
    last_global_nonce: int
    last_aggregate_nonce: int
    def __init__(self, last_global_nonce: _Optional[int] = ..., last_aggregate_nonce: _Optional[int] = ...) -> None: ...

class ReadStreamRequest(_message.Message):
    __slots__ = ("tenant_id", "aggregate_id", "from_aggregate_nonce", "max_count", "forward")
    TENANT_ID_FIELD_NUMBER: _ClassVar[int]
    AGGREGATE_ID_FIELD_NUMBER: _ClassVar[int]
    FROM_AGGREGATE_NONCE_FIELD_NUMBER: _ClassVar[int]
    MAX_COUNT_FIELD_NUMBER: _ClassVar[int]
    FORWARD_FIELD_NUMBER: _ClassVar[int]
    tenant_id: str
    aggregate_id: str
    from_aggregate_nonce: int
    max_count: int
    forward: bool
    def __init__(self, tenant_id: _Optional[str] = ..., aggregate_id: _Optional[str] = ..., from_aggregate_nonce: _Optional[int] = ..., max_count: _Optional[int] = ..., forward: bool = ...) -> None: ...

class ReadStreamResponse(_message.Message):
    __slots__ = ("events", "is_end", "next_from_aggregate_nonce")
    EVENTS_FIELD_NUMBER: _ClassVar[int]
    IS_END_FIELD_NUMBER: _ClassVar[int]
    NEXT_FROM_AGGREGATE_NONCE_FIELD_NUMBER: _ClassVar[int]
    events: _containers.RepeatedCompositeFieldContainer[EventData]
    is_end: bool
    next_from_aggregate_nonce: int
    def __init__(self, events: _Optional[_Iterable[_Union[EventData, _Mapping]]] = ..., is_end: bool = ..., next_from_aggregate_nonce: _Optional[int] = ...) -> None: ...

class SubscribeRequest(_message.Message):
    __slots__ = ("tenant_id", "aggregate_id_prefix", "from_global_nonce")
    TENANT_ID_FIELD_NUMBER: _ClassVar[int]
    AGGREGATE_ID_PREFIX_FIELD_NUMBER: _ClassVar[int]
    FROM_GLOBAL_NONCE_FIELD_NUMBER: _ClassVar[int]
    tenant_id: str
    aggregate_id_prefix: str
    from_global_nonce: int
    def __init__(self, tenant_id: _Optional[str] = ..., aggregate_id_prefix: _Optional[str] = ..., from_global_nonce: _Optional[int] = ...) -> None: ...

class SubscribeResponse(_message.Message):
    __slots__ = ("event",)
    EVENT_FIELD_NUMBER: _ClassVar[int]
    event: EventData
    def __init__(self, event: _Optional[_Union[EventData, _Mapping]] = ...) -> None: ...

class ConcurrencyErrorDetail(_message.Message):
    __slots__ = ("tenant_id", "aggregate_id", "actual_last_aggregate_nonce", "actual_last_global_nonce")
    TENANT_ID_FIELD_NUMBER: _ClassVar[int]
    AGGREGATE_ID_FIELD_NUMBER: _ClassVar[int]
    ACTUAL_LAST_AGGREGATE_NONCE_FIELD_NUMBER: _ClassVar[int]
    ACTUAL_LAST_GLOBAL_NONCE_FIELD_NUMBER: _ClassVar[int]
    tenant_id: str
    aggregate_id: str
    actual_last_aggregate_nonce: int
    actual_last_global_nonce: int
    def __init__(self, tenant_id: _Optional[str] = ..., aggregate_id: _Optional[str] = ..., actual_last_aggregate_nonce: _Optional[int] = ..., actual_last_global_nonce: _Optional[int] = ...) -> None: ...
