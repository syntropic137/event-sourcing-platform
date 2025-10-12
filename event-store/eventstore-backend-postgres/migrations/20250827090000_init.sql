-- Core aggregates table tracks stream heads for optimistic concurrency
CREATE TABLE IF NOT EXISTS aggregates (
    tenant_id TEXT NOT NULL,
    aggregate_id TEXT NOT NULL,
    aggregate_type TEXT NOT NULL,
    last_nonce BIGINT NOT NULL DEFAULT 0,
    last_global_nonce BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (tenant_id, aggregate_id)
);

-- Append-only events table. global_nonce provides the total order.
CREATE TABLE IF NOT EXISTS events (
    tenant_id TEXT NOT NULL,
    aggregate_id TEXT NOT NULL,
    aggregate_type TEXT NOT NULL,
    aggregate_nonce BIGINT NOT NULL,
    global_nonce BIGSERIAL NOT NULL,
    event_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    event_version INTEGER NOT NULL,
    content_type TEXT NOT NULL,
    content_schema TEXT,
    correlation_id TEXT,
    causation_id TEXT,
    actor_id TEXT,
    timestamp_unix_ms BIGINT NOT NULL DEFAULT 0,
    recorded_time_unix_ms BIGINT NOT NULL,
    payload_sha256 BYTEA,
    headers JSONB NOT NULL DEFAULT '{}'::jsonb,
    payload BYTEA NOT NULL,
    PRIMARY KEY (tenant_id, aggregate_id, aggregate_nonce),
    UNIQUE (tenant_id, event_id),
    UNIQUE (global_nonce),
    CHECK (aggregate_nonce > 0)
);

CREATE INDEX IF NOT EXISTS idx_events_global_nonce ON events (global_nonce);
CREATE INDEX IF NOT EXISTS idx_events_tenant_aggregate ON events (tenant_id, aggregate_id);
CREATE INDEX IF NOT EXISTS idx_events_tenant_recorded_time ON events (tenant_id, recorded_time_unix_ms);

-- Per-aggregate idempotency ledger
CREATE TABLE IF NOT EXISTS idempotency (
    tenant_id TEXT NOT NULL,
    aggregate_id TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    request_fingerprint BYTEA NOT NULL,
    first_committed_nonce BIGINT NOT NULL,
    last_committed_nonce BIGINT NOT NULL,
    last_global_nonce BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (tenant_id, aggregate_id, idempotency_key)
);

-- Append-only guarantee: forbid UPDATE/DELETE on events.
CREATE OR REPLACE FUNCTION forbid_events_mutation()
RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'events are append-only and immutable';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_events_immutable_update ON events;
DROP TRIGGER IF EXISTS trg_events_immutable_delete ON events;

CREATE TRIGGER trg_events_immutable_update
BEFORE UPDATE ON events
FOR EACH ROW EXECUTE FUNCTION forbid_events_mutation();

CREATE TRIGGER trg_events_immutable_delete
BEFORE DELETE ON events
FOR EACH ROW EXECUTE FUNCTION forbid_events_mutation();

-- Enforce aggregate nonce contiguity at insertion time.
CREATE OR REPLACE FUNCTION validate_aggregate_nonce()
RETURNS trigger AS $$
DECLARE
    current_last BIGINT;
BEGIN
    SELECT e.aggregate_nonce
      INTO current_last
      FROM events AS e
     WHERE e.tenant_id = NEW.tenant_id
       AND e.aggregate_id = NEW.aggregate_id
     ORDER BY e.aggregate_nonce DESC
     LIMIT 1
     FOR UPDATE;

    IF current_last IS NULL THEN
        current_last := 0;
    END IF;

    IF NEW.aggregate_nonce <> current_last + 1 THEN
        RAISE EXCEPTION 'invalid aggregate_nonce %, expected % for aggregate % (tenant %)',
            NEW.aggregate_nonce, current_last + 1, NEW.aggregate_id, NEW.tenant_id;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_events_aggregate_seq ON events;
CREATE TRIGGER trg_events_aggregate_seq
BEFORE INSERT ON events
FOR EACH ROW EXECUTE FUNCTION validate_aggregate_nonce();
