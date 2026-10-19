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

On 19 October 2026, deployment `4d56e30f-a989-4de4-be9b-60b70d03d430`
published clean source `d4a0590d` at <https://app.tryzoen.com> and
<https://zoen-web.agenttironi.workers.dev>. All ten hosting checks, exact SPA bytes,
signing-worker integrity and WASM bytes passed on **each** origin. Receipt:
`/private/tmp/zoen-app-domain-v96/release-07-public/acceptance.json` (SHA256
`306b367117f5e39a8bc0ac05abfb1a692cf88abd4e243ad2a6df8c257aa9541b`).
This fresh production build includes canonical Web Excalidraw and the measured
miniapp header inset. Public worker integrity is
`sha384-dYc1Ds6ChZA3cp5LAy6/3BNbEuwGze2+MlxwJotYmJoXyigQWBHsMy9h5GHmbjwN`.
The WASM is 25,876,496 bytes, below the 25 MiB individual asset limit. Previous
release06 and its evidence remain separately retained.

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

Separate [CI run 39201639333](https://github.com/EnzoTironi/zoen-native/actions/runs/39201639333)
passed all 19 browser checkpoints in each locale on merge
`d126484348aded89d131b018933545dd858374c2` for head `db60c33e`;
artifact `12163494249` is retained in
`/private/tmp/zoen-cloudflare-web-v96/ci-db60-evidence`. The later local 22-step
journey includes actual canvas edits, cold reload and canonical peer byte comparison;
its final app check passed 795 tests. That successor and the new Directory CI step
still require their own CI run. Android Release failed separately on R8 heap
exhaustion; this is not a whole-workflow pass. See
[Web acceptance](../../docs/dev/expo-web-acceptance.md) for source boundaries.

Actual empty-profile browsers on **both** public origins rendered Portuguese
onboarding, the friendly connection error and system Retry font without unhandled
page errors or an injected analytics script. Root documents remained isolated;
sign-in documents remained popup-safe. Receipts and screenshots are under
`/private/tmp/zoen-app-domain-v96/release-07-public/{canonical,workers-preview}/`.
Both providers remain unavailable against the dead configured relay; this does not
establish successful WASM initialization or auth/sync.

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

The separately deployed [private Person/device directory](relay-state/DIRECTORY.md)
keeps registration, globally unique handles/devices, sponsored linking, one-time
boxes and revocation in one SQLite transaction domain. Its shared Rust policy passed
40 tests, 33 real native PostgreSQL scenarios and nine workerd cases. Private version
`759ffee3-0ef9-4ff5-a53b-1c971a3e2197` uses clean source `46128a65`; it has no public
routes. Those local scenarios and private provisioning are distinct; no remote
Directory journey was run.

The public relay still needs Directory/Space integration and final delivery
authorization, provider proofs, backup recovery, agents, key-package claims, MLS
state and WebSockets. Encrypted media in private R2 still requires its separate
activation/terms confirmation. Existing OAuth Workers, other DNS and R2 activation
are unchanged.

The `expo` workflow runs the actual local workerd storage tests without account
credentials or deployment. Public hosting and encrypted browser journeys are
separate acceptance checks; neither replaces relay protocol validation.
