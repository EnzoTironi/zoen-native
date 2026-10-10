#!/usr/bin/env bash
# Ubuntu 24.04 hosted runners include PostgreSQL 16, disabled by default:
# https://github.com/actions/runner-images/blob/main/images/ubuntu/Ubuntu2404-Readme.md#postgresql
set -euo pipefail

if [[ "${GITHUB_ACTIONS:-}" != true || "${RUNNER_OS:-}" != Linux ]]; then
  echo "ci-postgres.sh requires an isolated GitHub Actions Linux runner" >&2
  exit 1
fi
: "${GITHUB_ENV:?GitHub Actions environment file is required}"

for pg_ci_command in pg_lsclusters pg_conftool pg_isready psql sudo systemctl timeout; do
  command -v "$pg_ci_command" >/dev/null || { echo "Missing runner PostgreSQL tool: $pg_ci_command" >&2; exit 1; }
done
pg_ci_cluster="$(pg_lsclusters --no-header | awk '$1 == "16" && $2 == "main" && $3 == "5432" { print $1 "/" $2 }')"
if [[ "$pg_ci_cluster" != 16/main ]]; then
  pg_lsclusters
  echo "Expected the runner's installed PostgreSQL 16/main cluster on port 5432" >&2
  exit 1
fi

pg_ci_diagnostics() {
  echo "Runner PostgreSQL startup diagnostics:" >&2
  timeout 5s pg_lsclusters >&2 || true
  timeout 5s sudo -n cat /etc/postgresql/16/main/start.conf >&2 || true
  timeout 5s sudo -n pg_conftool 16 main show listen_addresses >&2 || true
  timeout 5s sudo -n pg_conftool 16 main show max_connections >&2 || true
  timeout 5s sudo -n systemctl --no-pager --full status postgresql@16-main.service >&2 || true
  timeout 5s sudo -n journalctl --no-pager --unit=postgresql@16-main.service --lines=60 >&2 || true
  timeout 5s sudo -n tail -n 60 /var/log/postgresql/postgresql-16-main.log >&2 || true
}

# PgCommon leaves digits-and-dots values unquoted: an IPv4 literal is invalid config syntax.
sudo -n pg_conftool 16 main set listen_addresses localhost
sudo -n pg_conftool 16 main set max_connections 300
timeout 60s sudo -n systemctl restart postgresql@16-main.service || {
  pg_ci_status=$?
  pg_ci_diagnostics
  exit "$pg_ci_status"
}
pg_ci_deadline=$((SECONDS + 30))
until pg_isready --quiet --host=127.0.0.1 --port=5432 --timeout=1; do
  if (( SECONDS >= pg_ci_deadline )); then
    pg_ci_diagnostics
    echo "Runner PostgreSQL did not become ready within 30 seconds" >&2
    exit 1
  fi
  sleep 1
done

# Fixture-only credentials; retain the Docker fixture's superuser/CREATE DATABASE rights.
timeout 10s sudo -n -u postgres psql -X -w --port=5432 --dbname=postgres --set=ON_ERROR_STOP=1 <<'SQL'
DO $$ BEGIN
  IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'zoen') THEN
    CREATE ROLE zoen;
  END IF;
END $$;
SET password_encryption = 'scram-sha-256';
ALTER ROLE zoen WITH LOGIN SUPERUSER PASSWORD 'zoen-ci';
SQL

pg_ci_url='postgres://zoen:zoen-ci@127.0.0.1:5432/postgres'
pg_ci_ready="$(PGCONNECT_TIMEOUT=5 timeout 10s psql "$pg_ci_url" -X -w -At --set=ON_ERROR_STOP=1 \
  --command="SELECT current_setting('server_version_num')::int / 10000, current_user, current_setting('max_connections'), rolsuper FROM pg_roles WHERE rolname = current_user")"
if [[ "$pg_ci_ready" != '16|zoen|300|t' ]]; then
  echo "Unexpected PostgreSQL fixture configuration: $pg_ci_ready" >&2
  exit 1
fi
printf 'ZOEN_TEST_PG=%s\n' "$pg_ci_url" >> "$GITHUB_ENV"
echo "Runner PostgreSQL 16 is ready on localhost for the Zoen integration journeys"
