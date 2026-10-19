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

On 19 October 2026, deployment `bd443adb-9461-452d-9c30-9a4f4005c23a`
published at <https://zoen-web.agenttironi.workers.dev>. All ten public hosting
checks and the exact signing-worker integrity check passed. This includes
the decorative-sky accessibility correction verified separately in the local release.

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

The remaining migration keeps signature/admission rules in shared Rust/WASM,
adds the directory/identity/revocation transactions and WebSocket protocol, and
stores encrypted media in private R2. R2 activation is pending the separate
subscription/terms confirmation. Existing OAuth Workers and DNS remain unchanged.

The `expo` workflow runs the actual local workerd storage tests without account
credentials or deployment. Public hosting and encrypted browser journeys are
separate acceptance checks; neither replaces relay protocol validation.
