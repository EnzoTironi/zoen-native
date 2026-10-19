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

On 19 October 2026, deployment `4546641a-e125-48a7-a2ba-6be2b99939af`
published at <https://app.tryzoen.com> and
<https://zoen-web.agenttironi.workers.dev>. All ten hosting checks and exact
signing-worker integrity passed on **each** origin; receipt:
`/private/tmp/zoen-app-domain-v96/live-hosting.json`. This fresh production
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

Separate [CI run 39194529436](https://github.com/EnzoTironi/zoen-native/actions/runs/39194529436)
passed all 19 browser checkpoints in each locale on merge
`10276df12927fd6c562ca9e64920089224639770` for head `e2b0ea07`;
artifact `12161960284` is retained locally in
`/private/tmp/zoen-cloudflare-web-v96/ci-e2b-evidence`, with `ci-e2b-receipts.json`.
It includes the decorative attribute correction and uses its own pinned asset
bytes. The later ten-file change `762ceb51` has a separate 795-test full app check
at `/private/tmp/zoen-web-v96/final-app-check.json`. These are distinct source and
artifact boundaries; see [Web acceptance](../../docs/dev/expo-web-acceptance.md).

An actual guest browser on the current canonical domain rendered Portuguese
onboarding and the friendly connection error without unhandled page errors or an
injected analytics script. The sign-in document remained popup-safe. Its receipt
and screenshot are `/private/tmp/zoen-app-domain-v96/guest-boot.json` and
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

[Shared Rust/WASM admission](admission.md) passed 20 policy/boundary tests,
the eleven-scenario native FDB contract and actual Node/workerd equivalence checks.
It is not deployed or connected to the storage Worker. The remaining adapter must
preserve the canonical protocol, directory/identity/revocation transactions and
WebSocket behavior. Encrypted media storage in private R2 also remains pending its
separate activation/terms confirmation. Existing OAuth Workers, DNS and R2
activation remain unchanged.

The `expo` workflow runs the actual local workerd storage tests without account
credentials or deployment. Public hosting and encrypted browser journeys are
separate acceptance checks; neither replaces relay protocol validation.
