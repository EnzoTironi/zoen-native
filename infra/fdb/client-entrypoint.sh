#!/bin/sh
# FoundationDB clients rewrite their cluster file when coordinators change, so it must be a
# writable file: seed it from a mounted read-only copy (FDB_CLUSTER_SOURCE, fdb-operator's
# ConfigMap) or from FDB_CLUSTER (the connection string, Fly).
set -eu
if [ ! -s "$FDB_CLUSTER_FILE" ]; then
  if [ -n "${FDB_CLUSTER_SOURCE:-}" ]; then
    cp "$FDB_CLUSTER_SOURCE" "$FDB_CLUSTER_FILE"
  elif [ -n "${FDB_CLUSTER:-}" ]; then
    printf '%s\n' "$FDB_CLUSTER" > "$FDB_CLUSTER_FILE"
  fi
fi
exec "$@"
