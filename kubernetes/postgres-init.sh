#!/usr/bin/env bash
# Runs only on an EMPTY PostgreSQL volume, before application migrations.
# Never enable shell tracing or print the password variables.
set -euo pipefail
: "${EVENT_MIGRATION_PASSWORD:?missing migration password}"
: "${EVENT_RUNTIME_PASSWORD:?missing runtime password}"
psql --username="$POSTGRES_USER" --dbname="$POSTGRES_DB" --set=ON_ERROR_STOP=1 \
  --set=migration_password="$EVENT_MIGRATION_PASSWORD" \
  --set=runtime_password="$EVENT_RUNTIME_PASSWORD" --set=database="$POSTGRES_DB" <<'SQL'
SELECT format('CREATE ROLE eventos_migrator LOGIN PASSWORD %L', :'migration_password') \gexec
SELECT format('CREATE ROLE eventos_runtime LOGIN PASSWORD %L', :'runtime_password') \gexec
ALTER DATABASE :"database" OWNER TO eventos_migrator;
REVOKE CREATE ON SCHEMA public FROM PUBLIC;
GRANT USAGE, CREATE ON SCHEMA public TO eventos_migrator;
GRANT USAGE ON SCHEMA public TO eventos_runtime;
ALTER DEFAULT PRIVILEGES FOR ROLE eventos_migrator IN SCHEMA public
  GRANT SELECT, INSERT, UPDATE ON TABLES TO eventos_runtime;
ALTER DEFAULT PRIVILEGES FOR ROLE eventos_migrator IN SCHEMA public
  GRANT USAGE, SELECT ON SEQUENCES TO eventos_runtime;
SQL
