# Current product checkpoint

The full goal remains active: 30 journeys, six platforms and the public backend.
`fullAppParityAccepted`, `productionReadinessAccepted` and
`scaleReadinessAccepted` remain false. This checkpoint records bounded work;
it does not replace the [complete goal](zoen-complete-product-goal.md) or its
[acceptance matrix](../../apps/zoen/acceptance.json).

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
new permission grant. Current-source Web and Android qualification remains
separate; the browser projection still omits typed item-update events.

## Backend and local production environment

The active backend direction is the native Rust server. The custom Cloudflare
Workers/Durable Objects port is retired; no further adapter work or cloud
activation is planned. Cloudflare can continue to serve DNS, static assets or
S3-compatible encrypted storage without changing the server runtime.

The immediate environment is local, using real PostgreSQL, FoundationDB and
S3-compatible object storage with the native relay container. Persistence,
migrations, health/readiness, restart and recovery must be exercised there.
The full goal still requires public deployment and final platform artifacts;
local success does not establish production or scale acceptance.

The pending Cloudflare adapter/oracle changes were archived before scoped
removal. Native receipt precision, replay paging/race fixes and client receipt
handling remain under validation. The simulator and development server handed
to the user are separate protected resources.

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
