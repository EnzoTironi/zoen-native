#!/usr/bin/env bash
# Capacity numbers (S8, ADR 0022): release relays on a fresh Postgres database and FoundationDB
# cell, driven by zoen-load over the real protocol. Every simulated person shares this
# machine's address, so the per-address limits are lifted; every other limit stays unless a
# scenario lifts it with LIMITS=<spec> (the hot-Space one does, see the sweep).
#   scripts/bench-load.sh run [--nodes 2] -- <zoen-load args>   one scenario
#   scripts/bench-load.sh sweep                                  the scenarios in ADR 0022
# Needs ZOEN_TEST_PG (an admin URL), FoundationDB (scripts/fdb.sh) and, for --nodes > 1, NATS.
# Results: one JSON per scenario in $OUT (default .dev/bench/<timestamp>).
# RELAY_BIN=<path> runs another relay build (before/after comparisons, ADR 0023).
set -euo pipefail
cd "$(dirname "$0")/.."
: "${ZOEN_TEST_PG:?set ZOEN_TEST_PG=postgres://user@host:port/postgres}"
OUT="${OUT:-.dev/bench/$(date +%Y%m%d-%H%M%S)}"
RELAY_BIN="${RELAY_BIN:-target/release/zoen-relay}"
mkdir -p "$OUT"
eval "$(scripts/fdb.sh env)"

build() {
  cargo build -q --release -p zoen-relay -p zoen-load
}

# run <label> <nodes> <zoen-load args…>
run() {
  local label="$1" nodes="$2"; shift 2
  local db="zoen_bench_$(openssl rand -hex 4)" base="${ZOEN_TEST_PG%/*}"
  psql "$ZOEN_TEST_PG" -qc "CREATE DATABASE $db"
  local pids=() relays=() pidargs=()
  for i in $(seq 1 "$nodes"); do
    local port=$((28786 + i))
    local env=(DATABASE_URL="$base/$db" ZOEN_FDB_CELL="$db" ZOEN_BIND="127.0.0.1:$port"
      ZOEN_BLOB_DIR="$OUT/blobs" ZOEN_DB_POOL=32 LOG_FORMAT=json
      ZOEN_LIMITS="connect_ip=1000000/s:1000000,register_ip=1000000/s:1000000${LIMITS:+,$LIMITS}")
    if (( nodes > 1 )); then env+=(ZOEN_NATS_URL="${ZOEN_NATS_URL:?eval \"\$(scripts/nats.sh env)\"}"); fi
    env "${env[@]}" "$RELAY_BIN" > "$OUT/$label-relay$i.log" 2>&1 &
    pids+=($!); relays+=(--relay "http://127.0.0.1:$port"); pidargs+=(--pid "relay$i=$!")
  done
  for p in "${relays[@]}"; do
    [[ "$p" == --relay ]] && continue
    for _ in $(seq 100); do curl -sf "$p/healthz" >/dev/null && break; sleep 0.1; done
  done
  local fdb; fdb="$(pgrep -f 'fdbserver.*4500' | head -1)"
  pidargs+=(--pid "fdb=$fdb")
  if (( nodes > 1 )); then pidargs+=(--pid "nats=$(pgrep -x nats-server | head -1)"); fi
  local rc=0
  target/release/zoen-load "${relays[@]}" "${pidargs[@]}" --label "$label" --json "$OUT/$label.json" "$@" \
    > /dev/null 2> "$OUT/$label.err" || rc=$?
  local i=0
  for p in "${relays[@]}"; do
    [[ "$p" == --relay ]] && continue
    i=$((i + 1)); curl -sf "$p/metrics" > "$OUT/$label-metrics$i.txt" || true
  done
  kill "${pids[@]}" 2>/dev/null || true
  wait "${pids[@]}" 2>/dev/null || true
  ZOEN_FDB_CELL="$db" target/release/zoen-relay log drop-cell "$db" >/dev/null
  psql "$ZOEN_TEST_PG" -qc "DROP DATABASE $db WITH (FORCE)"
  summarize "$label" "$rc"
  return 0
}

summarize() {
  local label="$1" rc="$2"
  if [[ -f "$OUT/$label.json" ]]; then
    python3 - "$OUT/$label.json" "$rc" <<'PY'
import glob, json, re, sys
r = json.load(open(sys.argv[1])); c = r["config"]; x = r["run"]
counters = {}
for f in glob.glob(sys.argv[1][:-len(".json")] + "-metrics*.txt"):
    for name, v in re.findall(r"^(zoen_relay_\w+_total) (\d+)$", open(f).read(), re.M):
        counters[name] = counters.get(name, 0) + int(v)
batches = counters.get("zoen_relay_append_batches_total")
per_txn = (f', {counters.get("zoen_relay_events_sequenced_total", 0) / batches:.1f} appends/transaction'
           if batches else "")
print(f'{r["label"]:<22} rc={sys.argv[2]} users={c["users"]} group={c["group"]} nodes={c["relays"]} '
      f'rate={c["rate"]:g}/s → accepted {x["accepted_per_s"]:g}/s, deliveries {x["deliveries_per_s"]:g}/s '
      f'({x["deliveries"]}/{x["deliveries_expected"]}), delivery p50 {x["delivery"]["p50_ms"]} ms '
      f'p99 {x["delivery"]["p99_ms"]} ms, ack p99 {x["ack"]["p99_ms"]} ms, '
      f'{x["messages_per_relay_core_second"]} msgs/relay-core-s{per_txn}')
PY
  else
    echo "$label rc=$rc (no report; see $OUT/$label.err)"; tail -3 "$OUT/$label.err"
  fi
}

sweep() {
  # Throughput: 2,000 people in groups of 8, rising rates until latency bends.
  for r in 250 500 1000 2000 4000; do run "rate-$r" 1 --users 2000 --group 8 --rate "$r" --seconds 30; done
  # Fan-out: the same message rate into bigger groups.
  for g in 2 32 128; do run "fanout-$g" 1 --users 2048 --group "$g" --rate 500 --seconds 30; done
  # Connections: 10,000 people online, a trickle of messages.
  run "conns-10k" 1 --users 10000 --group 8 --rate 200 --seconds 30 --connect-concurrency 512
  # One hot Space: sixteen people sending into the same log, the case per-Space sequencing
  # exists for (ADR 0023). Each sends far above the per-device publish limit (20/s), which is
  # lifted so the log is what's measured, not the limiter.
  local hot="publish_device=1000000/s:1000000,publish_account=1000000/s:1000000"
  for r in 1000 2000 4000; do
    LIMITS="$hot" run "hot-space-$r" 1 --users 16 --group 16 --rate "$r" --seconds 30
  done
  # Two nodes over NATS: half the people on each, so most deliveries cross the bus.
  run "two-nodes-1000" 2 --users 2000 --group 8 --rate 1000 --seconds 30
}

case "${1:-}" in
  run) shift; nodes=1
       if [[ "${1:-}" == --nodes ]]; then nodes="$2"; shift 2; fi
       [[ "${1:-}" == -- ]] && shift
       build; run "${LABEL:-run}" "$nodes" "$@" ;;
  sweep) build; sweep ;;
  *) echo "usage: $0 run [--nodes N] -- <zoen-load args> | sweep"; exit 2 ;;
esac
echo "results in $OUT"
