CREATE FUNCTION fn_event_bus_set_updated_at()
RETURNS TRIGGER AS $$
BEGIN
  NEW.updated_at = CURRENT_TIMESTAMP;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TABLE _pg_events (
    id BIGSERIAL PRIMARY KEY,
    topic VARCHAR(255) NOT NULL,
    payload TEXT NOT NULL,
    -- 1=pending, 2=delivered, 3=failed
    status SMALLINT NOT NULL DEFAULT 1 CHECK (status IN (1, 2, 3)),
    delivered_at TIMESTAMPTZ,
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    max_attempts INTEGER NOT NULL DEFAULT 5 CHECK (max_attempts > 0),
    next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_pg_events_pending
    ON _pg_events (next_attempt_at, id) WHERE status = 1 AND attempts < max_attempts;
CREATE INDEX idx_pg_events_topic_pending
    ON _pg_events (topic, next_attempt_at, id) WHERE status = 1 AND attempts < max_attempts;

DO $$ BEGIN IF NOT EXISTS
 (SELECT 1 FROM pg_trigger WHERE tgname = 'set_updated_at_events') THEN
        CREATE TRIGGER set_updated_at_events BEFORE UPDATE ON _pg_events
        FOR EACH ROW EXECUTE PROCEDURE fn_event_bus_set_updated_at();
    END IF;
END $$;

CREATE TABLE _pg_event_deliveries (
    event_id      BIGINT NOT NULL REFERENCES _pg_events(id) ON DELETE CASCADE,
    handler         TEXT NOT NULL,
    -- 1=pending, 2=delivered, 3=failed
    status          SMALLINT NOT NULL DEFAULT 1 CHECK (status IN (1, 2, 3)),
    attempts        INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    max_attempts    INTEGER NOT NULL DEFAULT 5 CHECK (max_attempts > 0),
    next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_error      TEXT,
    delivered_at    TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (event_id, handler)
);

DO $$ BEGIN IF NOT EXISTS
 (SELECT 1 FROM pg_trigger WHERE tgname = 'set_updated_at_event_deliveries') THEN
        CREATE TRIGGER set_updated_at_event_deliveries BEFORE UPDATE ON _pg_event_deliveries
        FOR EACH ROW EXECUTE PROCEDURE fn_event_bus_set_updated_at();
    END IF;
END $$;

CREATE INDEX idx_event_deliveries_pending
    ON _pg_event_deliveries (next_attempt_at, event_id) WHERE status = 1 AND attempts < max_attempts;
CREATE INDEX idx_event_deliveries_delivered
    ON _pg_event_deliveries (delivered_at) WHERE status = 2 AND delivered_at IS NOT NULL;
