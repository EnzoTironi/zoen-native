# Static Web hosting and edge DNS

Cloudflare serves the Expo static export and edge DNS. The custom Worker/Durable
Object backend port is retired. The current backend is the existing native Rust
server with PostgreSQL, FoundationDB and S3-compatible object storage, developed
locally in the owned [production-like composition](../local/compose.yaml).

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
canvas edits, cold reload and canonical peer byte comparison. The Web artifact `12165466316` is retained with verified source/archive hashes in
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
guest browser checks. Connect and verify the native Rust relay before accepting production
OAuth flows. Provider origins and the separate OAuth integration Worker are unchanged.

## Backend and object storage

The static asset deployment above does not host or qualify authentication, the
relay protocol, receipt persistence or encrypted media. These remain native Rust
server responsibilities. Actual provider confirmation and public backend readiness
require their own evidence; missing provider configuration stays disabled.

The native object-store builder in `crates/zoen-relay/src/blobs.rs` supports an
S3-compatible endpoint through `ZOEN_S3_BUCKET` and `AWS_ENDPOINT_URL_S3` (or
`AWS_ENDPOINT_URL`). R2 can remain a private object store through that interface;
no custom Cloudflare runtime is needed. Existing bucket, DNS and OAuth resource
ownership is unchanged by the repository cleanup. No remote resources are removed
or automatically deployed by it.

Historical private backend proofs belong to their original source and artifacts.
The [immutable pre-retirement guide](https://github.com/EnzoTironi/zoen-native/blob/b4cffb7163888d34e47e470f6bb007672a1ed028/infra/cloudflare/README.md)
retains those dated results. They do not establish native local-stack or public
production acceptance. Static hosting/header/browser checks remain active.
