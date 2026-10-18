# Built-in MCP Apps views

These six views use the official MCP Apps App through the existing
`@zoen/miniapp-sdk` and shared `@zoen/ui`. They do not implement another RPC
transport, copy Rust state machines, generate demo state or persist browser state.
All shared changes use the existing item-scoped Rust tools and canonical results.

These are the source views for six core-bundled Rust AppSpecs. The generated HTML
and integrity manifests live in `crates/roda-apps/resources`. The existing official
AppBridge and sandbox host them; Rust tools, grants and native compact cards stay
authoritative. Native acceptance is separate from the source checks below.

| View      | Actual interaction                                                                                                                                                                                                                                                                |
| --------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Pet       | Care/Play/Leaderboard tabs, current native26×22 donkey art, feed, sleep/wake, affection, rename, release with host confirmation and client-only Donkey Dash using the existing Swift physics and score tool. Tab changes pause and retain the local run.                          |
| Poll      | Vote/change vote, add an option, close into the shared plan; votes and closure render canonical results.                                                                                                                                                                          |
| List      | Check/uncheck, add, share into the current chat with `ui/message`, open a prepared WhatsApp URL with `ui/open-link`. Opening never claims delivery.                                                                                                                               |
| MapTap    | Rotatable orthographic globe with the existing public-domain Natural Earth land110 asset, drag/tap and accessible coordinate/rotation controls, canonical per-round score, result arc/distance and group leaderboard. Target marker appears only after the current actor's guess. |
| Recipe    | Canonical servings and ingredient checklist, scaled display quantities, shared cooking steps; explicit local kitchen timer is labeled as an in-view timer, not an OS alarm.                                                                                                       |
| Countdown | Canonical target/date and local ticking display in the device time zone; no invented mutation or calendar API.                                                                                                                                                                    |

The shared hook admits actions at committed mount, serializes double taps and
retains input on denial. It never optimistically changes canonical item data.
Only transient UI selection, form text, game physics and a local kitchen timer
live in React. Hidden game play pauses; listeners/animation/timers are released.

## Build and verify

Each real entry has an explicit manifest. The one shared builder selects and
checks its actual dependency graph; no temporary source copies or second bundler
are involved.

```sh
for app in pet poll list maptap recipe countdown; do
  node tools/miniapp-build/build.mjs miniapps/builtins --manifest "manifests/$app.json"
  node tools/miniapp-build/check.mjs miniapps/builtins crates/roda-apps/resources "manifests/$app.json"
done
node --test miniapps/builtins/test/*.test.mjs tools/miniapp-build/manifest-input.test.mjs
```

All six have the same 768 KiB per-view budget as Schedule. Network origin lists
are empty. List declares only `wa.me` as an outbound link; the host shows its
normal native confirmation. No native capabilities are requested automatically.

Tests execute the generated production HTML, actual shared UI/React/official SDK
and production host AppBridge, with only canonical backend outcomes scripted.
They cover exact tools/arguments, single-flight, canonical peer updates, denied
input retention, actor display-name mapping, coordinate bounds, recipe completion,
no automatic domain writes and real MCP message/link paths. They do not prove
Rust mutation behavior or native WebKit/VoiceOver/layout acceptance.

## Canonical behavior

The six specs all provide standard MCP Apps HTML resources and pinned manifests.
Their old native mutation surfaces are removed from `MiniappState`; that component
still displays historical state without invoking the six apps' tools. Other app
fallbacks are retained. Shared state is never recomputed in the host.

List shares only after an explicit click through standard `ui/message` or
`ui/open-link`; opening WhatsApp makes no delivery claim. The obsolete simulated
send operation/state are removed from the core and its old Android button. Poll
closure rejects an already-closed canonical state before producing a plan effect.

## Native fidelity boundary

The current Swift sources were checked for all six. MapTap reuses the same
orthographic math and coastline bytes; Pet keeps its Care/Play/Leaderboard
structure and current DonkeyArt. Recipe keeps shared checklist/steps plus a
private timer, and Countdown keeps the canonical target with device-zone date.
Poll/List keep their actual canonical interactions. Native host controls such as
Home pinning belong to the host and are not duplicated as invented guest tools.

This source preparation is not a visual/native parity acceptance. The old
Swift-only ambient doodle animation, Pet unboxing and local floating emotes are
not implemented here; native layout, pointer/scroll arbitration, VoiceOver and
reduced-motion behavior still need the coordinator's signed-device review.
