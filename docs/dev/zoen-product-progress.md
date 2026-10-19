# Current product checkpoint

The full goal remains active: 30 journeys, six platforms and the public backend.
`fullAppParityAccepted`, `productionReadinessAccepted` and
`scaleReadinessAccepted` remain false. This checkpoint records bounded work;
it does not replace the [complete goal](zoen-complete-product-goal.md) or its
[acceptance matrix](../../apps/zoen/acceptance.json).

PR136 merged into `main` at `edc0e684` after all 12 CI checks passed on
`c05b0bdd`. Continued work is in [PR140](https://github.com/EnzoTironi/zoen-native/pull/140).
The earlier sections below retain their original artifact boundaries.

The new source uses **Apps** in 224 English/Portuguese display strings, including
permissions, widgets, Siri and shared-core messages. Internal IDs, routes and
signed historical content remain unchanged. All 836 app checks pass. The Web
attachment menu now uses a browser-native popover anchored to the composer;
actual pointer, keyboard, focus, file chooser and narrow-window checks pass.

Cold relay-offline sending no longer hides the conversation: a pending message's
null event hash previously matched every message's null thread reference. The
driver now resolves only actual thread references. Both browser locales pass
all 24 checkpoints, retaining original IDs and one native-peer occurrence after
reconnect and cold-route reload. This uses the genuine retained WASM; current
Apps-copy WASM and complete offline PWA startup remain separate qualifications.

The managed local gateway also passes actual encrypted messaging and Delivered/
Read across different native processes on unchanged connections. All seven
services are healthy, with the original six processes intact. Its 24 ownership/
lifecycle cases pass; enabled-gateway backup/restore and HA remain unaccepted.
See the receipts in `postMergeCheckpoint` and the [gateway guide](../../infra/local/FRONTDOOR.md).

## One product model and simple names

The 13 shared packages now use purpose-based names, including `miniapps`,
`protocol`, `documents`, `event-log` and `client-core`. The native object is
`Client`, with newly generated `client_core` bindings and `ClientFFI` artifacts.
There are no old-name aliases. Existing signing, encryption and key-derivation
domains retain their exact bytes; immutable evidence retains its original names.
See the [naming policy](naming.md).

Account and demo storage are separate, under app-owned directories. Android
retains no-backup storage; Apple retains backup exclusion. The obsolete local
invite API is removed: actual invitations use the relay and canonical link
builders. Native onboarding uses explicit debug-only QA flags rather than a
broad launch-argument bypass.

Generators clear their owned output directories and verify exact inventories.
Actual stale native-library and Swift-source probes cause regeneration instead
of packaging extra artifacts. Existing installed device data was not deleted.

Prelaunch API changes update callers directly. The Android source verifier now
discovers each revision's actual library manifest and reports historical ABI
changes without a compatibility registry. Current generated-source, Kotlin/JNI
and checksum checks remain blocking. Independently compiled Swift geometry and
motion match the original art corpora; pet pixels and Excalidraw runtime HTML are
unchanged after real producer runs. The 15 source-verifier tests pass.

## Current validation boundary

- Expo SDK 58.0.16 compatibility and Expo Doctor 20/20 passed. Current app checks
  pass TypeScript, Oxlint, Oxfmt and 825/825 tests without skips.
- Renamed client, relay, admission and browser-core targets compile. The actual
  offline-invite regression confirms that no local signed grant is manufactured.
- Fresh Android arm64 bindings and native ABI pass verification. All 123 JVM
  tests and the two unchanged API 35 MCP App runtime flows pass. This is a bounded
  native-host proof; complete Expo Android acceptance remains pending.
- Fresh iOS simulator bindings and XCFramework generation pass, including cache
  rejection of an extra Swift source. The signed Debug app passes onboarding,
  demo Home, an actual Hike MCP WebView and a conversation on its owned simulator.
  Account/demo databases and backup exclusion were verified there. This does not
  certify a current Release artifact or a real provider-login journey.

Current copy removes unnecessary product names from search, account preparation,
invitations and connected-drawing status. A development-client startup URL is
handled as an SDK envelope, without opening or authenticating with its inner URL.
Malformed product links and provider callback shapes retain strict handling.
The native artifact proof predates this small JavaScript follow-up. A separate
current-bundle recheck passes cold/warm development links and explicit demo entry,
with all five card IDs and four conversation message IDs retained. The Expo
development-client link reloads JavaScript; demo selection is session-only.

Miniapp autosaves now appear as compact activity in conversations, with one
current native card and a direct history action. Only adjacent typed updates to
the same item can group; all original event IDs, ordering, delivery states and
human messages remain intact. Actual iOS checks covered 22 drawing updates,
exact search/jump to version 21, read-only history and an existing thread.
The receipt is `/private/tmp/zoen-miniapp-activity-v116/receipt.json`, SHA-256
`62a291dd2e471e76c0f052257519a26f2b2dda555f293628cc35a278fa124fc1`.
This uses the existing native miniapp snapshot, without a scene thumbnail or
new permission grant. Current-source Web and Android runtime qualification remains
separate.

The browser core now exposes accepted item versions and restores from the existing
document fold as typed activity, through the parser and browser driver to the same
conversation projection. Signed replies to activity retain their original thread;
ambiguous item ownership surfaces an integrity warning while preserving human
history and existing saved fork warnings. Receipt catch-up also respects confirmed
membership removal and cannot carry stale marks into a later rejoin.

Current source passes all 828 app checks, all 72 Rust browser-core tests, strict
all-target Clippy and two actual signed Rust DTO-to-parser/driver/projection
qualifications. The receipt is
`/private/tmp/zoen-miniapp-web-v118/rust-verification-v3-1.json`, SHA-256
`f29a98fffbc367d80cb6786b7ce3f5cd8a07ba1f0fabfde75447832e33dea61b`.
These source checks have separate WASM/runtime qualification below; Android
runtime parity remains open. The user's iOS session was untouched.

A fresh release WASM and production Expo Web export now pass an actual local
native-backend journey: public device linking, encrypted history, four accepted
drawing edits, compact activity with a human-message boundary, exact-ID search
and read-only version history. After restarting all six backend services and
opening Chrome with the same profile, the account/device, selected canonical
fields of all seven entries, five projected rows and versions 1–5 matched. The
current four-shape drawing rendered again; native verification retained all 18
signed events. This compares canonical fields and visible scene recovery, not a
raw scene-byte digest or OS-keychain recovery.
Receipt: `/private/tmp/zoen-miniapp-web-v118/session_rw7tefs6/web-runtime-receipt.json`,
SHA-256 `8dbfc2d61997022ae7a19d9f63cac05c8f58a887cd1c924ccdafc2db800a0c19`.
One automation navigation timeout remains recorded with its cause unconfirmed;
restarting only the owned Chrome recovered. Web attachments still use a sheet,
and actual Apple/Google confirmation remains unverified. The owned browser/server
were stopped; the native backend and user's iPhone development session remain.

The existing release Web journey also passes all 22 checkpoints in each of
English and Brazilian Portuguese against that exact retained export and CLI.
It covers linking, encrypted messages/files, Markdown, canonical MCP apps,
Excalidraw and local erasure while preserving the native peer. The retention
check now reveals older virtualized messages through the real conversation
search and requires all three original IDs, authors and complete bodies.
Neither its timeout nor assertions were relaxed. Both runs completed without
uncaught browser errors, rebuilds or service restarts; their processes closed.
Receipt: `/private/tmp/zoen-ci-2908-release-web/final-existing-entrypoint.json`,
SHA-256 `a288d78947174599901e1b21f609153c0e4205e0e8e5020077c7f0f259d83c66`.
This is local release-journey evidence, separate from the broader CI checks.

## Backend and local production environment

The active backend direction is the native Rust server. The custom Cloudflare
Workers/Durable Objects port is retired; no further adapter work or cloud
activation is planned. Cloudflare can continue to serve DNS, static assets or
S3-compatible encrypted storage without changing the server runtime.

The local environment now runs the native relay with real PostgreSQL 17,
FoundationDB 7.3.71 and private SeaweedFS S3 storage. Its actual public Client
passed seven checkpoints, including receipt replay, encrypted attachment
decryption and recovery-blob readback. Concurrent create-only storage writes,
immutable retries and a real storage restart passed. A cold backup restored into
fresh volumes with the exact images and original keys; all 41 PostgreSQL tables
and sequences, 30 observed FDB Space keys and the actual media/recovery ciphertext
bytes matched. Ten ownership/integrity checks and 151 unique shared/native tests
passed, with strict backend Clippy. The current native Client receipt guards also
pass all 13 targeted tests and strict all-target Clippy. One exploratory backend
benchmark is excluded.

The environment guide is [infra/local/README.md](../../infra/local/README.md).
Runtime evidence is `/private/tmp/zoen-native-deploy-v118/final-seal/acceptance.json`,
SHA-256 `a5fa24612efefc755dbe6ecb2a087e8469424a7c66545244ca44ae3ca660a756`;
the policy/integration receipt is
`/private/tmp/zoen-native-policy-v119/final-receipt.json`, SHA-256
`9b29d88f53fe04916e9b2254839606391828d0644a227c9c428bb7df83e15071`.
The Client completed before the external restart/restore. Subsequent datastore
and ciphertext comparisons do not establish reopening that Client or recovering
an OS keychain. Two relays and NATS are healthy; cross-process delivery, HA,
release rollback and capacity remain unverified. Browser QA uses the same canonical
relay URL on both sides; pairing deliberately rejects a different relay URL.

The full goal still requires public deployment and final platform artifacts;
local success does not establish production or scale acceptance.

The pending Cloudflare adapter/oracle changes were archived before scoped
removal. The committed custom adapter source is also removed, retaining the
native cryptographic policy and static Web tooling. Native receipts now retain
exact unsigned watermarks with bounded keyset replay and tested SQL flush/query
races. Current membership controls every replay page and live receipt audience;
removed identities cannot seed new marks that later reappear on rejoin.
The simulator and development server handed to the user remain separate protected
resources.

Earlier Cloudflare results retain their original scope and source attribution
in the acceptance matrix and commit `1401c3e9`. They are historical adapter
proofs, not the current hosting direction. The original receipt remains
`/private/tmp/zoen-ordinary-presence-v114/acceptance.json`, SHA-256
`f5d9e83a79e7319af9050b601a728233b75b4c15dc4214cc5adbb0c1df3046d0`.

Real providers, public deployment, push/headless execution, encrypted calls,
remaining media/editor recovery, desktop, accessibility and production operation
remain required. The [operating envelope](zoen-operating-envelope-v1.json) is
declared and unmeasured. SPACE is evaluated only as an isolated executor in the
[detached-agent review](zoen-detached-agents-review.md); no dependency or host
deployment was introduced.
