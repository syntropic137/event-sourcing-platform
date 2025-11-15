# SQL Enforcement (Postgres)

Suggested table `events`:

- Columns
  - global_nonce BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY
  - event_id UUID NOT NULL UNIQUE
  - stream_id TEXT NOT NULL
  - stream_type TEXT NOT NULL
  - stream_version BIGINT NOT NULL
  - event_type TEXT NOT NULL
  - schema_version INT NOT NULL
  - content_type TEXT NOT NULL
  - payload BYTEA NOT NULL
  - correlation_id TEXT NULL
  - causation_id TEXT NULL
  - actor_id TEXT NULL
  - tenant_id TEXT NULL
  - headers JSONB NOT NULL DEFAULT '{}'::jsonb
  - timestamp TIMESTAMPTZ NOT NULL DEFAULT now()

- Constraints/Indexes
  - UNIQUE(stream_id, stream_version)
  - INDEX(global_nonce)
  - INDEX(stream_type, stream_id, stream_version)

- Sequencing
  - Enforce contiguous stream_version per stream_id (application tx with SELECT ... FOR UPDATE or trigger).

- Optional registry
  - event_types(event_type, schema_version, data_schema_uri, content_type, active)
  - Validate on insert via FK or application logic.
