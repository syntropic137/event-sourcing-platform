-- events table: append-only event log
CREATE TABLE IF NOT EXISTS events (
    global_nonce BIGSERIAL PRIMARY KEY,
    event_id UUID NOT NULL UNIQUE,
    aggregate_id TEXT NOT NULL,
    aggregate_type TEXT NOT NULL,
    aggregate_nonce BIGINT NOT NULL,
    event_type TEXT,
    content_type TEXT,
    payload BYTEA NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT uq_aggregate_nonce UNIQUE (aggregate_id, aggregate_nonce),
    CONSTRAINT ck_aggregate_nonce_positive CHECK (aggregate_nonce > 0)
);

-- indexes to speed up queries
CREATE INDEX IF NOT EXISTS idx_events_aggregate_id ON events(aggregate_id);
CREATE INDEX IF NOT EXISTS idx_events_aggregate_id_nonce ON events(aggregate_id, aggregate_nonce);
-- global_position is PK hence indexed

-- immutability: forbid updates and deletes at the DB level
CREATE OR REPLACE FUNCTION forbid_events_update_delete()
RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'events are append-only and immutable';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_events_immutable_update ON events;
DROP TRIGGER IF EXISTS trg_events_immutable_delete ON events;

CREATE TRIGGER trg_events_immutable_update
BEFORE UPDATE ON events
FOR EACH ROW EXECUTE FUNCTION forbid_events_update_delete();

CREATE TRIGGER trg_events_immutable_delete
BEFORE DELETE ON events
FOR EACH ROW EXECUTE FUNCTION forbid_events_update_delete();

-- per-aggregate sequencing: client proposes aggregate_nonce, store validates
CREATE OR REPLACE FUNCTION validate_aggregate_nonce()
RETURNS trigger AS $$
DECLARE
    last_nonce BIGINT;
    expected BIGINT;
BEGIN
    SELECT MAX(aggregate_nonce) INTO last_nonce FROM events WHERE aggregate_id = NEW.aggregate_id;
    IF last_nonce IS NULL THEN
        expected := 1;
    ELSE
        expected := last_nonce + 1;
    END IF;

    IF NEW.aggregate_nonce <> expected THEN
        RAISE EXCEPTION 'invalid aggregate_nonce %, expected % for aggregate %', NEW.aggregate_nonce, expected, NEW.aggregate_id;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_events_aggregate_seq ON events;
CREATE TRIGGER trg_events_aggregate_seq
BEFORE INSERT ON events
FOR EACH ROW EXECUTE FUNCTION validate_aggregate_nonce();
