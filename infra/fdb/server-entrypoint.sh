#!/bin/sh
# One-process FoundationDB for staging (single redundancy, ssd engine) on a Fly volume.
# The cluster file names the coordinator by its private DNS name, so the machine's address
# can change without rewriting clients. Production runs fdb-operator (infra/k8s).
set -eu
DATA=/data/fdb
mkdir -p "$DATA/data" "$DATA/logs"
CLUSTER="$DATA/fdb.cluster"
[ -s "$CLUSTER" ] || printf '%s\n' "${FDB_CLUSTER:?set FDB_CLUSTER}" > "$CLUSTER"
IP="${FLY_PRIVATE_IP:?private IP}"
fdbserver --public-address "[$IP]:4500" --listen-address "[::]:4500" \
  --cluster-file "$CLUSTER" --datadir "$DATA/data" --logdir "$DATA/logs" \
  --memory "${FDB_MEMORY:-1536MiB}" --cache-memory "${FDB_CACHE:-256MiB}" \
  --locality-zoneid "$FLY_MACHINE_ID" --locality-machineid "$FLY_MACHINE_ID" &
SERVER=$!
if [ ! -f "$DATA/.configured" ]; then
  until fdbcli -C "$CLUSTER" --timeout 5 --exec "configure new single ssd-2" >/dev/null 2>&1 \
    || fdbcli -C "$CLUSTER" --timeout 5 --exec "status minimal" 2>/dev/null | grep -q "is available"; do
    sleep 2
  done
  touch "$DATA/.configured"
fi
wait "$SERVER"
