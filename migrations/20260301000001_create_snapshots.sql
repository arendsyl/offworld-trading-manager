CREATE TABLE IF NOT EXISTS game_snapshots (
    id             BIGSERIAL PRIMARY KEY,
    saved_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    label          TEXT,
    galaxy         JSONB NOT NULL,
    players        JSONB NOT NULL,
    ships          JSONB NOT NULL,
    projects       JSONB NOT NULL,
    trade_requests JSONB NOT NULL,
    orders         JSONB NOT NULL,
    last_prices    JSONB NOT NULL
);
CREATE INDEX ON game_snapshots (saved_at DESC);
