-- Compatible with the former Alembic schema. Existing data is never dropped.
CREATE TABLE IF NOT EXISTS events (
    id UUID PRIMARY KEY,
    source VARCHAR(64) NOT NULL,
    external_id VARCHAR(128) NOT NULL,
    type VARCHAR(64) NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    ingested_at TIMESTAMPTZ NOT NULL,
    payload JSONB NOT NULL,
    CONSTRAINT uq_events_source_external_id UNIQUE (source, external_id)
);
-- Fail rather than silently adopting an incompatible pre-existing table.
DO $$
BEGIN
    IF (SELECT count(*) FROM information_schema.columns
        WHERE table_schema = current_schema() AND table_name = 'events') <> 7
       OR (SELECT count(*) FROM information_schema.columns
           WHERE table_schema = current_schema() AND table_name = 'events'
             AND is_nullable = 'NO'
             AND ((column_name = 'id' AND data_type = 'uuid')
               OR (column_name IN ('source', 'type') AND data_type = 'character varying' AND character_maximum_length = 64)
               OR (column_name = 'external_id' AND data_type = 'character varying' AND character_maximum_length = 128)
               OR (column_name IN ('occurred_at', 'ingested_at') AND data_type = 'timestamp with time zone')
               OR (column_name = 'payload' AND data_type = 'jsonb'))) <> 7
       OR NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conrelid = 'events'::regclass
           AND conname = 'uq_events_source_external_id' AND pg_get_constraintdef(oid) = 'UNIQUE (source, external_id)')
       OR NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conrelid = 'events'::regclass
           AND contype = 'p' AND pg_get_constraintdef(oid) = 'PRIMARY KEY (id)') THEN
        RAISE EXCEPTION 'Existing events schema is incompatible; migration aborted without changing data';
    END IF;
END $$;
CREATE INDEX IF NOT EXISTS ix_events_occurred_at_type ON events (occurred_at, type);
