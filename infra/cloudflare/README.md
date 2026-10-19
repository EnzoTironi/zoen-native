# Cloudflare deployment

Node 26, Rust 1.99.1 and the pinned shared tooling are required:

```sh
npm ci --prefix infra/cloudflare
rustup component add llvm-tools-preview
```

The portable backend uses LLVM’s archiver for WASM C objects; an existing
`AR_wasm32_unknown_unknown` override is retained. Native builds keep their host tools.

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

On 19 October 2026, deployment `359e0cd0-41c5-4e53-b64c-fef8e1b4d0e1`
published clean Web source `891d6d9b` at <https://app.tryzoen.com> and
<https://zoen-web.agenttironi.workers.dev>. All ten hosting checks, exact SPA bytes,
signing-worker integrity and WASM bytes passed on **each** origin. Receipt:
`/private/tmp/zoen-app-domain-v96/release-09-public/acceptance.json` (SHA256
`5c9210eccb0267bd60d59255c830ccdfc0f79fd29097532054e03a8444394ac0`).
This fresh production build includes canonical Web Excalidraw and the measured
miniapp header inset. Public worker integrity is
`sha384-pImaTmN20eB49OhAsS9/4uCr8hZDDmu0NhVTfWUyy5IEBLeeGXkVY+YR8YeYLGlH`.
The WASM is 23,251,769 bytes (SHA256
`f1e077a46b914c8098d9448b42e694fd47bffdc944cb73a553d16c50ae17e096`),
below the 25 MiB individual asset limit. Release builds omit only the debugging
name section; all remaining WASM sections are unchanged. Worker output is identical
from the repository and app directories. Releases07/06 retain their separate evidence.

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

Separate [CI run 39204333306](https://github.com/EnzoTironi/zoen-native/actions/runs/39204333306)
passed all 22 browser checkpoints in each locale on merge
`59998b677ce581bd49a045538ee62f3d17b55347` for head `52da9729`, including real
canvas edits, cold reload and canonical peer byte comparison. Its Directory baseline
passed 33 native PostgreSQL scenarios and nine workerd cases; it predates the
key-package extension below. Artifacts `12165466316` (Web) and `12164869200`
(Directory) are retained with verified source/archive hashes in
`/private/tmp/zoen-cloudflare-web-v96/ci-52da-evidence`. Linux discovered 795 client
tests: 793 passed and two Darwin-only Swift harnesses were skipped; the separate
local macOS check passed all 795. These checks and the later stripped production
artifact have distinct receipts. No Android or whole-workflow result is inferred. See
[Web acceptance](../../docs/dev/expo-web-acceptance.md) for source boundaries.

Actual empty-profile browsers on **both** public origins rendered Portuguese
onboarding, the friendly connection error and system Retry font without unhandled
page errors or an injected analytics script. Root documents remained isolated;
sign-in documents remained popup-safe. Receipts and screenshots are under
`/private/tmp/zoen-app-domain-v96/release-09-public/{canonical,workers-preview}/`.
Both providers remain unavailable against the dead configured relay; this does not
establish successful WASM initialization or auth/sync.

The preview does not establish provider login or public encrypted sync. The
configured production origin is `https://app.tryzoen.com`, and the configured relay
is `https://relay.tryzoen.com`; its challenge request failed DNS lookup in both
guest browser checks. Connect the healthy Cloudflare relay before accepting production
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
boxes, revocation and MLS key-package publication/claims in one SQLite transaction
domain. Reviewed runtime `8bb0ae1e` passed 47 shared Rust tests, three native receipt
regressions, 71 native PostgreSQL scenarios and 18 workerd cases. It was privately
deployed as `1fd4bab4-7013-4749-bb1e-3c59efa146c3`, retaining the existing namespace
and bindings, with no routes, custom domains, workers.dev or previews. Receipt:
`/private/tmp/zoen-cloudflare-admission-v96/validation/directory-keypackages-deployment-acceptance.json`
(SHA256 `4b3cb67beee155ae69e724520805f04f170849aae83f2b2bc312c8633f92067a`).
This provisioning proof adds no remote Directory journey claim. Fresh claims are
bounded to 128 records and 768 KiB canonical response bytes; Rust contexts remain
bounded to 1 MiB. These private limits do not establish full native capacity parity.

The public relay still needs Directory/Space integration and final delivery
authorization, real provider integration, backup recovery, agents, package-refill notifications,
Space MLS epochs/commits/checkpoints/holds and WebSockets. Public media integration
and backup recovery remain separate acceptance gates.

On 19 October 2026, after explicit user authorization for R2, the dashboard already
exposed active R2. Bucket `zoen-private-blobs` was created with Standard storage and
automatic placement (ENAM). Official pinned Wrangler verified zero objects/bytes,
disabled public `r2.dev` access and no custom domains. No subscription agreement,
card change, API token or DNS change was submitted. Receipt:
`/private/tmp/zoen-cloudflare-r2-v100/provisioning-acceptance.json` (SHA256
`bd9864fffc859dfc803bf998b2d64cdc8e6cf4da28032a57a06ef2a8a77ee41b`).
That receipt proves bucket provisioning only. Subsequent [private media acceptance](relay-state/MEDIA.md)
deploys source `86cc9a74` as Directory version `9882fa56-24bb-4573-a592-a50b4c138972`,
preserving its namespace, fixed cell, relay name and migration tag. The added R2
binding is private; routes, workers.dev and preview URLs stay disabled.
Twenty-eight native HTTP vectors, three storage-fault vectors, 13 media workerd
cases and the existing 18 Directory/35 provider cases pass. A real R2 journey passes
seven checkpoints, including native decryption and post-Unlink upload refusal.
Its one 647-byte encrypted QA object was removed; both exact allowlisted keys are
confirmed missing. Small revoked metadata may remain only in the fresh QA cell.
The production fixed cell was not addressed by that journey. Receipt:
`/private/tmp/zoen-cloudflare-r2-v100/remote-acceptance.json` (SHA256
`648f8479451c33f794aa484d75b1a38f7ccf43b53a261c7d4e9a6fa830e7c743`).
Existing OAuth Workers and DNS are unchanged. Public media, transfer, backup recovery
and complete public relay integration remain unaccepted; the two known local
socket-close gates remain red.

The `expo` workflow runs the actual local workerd storage tests without account
credentials or deployment. Public hosting and encrypted browser journeys are
separate acceptance checks; neither replaces relay protocol validation.
The `/private/tmp` receipt paths above are machine-local evidence, not portable CI
artifacts; the linked CI run and artifact IDs preserve their separate attribution.
