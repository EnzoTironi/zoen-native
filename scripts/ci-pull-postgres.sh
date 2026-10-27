#!/usr/bin/env bash
# The Docker Official Image's 17-alpine index was verified through registry metadata.
# https://hub.docker.com/_/postgres
# https://docs.docker.com/reference/cli/docker/image/pull/#pull-an-image-by-digest-immutable-identifier
# stdout is only the successfully acquired reference; diagnostics go to stderr.
set -euo pipefail

pg_ci_digest=sha256:266d3c257594e195b309ae9d1805e54c788f1d7b375f56bf6c367aac6c917e7b
for pg_ci_tool in docker timeout; do
  command -v "$pg_ci_tool" >/dev/null || { echo "Missing image acquisition tool: $pg_ci_tool" >&2; exit 1; }
done

# Do not repeatedly hit a quota-exhausted registry. Each distinct source gets
# one bounded attempt at the same immutable image; tests/readiness own their
# existing deadlines independently of this acquisition-only limit.
for pg_ci_repository in \
  docker.io/library/postgres \
  public.ecr.aws/docker/library/postgres \
  mirror.gcr.io/library/postgres; do
  pg_ci_reference="$pg_ci_repository:17-alpine@$pg_ci_digest"
  echo "Acquiring CI PostgreSQL: $pg_ci_reference" >&2
  if ! timeout 60s docker pull --quiet "$pg_ci_reference" >&2; then
    echo "PostgreSQL image source unavailable: $pg_ci_repository" >&2
    continue
  fi
  if ! pg_ci_id="$(timeout 5s docker image inspect --format '{{.Id}}' "$pg_ci_reference")"; then
    echo "Acquired PostgreSQL image cannot be inspected: $pg_ci_repository" >&2
    continue
  fi
  if [[ ! "$pg_ci_id" =~ ^sha256:[0-9a-f]{64}$ ]]; then
    echo "Acquired PostgreSQL image has no valid local image ID: $pg_ci_repository" >&2
    continue
  fi
  printf '%s\n' "$pg_ci_reference"
  exit 0
done

echo "Unable to acquire the pinned CI PostgreSQL image from any source" >&2
exit 1
