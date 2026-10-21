# Owned local code assets

Foreground HTTPS delivery for an explicitly verified App runtime package. Uses
Node built-ins; no installation, dependency, account, document store or daemon is
added. The initial package schema is the existing DeckCraft `schema: 1`,
`zoen-host-v1` module/WASM contract. Other engines need their own reviewed package
contract before this boundary is extended.

The serving pattern is the existing in-memory allowlist from
`miniapps/deckcraft-ui/test/managed-startup-browser.mjs`. Package validation mirrors
`apps/zoen/src/miniapps/runtime-package.ts`, adding loopback-only delivery and an
independently supplied hash of the exact package JSON. App origin, MIME, hash,
authorization, CSP and WebView guards remain unchanged.

## Admission and lifetime

- `--package-sha256` is mandatory. Startup verifies that exact JSON, its strict
  shape, both declared lengths and both SHA-256 hashes before listening.
- `--root` maps the package's complete URL path to disk. Only its two code assets
  are read. Assets must be canonical regular files, owned by the current user,
  without symlinks or group/other write access. They are loaded once into memory;
  later disk changes do not change served bytes.
- Only canonical HTTPS `localhost` or `127.0.0.1` URLs with the configured port
  are admitted. The listener binds **IPv4 `127.0.0.1` only**, default port `9446`.
  There is no alternate bind host, redirect, fallback, filesystem browsing,
  document route, transform, compression or credential handling.
- GET/HEAD use exact raw allowlisted paths and the manifest Host. Unknown paths,
  encoded aliases, query strings and traversal return 404; other methods or
  request bodies return 405. Successful responses have exact MIME/length,
  `no-store, no-transform`, `nosniff` and anonymous `Access-Control-Allow-Origin: *`.
  This CORS policy is only for immutable public code, never private artifacts.
- Explicit certificate/key files are required. The leaf must be current, cover
  the manifest hostname and match the key. The key must have no group/other
  permission bits, for example `0600`. No certificate or trust is provisioned.
- The profile is a canonical, current-user-owned private directory (`0700`). Its
  parent must already exist. `owner.json` binds package/root/origin/certificate;
  `active.json` is an exclusive PID/run-ID lease; bounded `receipt.json` records
  ready/closed/failed state and public hashes. These files use `0600`.
- A changed tuple/certificate requires an explicit **new profile**. Active or
  stale leases are refused; nothing is killed, adopted or silently removed.
  After a crash, inspect the owner, PID and listener manually before removing
  only that proven stale `active.json`. A receipt alone does not prove a live
  listener. The helper does not perform stale-owner recovery.
- SIGINT/SIGTERM stop the foreground listener, destroy idle/incomplete TLS
  connections and release only its own matching lease. No autostart is installed.
  Request paths/headers and key contents are never logged.

## Candidate 04 handoff command

These are **future explicit activation commands**, not an activation performed by
the helper's tests. First review the sealed candidate and independently verify
the package hash. Provision a private parent directory and a project-owned leaf
certificate/key in a separate reviewed step; the helper never creates them.
Example paths below are proposed, and do not imply they already exist.

From the repository root, after that review:

```sh
node tools/code-assets/serve.mjs \
  --package /Users/enzotironi/.codex/builds/zoen-deckcraft-repro-v249/sealed-candidate-04/runtime-package.json \
  --package-sha256 c972da1a80f30c889e6d3ad7d08e8821822bbf96cc98e525459e6d856716169e \
  --root /Users/enzotironi/.codex/builds/zoen-deckcraft-repro-v249/sealed-candidate-04 \
  --cert /Users/enzotironi/.codex/builds/zoen-code-assets-v1/localhost-chain.crt \
  --key /Users/enzotironi/.codex/builds/zoen-code-assets-v1/localhost.key \
  --profile /Users/enzotironi/.codex/builds/zoen-code-assets-v1/candidate-04-service \
  --port 9446
```

Startup prints only the origin and receipt path. Keep this foreground process
owned by the QA session. Inspect the receipt and verify a real trusted GET before
qualifying native behavior. Do not replace an existing listener on 9446.

Candidate 04 is the deliberately new reproducible WASM hash
`cb0af84197762083dc06f9b3812b6c60fe2bbbfeac534ff44c186081dd37b241`
(36,387,042 bytes), with JS hash
`4a3a583c958a150900c58e773c2ed58df6213e733ed3496e9c60c173cd9bebc4`
(157,851 bytes). It is **not** recovery of the missing original `2c6ab9…` WASM.
The server does not promote registry/resource-manifest pins or build native core.
Serving candidate 04 while the app retains the old WASM pin must still fail its
existing loader guard. Promotion and iPhone acceptance are separate work.

## Simulator-only trust plan

In the later activation batch, create a dedicated project-owned QA CA and a leaf
with `DNS:localhost` and `IP:127.0.0.1` SANs. Keep the CA private key separate from
the service; give the service only the leaf chain and protected leaf key.
Record public fingerprints and expiry in that batch's proof. Do not import a
third-party application's certificate or change macOS/global trust.

Host verification uses only that command's explicit CA, for example:

```sh
curl --cacert /Users/enzotironi/.codex/builds/zoen-code-assets-v1/qa-ca.crt \
  https://localhost:9446/deckcraft/a31b990d32f2c8562f1588562f922960c3fde48b/deckcraft-web-1eb288115950a000.js
```

If separately authorized for the existing QA simulator, the scoped trust command
is:

```sh
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer \
  xcrun simctl keychain 591F1216-9B73-4BE1-807A-309DB937D6FA \
  add-root-cert /Users/enzotironi/.codex/builds/zoen-code-assets-v1/qa-ca.crt
```

This is not executed by these sources/tests. The current `simctl keychain` has no
selective removal command; do not reset the existing keychain or erase user data
to undo it. Use a dedicated disposable QA simulator if disposable trust is
required. The code-resource fetch is the existing WK trusted-parent fetch path;
Rust's separate bundled-root TLS client is unchanged. No ATS exception, insecure
fetch, child-network permission or global CA environment variable is added.

## Focused verification

```sh
node --test tools/code-assets/serve.test.mjs
```

Tests require Node and `/usr/bin/openssl` on macOS. They generate an ephemeral
self-signed leaf only inside a test temporary directory, supply it as a CA to
individual real HTTPS requests, use temporary ports/profiles, then close all
listeners and remove the files. They never change host/simulator trust or run a
persistent service. They prove this serving boundary; they do not prove WKWebView,
engine instantiation, native persistence, editor usability or candidate promotion.
