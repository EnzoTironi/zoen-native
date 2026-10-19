#!/bin/sh
# One-process FoundationDB (single redundancy, ssd engine) on its private persistent volume.
# The cluster file names the coordinator by its private DNS name, so the machine's address
# can change without rewriting clients. Production runs fdb-operator (infra/k8s).
set -eu
DATA=/data/fdb
mkdir -p "$DATA/data" "$DATA/logs"
CLUSTER="$DATA/fdb.cluster"
[ -s "$CLUSTER" ] || printf '%s\n' "${FDB_CLUSTER:?set FDB_CLUSTER}" > "$CLUSTER"
# Use the container's private address locally; Fly supplies its own private IP.
IP="${FDB_PUBLIC_IP:-${FLY_PRIVATE_IP:-$(hostname -i | awk '{print $1}')}}"
MACHINE="${FDB_MACHINE_ID:-${FLY_MACHINE_ID:-$(hostname)}}"
case "$IP" in
  *:*) PUBLIC="[$IP]:4500"; LISTEN="[::]:4500" ;;
  *) PUBLIC="$IP:4500"; LISTEN="0.0.0.0:4500" ;;
esac
fdbserver --public-address "$PUBLIC" --listen-address "$LISTEN" \
  --cluster-file "$CLUSTER" --datadir "$DATA/data" --logdir "$DATA/logs" \
  --memory "${FDB_MEMORY:-1536MiB}" --cache-memory "${FDB_CACHE:-256MiB}" \
  --locality-zoneid "$MACHINE" --locality-machineid "$MACHINE" &
SERVER=$!
terminate() {
  kill -TERM "$SERVER" 2>/dev/null || true
  wait "$SERVER" || true
  exit 0
}
trap terminate TERM INT
if [ ! -f "$DATA/.configured" ]; then
  until fdbcli -C "$CLUSTER" --timeout 5 --exec "configure new single ssd-2" >/dev/null 2>&1 \
    || fdbcli -C "$CLUSTER" --timeout 5 --exec "status minimal" 2>/dev/null | grep -q "is available"; do
    kill -0 "$SERVER" 2>/dev/null || { wait "$SERVER"; exit 1; }
    sleep 2
  done
  touch "$DATA/.configured"
fi
wait "$SERVER"
