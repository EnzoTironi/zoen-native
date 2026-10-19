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
  pass TypeScript, Oxlint, Oxfmt and 815/815 tests without skips.
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

## Backend and remaining work

Private Cloudflare source composes Directory authentication with authoritative
Space membership, byte-bounded history reads, canonical publication receipts,
reference outboxes and current-device send fences. The public routes remain
disabled. Earlier native/WASM/SQLite proofs retain their version-specific
receipts; package renaming does not make those whole-product acceptance.

The ordinary WebSocket adapter now passes the original Hello/Auth idle and
256-connection capacity checks at their unchanged five-second deadlines. Its
handle registry stores no account or session authority. Directory retains all
device/session admission and actual-write fences; Space retains membership.
Closing sockets keep their reservation until actual transport termination. One
shared alarm scheduler serializes installation and recomputes pending work after
awaits. The explicit binary type follows the current
[Cloudflare WebSocket API](https://developers.cloudflare.com/workers/runtime-apis/websockets/#binary-messages).

The current local checkpoint passes 147 shared Rust tests, strict Clippy,
28 presence/registry/scheduler unit tests and 48 actual workerd Directory cases.
The runtime cases include genuine native signed authentication/registration,
actual abrupt process death followed by persisted-orphan retirement and fresh
authentication, and recovery of a real close-callback SQL failure through the
next actual durable alarm. Separate closed-group publication/read suites pass.

The first complete native/candidate wire baseline now passes with two real
accounts and a second linked device: actual HPKE enrollment, Direct/Group MLS,
520 encrypted messages crossing history pages, profile/presence, removal/rejoin,
duplicate publication, unlink fences and complete history recovery after real
server/client SQLite reopen. The comparison retains 19,054 raw frame rows and
only the documented nonce/clock and distinct-device stock-notice ordering rules.
It does not filter missing frames. All task-owned services stopped and exact
test namespaces were verified clean. No protected shared service was reset.

The bounded receipt is `/private/tmp/zoen-ordinary-presence-v114/acceptance.json`,
SHA-256 `f5d9e83a79e7319af9050b601a728233b75b4c15dc4214cc5adbb0c1df3046d0`.
The four earlier failed wire runs and first integration failure remain retained.
The old hibernation diagnosis remains historical evidence, rather than a runtime
fallback. Ephemeral receipt replay, device-message fanout, final processing
credit, slow-reader/flood and concurrent fault cases still require completion.
The portable checked-in form of the standalone wire oracle is being prepared;
the checked-in Directory verifier already includes its lifecycle cases.

Real providers, public deployment, push/headless execution, encrypted calls,
remaining media/editor recovery, desktop, accessibility and production operation
remain required. The [operating envelope](zoen-operating-envelope-v1.json) is
declared and unmeasured. SPACE is evaluated only as an isolated executor in the
[detached-agent review](zoen-detached-agents-review.md); no dependency or host
deployment was introduced.
