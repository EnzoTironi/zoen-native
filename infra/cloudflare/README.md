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

On 19 October 2026, deployment `30341e79-f049-4c4c-88b9-bb5fb4942fbf`
published at <https://zoen-web.agenttironi.workers.dev>. All ten public hosting
checks and exact signing-worker integrity passed; receipt:
`/private/tmp/zoen-cloudflare-web-v96/live-hosting-03.json`. This fresh production
build includes the decorative-sky accessibility correction and localized sign-in
connection failure. Public worker integrity is
`sha384-5lhNMH1yUGXCJcds6hQwwqxce9rFjyYH5ABCkzRUpP70btnwC/2vOLvpz4P6rhtw`.

Separate [CI run 39194529436](https://github.com/EnzoTironi/zoen-native/actions/runs/39194529436)
passed all 19 browser checkpoints in each locale on merge
`10276df12927fd6c562ca9e64920089224639770` for head `e2b0ea07`;
artifact `12161960284` is retained locally in
`/private/tmp/zoen-cloudflare-web-v96/ci-e2b-evidence`, with `ci-e2b-receipts.json`.
It includes the decorative attribute correction and uses its own pinned asset
bytes. The later ten-file change `762ceb51` has a separate 795-test full app check
at `/private/tmp/zoen-web-v96/final-app-check.json`. These are distinct source and
artifact boundaries; see [Web acceptance](../../docs/dev/expo-web-acceptance.md).

The actual guest-browser check belongs to the preceding deployment
`bd443adb-9461-452d-9c30-9a4f4005c23a`: onboarding rendered, root/sign-in isolation
matched policy and unhandled page errors were empty. Provider controls remained
unavailable, with a handled challenge-fetch error against the unavailable relay.
Its receipt/screenshot are under `/private/tmp/zoen-web-v96/public-preview/`.
It establishes neither WASM initialization nor auth/sync, and predates the friendly
error-message fix; the latest deployment has HTTP/integrity evidence only.

The preview does not establish provider login or public encrypted sync. The
configured production origin is `https://app.tryzoen.com`, and the configured relay
is `https://relay.tryzoen.com`; the latter currently resolves through an unavailable
Fly application. No custom domain, DNS record or provider origin was changed by
this deployment. Connect the healthy Cloudflare relay and configure the canonical
app domain before accepting production OAuth flows.

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
