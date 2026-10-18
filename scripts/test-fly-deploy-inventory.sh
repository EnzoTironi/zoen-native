#!/usr/bin/env bash
# Exercises the production deployment script with offline CLI doubles only.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
deploy_script="$root/infra/fly/deploy.sh"
if [[ $# -gt 0 ]]; then deploy_script="$1"; fi
scratch="$(mktemp -d /tmp/zoen-fly-inventory.XXXXXX)"
trap 'rm -rf "$scratch"' EXIT
mkdir "$scratch/bin"

cat > "$scratch/bin/fly" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail
record() { printf '%s\n' "$1" >> "$FLY_TEST_TRACE"; }
unavailable() { echo "Synthetic inventory failure" >&2; exit 17; }
case "$1 $2" in
  "apps list")
    [[ "$FLY_TEST_CASE" != app-failure ]] || unavailable
    if [[ "$FLY_TEST_CASE" == missing ]]; then printf '[]\n'
    else printf '[{"Name":"zoen-staging-relay"},{"name":"zoen-staging-pg"},{"Name":"zoen-staging-fdb"}]\n'; fi
    ;;
  "secrets list")
    [[ "$FLY_TEST_CASE" != secret-failure ]] || unavailable
    if [[ "$FLY_TEST_CASE" == missing ]]; then printf '[]\n'
    else printf '[{"Name":"ZOEN_BACKUP_VAULT_KEY"},{"name":"ZOEN_METRICS_KEY"},{"Name":"ZOEN_ADMIN_TOKEN"},{"Name":"POSTGRES_PASSWORD"},{"Name":"AWS_ACCESS_KEY_ID"}]\n'; fi
    ;;
  "volumes list")
    [[ "$FLY_TEST_CASE" != volume-failure ]] || unavailable
    printf '[{"Name":"pgdata"},{"name":"fdbdata"}]\n'
    ;;
  "certs list")
    [[ "$FLY_TEST_CASE" != cert-failure ]] || unavailable
    if [[ "$FLY_TEST_CASE" == missing ]]; then printf '[]\n'
    else printf '[{"Hostname":"relay.tryzoen.com"},{"Hostname":"api.tryzoen.com"},{"Hostname":"media.tryzoen.com"},{"Hostname":"id.tryzoen.com"}]\n'; fi
    ;;
  "certs show") : ;; # Allows the same harness to demonstrate the old regression.
  "apps create") record app-create ;;
  "secrets set") record secret-set ;; # Never record credential arguments.
  "volumes create") record volume-create ;;
  "certs add") record cert-add ;;
  "deploy "*) record deploy ;;
  *) echo "Unexpected mocked Fly operation" >&2; exit 99 ;;
esac
MOCK
cat > "$scratch/bin/openssl" <<'MOCK'
#!/usr/bin/env bash
printf 'synthetic-test-value\n'
MOCK
cat > "$scratch/bin/curl" <<'MOCK'
#!/usr/bin/env bash
printf 'ok\n'
MOCK
chmod +x "$scratch/bin/fly" "$scratch/bin/openssl" "$scratch/bin/curl"

run_case() {
  local scenario="$1" action="$2" expected="$3" result
  : > "$scratch/$scenario.trace"
  if env -i PATH="$scratch/bin:/usr/bin:/bin:/usr/sbin:/sbin" \
    FLY_TEST_CASE="$scenario" FLY_TEST_TRACE="$scratch/$scenario.trace" \
    /bin/bash "$deploy_script" "$action" > "$scratch/$scenario.output" 2>&1; then
    result=success
  else
    result=failure
  fi
  if [[ "$result" != "$expected" ]]; then
    echo "$scenario: expected $expected, got $result" >&2
    exit 1
  fi
  if [[ "$expected" == failure ]] && ! grep -q 'Could not read Fly inventory' "$scratch/$scenario.output"; then
    echo "$scenario: failure did not come from the inventory guard" >&2
    exit 1
  fi
}
expect_count() {
  local scenario="$1" operation="$2" expected="$3" actual
  actual="$(awk -v operation="$operation" '$0 == operation { count++ } END { print count + 0 }' "$scratch/$scenario.trace")"
  if [[ "$actual" != "$expected" ]]; then
    echo "$scenario: unexpected $operation count $actual (wanted $expected)" >&2
    exit 1
  fi
}

run_case secret-failure relay failure
expect_count secret-failure secret-set 0
expect_count secret-failure deploy 0
expect_count secret-failure app-create 0

run_case missing relay success
expect_count missing app-create 1
expect_count missing secret-set 3
expect_count missing deploy 1
expect_count missing cert-add 4

run_case existing relay success
expect_count existing app-create 0
expect_count existing secret-set 0
expect_count existing deploy 1
expect_count existing cert-add 0

run_case app-failure relay failure
expect_count app-failure app-create 0
expect_count app-failure secret-set 0
expect_count app-failure deploy 0

run_case volume-failure postgres failure
expect_count volume-failure volume-create 0
expect_count volume-failure secret-set 0
expect_count volume-failure deploy 0

# Certificate inventory follows the existing deploy; a failed read must not add
# certificates or touch already-present secrets. It does not undo that deploy.
run_case cert-failure relay failure
expect_count cert-failure secret-set 0
expect_count cert-failure deploy 1
expect_count cert-failure cert-add 0

printf 'Fly inventory regression PASS: failed reads abort; confirmed absence provisions; existing secrets remain unchanged (6 offline cases).\n'
