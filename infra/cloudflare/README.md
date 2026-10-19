# Cloudflare deployment

Node 26 and the pinned shared tooling are required:

```sh
npm ci --prefix infra/cloudflare
```

## Web application

```sh
node scripts/deploy-expo-web.mjs --dry-run
node scripts/deploy-expo-web.mjs
node scripts/expo-web-hosting.mjs apps/zoen/dist --workers
```

The deployment command always builds a fresh production Expo export using
`apps/zoen/config/identity-providers.json`; it never uploads the loopback QA export.
It checks worker/WASM integrity and Cloudflare's 25 MiB asset limit before upload.
`_headers` and `_redirects` retain the signing worker's isolation and the exact
popup-safe sign-in routes. The hosting check exercises the actual Workers runtime,
including document-navigation SPA fallback and ordinary worker/WASM fetches.

On 19 October 2026, deployment `13975cc7-93e5-47f6-9f89-c769f1bd0756`
published at <https://app.tryzoen.com> and
<https://zoen-web.agenttironi.workers.dev>. All ten hosting checks and exact
signing-worker integrity passed on **each** origin; receipt:
`/private/tmp/zoen-app-domain-v96/live-hosting-final.json`. This fresh production
build includes the decorative-sky accessibility correction and localized sign-in
connection failure. Public worker integrity is
`sha384-5lhNMH1yUGXCJcds6hQwwqxce9rFjyYH5ABCkzRUpP70btnwC/2vOLvpz4P6rhtw`.

The exact `app.tryzoen.com/*` Worker Route is fail-closed. Only the two existing
app DNS records changed to Proxied; their values and Auto TTL were retained, with
no deletion. The asset-only Worker serves the application without an origin fetch.
Root, www, relay, auth and mail DNS are unchanged. The reversible before/after
snapshot is `/private/tmp/zoen-app-domain-v96/migration-proxy-acceptance.json`.

The zone's automatic analytics injection initially changed the served HTML.
The app now emits `Cache-Control: public, max-age=0, must-revalidate, no-transform`,
preserving exact document/worker bytes. The real Pages fixture, local Worker and
both public origins passed the strengthened header/exact-body checks. This uses
Cloudflare's documented [no-transform policy](https://developers.cloudflare.com/web-analytics/get-started/),
without changing analytics settings for other sites.

Separate [CI run 39198364287](https://github.com/EnzoTironi/zoen-native/actions/runs/39198364287)
passed all 19 browser checkpoints in each locale on merge
`bfd35da7d52271a332e8b439b69d23850e87a973` for head `7c71cecb`;
artifact `12162482735` is retained locally in
`/private/tmp/zoen-cloudflare-web-v96/ci-7c71-evidence`, with `ci-7c71-receipts.json`.
It includes the decorative attribute correction and uses its own pinned asset
bytes. The included ten-file change `762ceb51` also has a 795-test local full app check
at `/private/tmp/zoen-web-v96/final-app-check.json`. These are distinct source and
artifact boundaries; see [Web acceptance](../../docs/dev/expo-web-acceptance.md).

An actual guest browser on the current canonical domain rendered Portuguese
onboarding and the friendly connection error without unhandled page errors or an
injected analytics script. The sign-in document remained popup-safe. Its receipt
and screenshot are `/private/tmp/zoen-app-domain-v96/final-web/guest-boot.json` and
`canonical-onboarding.png`. Both providers remain unavailable against the dead
configured relay; this does not establish WASM initialization or auth/sync.

The preview does not establish provider login or public encrypted sync. The
configured production origin is `https://app.tryzoen.com`, and the configured relay
is `https://relay.tryzoen.com`; the latter currently resolves through an unavailable
Fly application. Connect the healthy Cloudflare relay before accepting production
OAuth flows. Provider origins and the separate OAuth integration Worker are unchanged.

## Persistent relay storage

See [relay-state](relay-state/README.md) for the privately deployed SQLite Durable
Object, local workerd tests and actual remote synthetic storage check. It preserves
the Rust log's append, dedupe, head and atomic opaque-projection storage contract.
It exposes no public sync or authentication endpoint.

[Shared Rust/WASM admission](admission.md) passed native and actual Node/workerd
checks. The separately namespaced [private closed-group adapter](closed-groups.md)
now uses it for atomic membership/event/invite transactions. Its 29 Rust tests,
17 native differential steps, four no-write negatives, five workerd cases and four
shared storage contracts passed. The deployed private Worker also matched native
bytes across eleven remote steps and retained six events through a second client
session. It exposes no public HTTP endpoint or raw append RPC.

The remaining relay must preserve the complete canonical protocol, transactional
directory/identity/revocation, MLS state and WebSocket delivery. Encrypted media in
private R2 still requires its separate activation/terms confirmation. Existing
OAuth Workers, other DNS and R2 activation are unchanged.

The `expo` workflow runs the actual local workerd storage tests without account
credentials or deployment. Public hosting and encrypted browser journeys are
separate acceptance checks; neither replaces relay protocol validation.
