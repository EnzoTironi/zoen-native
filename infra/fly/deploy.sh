#!/usr/bin/env bash
# Staging on Fly.io (gru). Idempotent: creates what's missing, then deploys.
#   infra/fly/deploy.sh            everything
#   infra/fly/deploy.sh relay      just the relay image
#   infra/fly/deploy.sh fdb        just FoundationDB
# Needs `fly` logged in. Secrets are generated here and go straight into `fly secrets`;
# nothing is printed or written to disk.
set -euo pipefail
cd "$(dirname "$0")"

ORG="${FLY_ORG:-personal}"
REGION=gru
RELAY=zoen-staging-relay
PG=zoen-staging-pg
FDB=zoen-staging-fdb
BUCKET="${ZOEN_S3_BUCKET:-zoen-staging-media}"
HOSTS=(relay.tryzoen.com api.tryzoen.com media.tryzoen.com id.tryzoen.com)

has_app() { fly apps list --json | grep -qiE "\"(name|Name)\": *\"$1\""; }
has_secret() { fly secrets list -a "$1" --json | grep -qiE "\"(name|Name)\": *\"$2\""; }
ensure_app() { has_app "$1" || fly apps create "$1" --org "$ORG"; }

postgres() {
  ensure_app "$PG"
  fly volumes list -a "$PG" --json | grep -q '"name": *"pgdata"' \
    || fly volumes create pgdata -a "$PG" -r "$REGION" -s 1 -y
  if ! has_secret "$PG" POSTGRES_PASSWORD; then
    local pw; pw="$(openssl rand -hex 24)"
    fly secrets set -a "$PG" --stage POSTGRES_PASSWORD="$pw" >/dev/null
    ensure_app "$RELAY"
    fly secrets set -a "$RELAY" --stage DATABASE_URL="postgres://zoen:$pw@$PG.internal:5432/zoen_relay" >/dev/null
  fi
  fly deploy -c postgres.toml -a "$PG" --ha=false --yes
}

fdb() {
  ensure_app "$FDB"
  fly volumes list -a "$FDB" --json | grep -q '"name": *"fdbdata"' \
    || fly volumes create fdbdata -a "$FDB" -r "$REGION" -s 2 -y
  (cd ../.. && fly deploy . -c infra/fly/fdb.toml --dockerfile infra/fly/fdb.Dockerfile -a "$FDB" --ha=false --remote-only --yes)
}

storage() {
  ensure_app "$RELAY"
  has_secret "$RELAY" AWS_ACCESS_KEY_ID || fly storage create -a "$RELAY" -n "$BUCKET" -o "$ORG" -y >/dev/null
}

# Metrics pseudonym key and the admin token for /admin (ADR 0043). Generated once, staged
# straight into Fly; nobody sees them. Enzo reads the token with `fly ssh console -a
# zoen-staging-relay -C 'printenv ZOEN_ADMIN_TOKEN'` when he wants the dashboard.
# The PostHog project key is staged only if ZOEN_POSTHOG_KEY is set in the environment.
metrics_secrets() {
  has_secret "$RELAY" ZOEN_METRICS_KEY || fly secrets set -a "$RELAY" --stage ZOEN_METRICS_KEY="$(openssl rand -hex 32)" >/dev/null
  has_secret "$RELAY" ZOEN_ADMIN_TOKEN || fly secrets set -a "$RELAY" --stage ZOEN_ADMIN_TOKEN="$(openssl rand -hex 32)" >/dev/null
  if [[ -n "${ZOEN_POSTHOG_KEY:-}" ]]; then
    fly secrets set -a "$RELAY" --stage ZOEN_POSTHOG_KEY="$ZOEN_POSTHOG_KEY" ZOEN_POSTHOG_HOST="${ZOEN_POSTHOG_HOST:-https://us.i.posthog.com}" >/dev/null
  fi
}

relay() {
  ensure_app "$RELAY"
  metrics_secrets
  (cd ../.. && fly deploy . -c infra/fly/relay.toml --dockerfile infra/fly/relay.Dockerfile -a "$RELAY" --ha=false --remote-only --yes)
  for h in "${HOSTS[@]}"; do fly certs show "$h" -a "$RELAY" >/dev/null 2>&1 || fly certs add "$h" -a "$RELAY" >/dev/null; done
}

case "${1:-all}" in
  all) postgres; fdb; storage; relay ;;
  postgres|fdb|storage|relay) "$1" ;;
  *) echo "usage: $0 [all|postgres|fdb|storage|relay]"; exit 2 ;;
esac
echo "▸ https://$RELAY.fly.dev/healthz: $(curl -s -m 10 "https://$RELAY.fly.dev/healthz" || echo unreachable)"
