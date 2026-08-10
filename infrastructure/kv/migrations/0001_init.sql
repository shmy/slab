CREATE UNLOGGED TABLE _pg_caches (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX idx_pg_caches_expires_at ON _pg_caches (expires_at);
