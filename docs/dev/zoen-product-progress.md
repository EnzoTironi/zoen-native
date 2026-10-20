# Current product checkpoint

The full goal remains active: 30 journeys, six platforms and the public backend.
`fullAppParityAccepted`, `productionReadinessAccepted` and
`scaleReadinessAccepted` remain false. This checkpoint records bounded evidence;
it does not replace the [complete goal](zoen-complete-product-goal.md) or its
[acceptance matrix](../../apps/zoen/acceptance.json).

PR136 is merged; [PR140](https://github.com/EnzoTironi/zoen-native/pull/140)
remains draft. The recorded CI snapshot for `783edae1` has ten successes and Rust
still in progress, receipt `0e9cce7c`. It does not qualify the later lifecycle cut.

## Current native link lifecycle (v130)

A genuinely fresh current-783edae1 arm64 iOS Simulator Debug artifact now reproduces
the public-link stack-guard SIGBUS. Build receipt `84f40b98`, runtime `9b14b83f`
and IPS `14e93d39` identify the current artifact and matching UUID
`E272D661-8983-328F-ADA1-596799E50E81`; this is separate from historical v110.
One real secure-link attempt failed after approval. The owned simulator data
was left in place, with no Demo fallback or claim of encrypted history/cold reopen.

The integrated six-path Rust cut passes 24 focused tests: 14 actual pickup
lifecycle cases, 3 original network deadline/cancel cases and 7 existing identity,
history and refusal cases. Strict Clippy and formatting pass; all 446 inputs are
exact through those final checks (receipt `61579643`). The four-path Swift cut
synchronously retires the exact native owner before queued teardown and drains
actual pickup before releasing it. Its Foundation-only strict Swift 6 helper
passes (`3d492fa7`); this is not full Expo close/erase or native runtime proof.
The exact combined live integration is recorded in `live-integration.json`.

Fresh native generation passes in 27.99 seconds. New contract `6e5332fc` has 218
matching Swift checksum calls, with the old 216 unchanged. The full application
check passes TypeScript/Oxlint/Oxfmt and 883/883 tests without failures or skips in
18.45 seconds. The fixed signed Debug artifact is sealed at `cff17697` with new
UUID `19821CD3-3A87-30E9-BF7D-1811DE6FBCB8`, distinct from the failed baseline.
Its 176 native inputs are exact; 909 of 910 broad inputs are stable and the sole SDK
provider transition is reproduced by the canonical generator. Two sealer
assumption failures remain preserved.

The current fixed native app now passes a genuine public secure link on the
retained owned simulator, without Demo fallback. The public primary command
exits 0 after 16.236 seconds and sends 9 history events. A real app-process cold
restart preserves the same account/device and all 5 semantic timeline entries;
the version-3 drawing remains exactly 680 bytes with SHA256 `be5ce5ec…`, matching
the public primary. The real-account fullbleed App sheet renders without a
visible title/status band, dismisses through its native grabber, and returns
the same two Home items with unchanged document bytes. Final runtime receipt
`870a979e` and acceptance `057876a4` bind this bounded pass. The owned simulator
is shut down with data retained and its Metro is stopped. This does not claim a
new edit, pending-network save, latency,
whole lifecycle, optimized Release, provider journey or readiness acceptance.

The QA script's early empty-log assertion failed, but the UI confirmation tap
still ran; matching check 775952 was verified afterward. The evidence does not
claim that visual verification preceded that tap. This documentation update is
a later metadata-only delta to the recorded 910-input proof point; it changes
no runtime code or any of its 176 native inputs.

The separate pinned-host C-ABI v3 pair now records four baseline low-stack
stack overflows and two 2 MiB control passes; candidate02 passes 6/6 at measured
508/540/2044 KiB under the unchanged 512/544/2048 KiB caps. The first candidate
used a stale identical baseline binary and is invalid/preserved. Sealed receipt
`86b6e385` binds distinct binaries and the same generated-ABI/signed-WebSocket
harness. Actual worker/drain lease destruction remains a separate lifecycle
test scope. The harness stays outside production, is not a blocker for user
delivery, and does not qualify iOS, Windows or general runtime limits.
Original pre-ABI calibration failures remain preserved separately.

## Retained bounded App UI evidence

A fresh, exclusively owned Android AVD used genuine public enrollment and secure
linking, then actual Excalidraw UI editing, saving and process-cold-reopen in APK
`ba0c8ad1`. Another publicly linked device exported the exact same 680-byte scene,
SHA-256 `be5ce5ec25b32fe0098768d27b8226995a475eea7bcefab99edc35c41afcbb98`.
First/cold editor observations by 20.577/17.093 seconds include automation delay;
these are upper bounds, not phase timings or an A/B speedup. Native source-origin
URL remains unobserved. No identity, keystore or AVD was cloned.

The v128 APK `59839a92` built in 35.20 seconds with all 501 effective inputs exact
before/after. Its first genuine Home-card open shows a maximum editor with native
Close/Details, the retained rectangle, no double-header gap and no underlying
Home ghosting. Details navigation and close/save/reopen were not qualified.
Later diagnostics found transient guest framework/input-service stalls and
UiAutomation failures; the actual system ANR was **Digital Wellbeing**, not
Zoen. One data-preserving guest reboot retained Home/account/App state. Parent
then shut down only owned emulator 5556 with data intact. Protected 5554 and human
iOS/Metro/backend resources were untouched. No universal cold-open failure or
App-source crash cause is established.

The current continuous-photo bar's actual Day/Deepnight pixels, all four routes,
Insets/seam and accessibility remain pending. Earlier flat-color Day/Deepnight
and tab observations are historical only. One prior native popup opened and
immediate Back dismissed it, but Pin to Home did not persist. Reorder and complete
hardware Back behavior remain unaccepted, with no speculative source fix.

Native CI on `1b292c42`, run `39344596621`, passes API35 with 106 tests and no
skips, API28 with 104 passes and two unchanged WebView capability skips. Both
pass all ten mandatory rich-page repeats plus a separate visual run. The original
assertions, deadlines, method and dependency pins remain intact. The earlier
API28 Compose crash did not recur; its cause remains unknown. These standalone
native jobs do not qualify the newer Expo UI/runtime cut.

Before the current v130 native baseline, historical signed Debug v110 also
faulted during public link. Its original UUID/IPS/source attribution remains
separate. Later explicit Demo-only Hike/Excalidraw UI checks retain their local
UI scope; they do not qualify public linking or a current native body.

The v129 titleless/fullbleed iOS App UI now has a separate bounded pass in
explicit Demo mode on the historical signed Debugv110 native artifact. All 12
production JavaScript hashes match its exact 13-path source cut; the remaining
path is test-only. Actual Hand pan retains the editor, native-grabber pull-down
returns Home, and a second rectangle followed rapidly by dismissal/reopen retains
two visible rectangles. Title, Saved band and outer padding are absent; native
sheet corners remain. No held pending-network save phase was induced and no
public file hash was read. The first anomalous Hike/Pet/blank transition sequence
is preserved as inconclusive, not called fixed by the subsequent clean path.
Own iOS simulator/Metro cleanup passed; data remains retained.

Details moves to the existing Home/conversation card menus and management route.
The final integrated cut passes TypeScript, Oxlint, Oxfmt and all 882 app tests
without failures or skips; all 16 inputs are unchanged before/after. It was then
committed byte-exact in `cd578f8c`. The iOS UI receipt's 12 production hashes
precede only the generic network-enable band's `!fill` condition and the existing
Permissions route/helper/tests. Canonical Excalidraw, frame and native-header
branches were unchanged. Permission controls retain resource integrity, exact
post-consent bytes/policy, scope/owner checks and the existing native grant API;
eight causal and seventeen existing consent cases pass. No actual device-grant,
Auth or new native-body flow is established by those source checks. Demo-only UI
also does not qualify public iOS linking.

Prior Web release 48 (24 per locale) retains its exact export/source receipt,
including historical head `2b879f17` and uncommitted-source hashes. New shared
host/Home paths require their own release and persisted pin/reorder recheck.
Prior managed local two-relay/gateway/backup/restore proofs remain unchanged.
No deployment or backend-lifecycle change is part of this checkpoint. Providers,
complete journeys, outgoing Android media, public hosting, PWA offline startup,
rollback, HA and scale remain open.

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
