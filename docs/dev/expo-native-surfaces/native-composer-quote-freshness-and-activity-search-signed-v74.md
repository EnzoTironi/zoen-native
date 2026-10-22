[Native surface index](../expo-native-surfaces.md).

## Native composer, quote freshness and activity search — signed v74

V74 source `43ef6b840f189af3b2c021441f7889f9cf560f23` retains the same React Native
UITextView. A native helper fits the exact tagged editor using its real text, font,
width and insets; snapshot/nonce checks reject stale results and retries are bounded.
Sizing changes height without remounting the field or writing text, selection or
focus. Canonical historical ItemEdited notes enter search; quote preview refreshes
independently of the durable normal draft and reply ID.

On accepted v73, the before-diagnostic retained unsent `QA v74 keep` and native
pointer `0x110270c00`: one-line 52 pt → multiline Edit 52 pt clipped → Cancel 73.333 pt.
That failure remains attributed to v73. Actual signed v74 results:

- Isolated preserving install restores that unsent draft. One focused native
  editor `0x110e78c00` fits one-line 52 → Edit 73 → Cancel 52 pt; wrapped text without
  a newline fits 95 → Edit 73 → Cancel 95 pt with exact normal text retained. Actual
  swipe reply updates after its target is edited/deleted; clearing the quote retains
  the draft. One synthetic target `01M57E3QP0XDBNYFCA8CM16WS6` was created, edited
  and deleted. Original seed/Read history remains plus its tombstone; final
  Back/reopen has an empty 52 pt field, no quote and closed keyboard.
- Human `Cilantro` returns 4 actual historical ItemEdited-note results. Selected
  1 → 2 → 1 green bubbles are above the keyboard/composer and below the top HUD;
  search/close keeps the same editor. Home 4 cards, recipe 6/8, six-page history and
  the existing Delivered 11-second voice remain. Final normal draft is empty with
  no quote and closed keyboard; native height is 56.333 pt at its current font,
  not a universal 52 pt claim. No human content mutations. An initial Fixados wait
  mismatched the actual Miniapps section; that tooling diagnostic is retained.

The flag 0 signed Release has 794 inputs, source SHA256
`9e1c0629fe2b7dd7b6a24ab5f2889cbfa7d7d769b78cb2e64dd090f90d528f82`.
Build 97.64 s / minimum free 10,145,005,568 bytes; App 547/547, TypeScript/Oxlint/Oxfmt,
full native FFI 444/444 and scoped strict Clippy pass. The 547 tests include 9 compiled
quote/live-capture cases, 8 fitting-coordinator and 6 compiled field/attachment cases;
Swift 5/6 strict UIKit checks pass. Search 7 and retained voice 2 tests preserve their
source scope. Earlier failed check attempts remain; final6 is the final-source pass.
Both installed executable/JavaScript hashes and strict signatures match frozen v74
after native runtime; 794 source inputs remain verified. Saved proof is
`installed-public-app-checks.json` with both `public-app-after.log` receipts.

Isolated 25 steps / 27 PNGs and human 10 steps / 11 PNGs are retired one-use recordings,
not formal reusable replay/E2E. Evidence: `/private/tmp/zoen-native-composer-qa-v74/`
`isolated-runtime/runtime.json`, `human-runtime/runtime.json`, their observations
and retired YAML; `before-runtime/{observations,verdict}.json`,
`source-checks/final-checks.json`; `/private/tmp/zoen-iphone-release-v74-verification.json`,
`/private/tmp/zoen-v74-app-check-final6.log` and
`/private/tmp/zoen-activity-search-v74/{freeze.json,full-ffi.log}`.
V68 peer/draft/media and v73 positioning acceptance remain exact historical evidence,
not new v74 replays. Six scoped tooling services were stopped for both devices;
the relay was untouched (`scoped-cleanup.json`). No fresh peer/receipt,
physical/provider, broad accessibility/Dynamic Type, performance, hosted current-head
CI or full comparative parity claim.
