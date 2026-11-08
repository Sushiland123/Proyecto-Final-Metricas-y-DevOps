-- Additive migration: the legacy generic events API remains unchanged.
CREATE TABLE devices (
    id VARCHAR(64) PRIMARY KEY,
    facility_id VARCHAR(64) NOT NULL,
    temperature_min DOUBLE PRECISION NOT NULL CHECK (temperature_min >= -80),
    temperature_max DOUBLE PRECISION NOT NULL CHECK (temperature_max <= 150),
    humidity_min DOUBLE PRECISION NOT NULL CHECK (humidity_min >= 0),
    humidity_max DOUBLE PRECISION NOT NULL CHECK (humidity_max <= 100),
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    CHECK (temperature_min < temperature_max),
    CHECK (humidity_min < humidity_max)
);
CREATE INDEX devices_facility ON devices(facility_id, id);

-- Durable inbox and authoritative unique-event ledger. No external broker required.
CREATE TABLE readings (
    id UUID PRIMARY KEY,
    device_id VARCHAR(64) NOT NULL REFERENCES devices(id),
    event_id VARCHAR(128) NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    received_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    temperature DOUBLE PRECISION NOT NULL CHECK (temperature BETWEEN -80 AND 150),
    humidity DOUBLE PRECISION NOT NULL CHECK (humidity BETWEEN 0 AND 100),
    state VARCHAR(16) NOT NULL CHECK (state IN ('running', 'stopped', 'fault')),
    temperature_min DOUBLE PRECISION NOT NULL,
    temperature_max DOUBLE PRECISION NOT NULL,
    humidity_min DOUBLE PRECISION NOT NULL,
    humidity_max DOUBLE PRECISION NOT NULL,
    processed_at TIMESTAMPTZ,
    UNIQUE(device_id, event_id)
);
CREATE INDEX readings_device_time ON readings(device_id, occurred_at, id);
CREATE INDEX readings_pending ON readings(received_at, id) WHERE processed_at IS NULL;

-- One row per authorized, bounded reception attempt, not just per unique event.
-- No raw body or credentials: rejection evidence is a safe reason code.
CREATE TABLE reception_receipts (
    id UUID PRIMARY KEY,
    device_id VARCHAR(64),
    reading_id UUID REFERENCES readings(id),
    outcome VARCHAR(16) NOT NULL CHECK (outcome IN ('accepted', 'duplicate', 'rejected')),
    reason VARCHAR(32),
    received_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);
CREATE INDEX receipts_device ON reception_receipts(device_id, received_at, id);

CREATE TABLE business_alerts (
    id UUID PRIMARY KEY,
    reading_id UUID NOT NULL REFERENCES readings(id),
    kind VARCHAR(32) NOT NULL CHECK (kind IN ('temperature_low', 'temperature_high', 'humidity_low', 'humidity_high')),
    value DOUBLE PRECISION NOT NULL,
    threshold DOUBLE PRECISION NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    UNIQUE(reading_id, kind)
);
