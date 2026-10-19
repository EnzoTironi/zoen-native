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

Four actual dual-wire runs remain retained as failed journeys. The latest passed
520 encrypted publications per runtime and complete initial history pagination,
then exposed missing offline presence. Cold restart was not reached. Presence,
ephemeral receipt replay and device-message fanout still need complete wire and
real-client verification. Two untouched hibernating-socket close assertions also
remain red at their original deadlines; an isolated newer runtime reproduces
the failure.

Real providers, public deployment, push/headless execution, encrypted calls,
remaining media/editor recovery, desktop, accessibility and production operation
remain required. The [operating envelope](zoen-operating-envelope-v1.json) is
declared and unmeasured. SPACE is evaluated only as an isolated executor in the
[detached-agent review](zoen-detached-agents-review.md); no dependency or host
deployment was introduced.
