[Native surface index](../expo-native-surfaces.md).

## iPhone-first installed acceptance

Signed v6 on the disposable iPhone18 Pro/iOS27 simulator accepts ordinary encrypted
linking/history, actual chat rendering and profile photo propagation in both
directions against a fresh current-core primary. Native stock-photo save produced
profile version 2 and an exact matching primary photo hash; the primary update
produced version 3 and visibly replaced the iPhone avatar, name and bio. This
resolves the earlier observed photo-null/version-1 convergence failure.

With Metro stopped, a cold chat deep link opened the real conversation and Back
returned to native tabs. Metro was restored afterward. Per-app PT → EN → PT kept
the same device identity and original literal encrypted history. Native Google
confirmation opened; cancellation returned to onboarding without provisioning an
account. Real provider authentication is a separate remaining setup/acceptance step.

The v5 splash-screen failure was traced through native public logs to missing
`Intl.PluralRules` in Hermes. Registering supported CLDR rules from the existing
OSS i18n-js/make-plural integration fixed startup in v6. A regression removes that
API before executing real English and Portuguese plural messages, without a global
polyfill. iOS money formatting also requires a number-only Intl path: the installed
Hermes tag rejects BigInt there and does not expose `formatToParts` on Apple.
Amount arithmetic remains exact BigInt; only small separator/sign/currency probes
reach the native formatter. Its installed v8 financial journey now passes, as recorded below.

V6 source/artifact manifest SHA256:
`c5ed9e0d2e94775b5a1a5f19a1ca9c971320877554447563788dfd9221914b94`.
JS bundle SHA256:
`8a0b2e32737db608fc665f79ec74916a0107335fb380356dc1cfac617eca5888`.
A 27-second native main-journey clip was recorded. Signed v7 subsequently accepted
the native SwiftUI Appearance Picker; v8 accepted exact money and native UIKit theme
synchronization, as recorded below.

Signed v7 also discovers Zoen in the native Home Screen Widget gallery. Its actual
Portuguese preview displays the privacy description, lock/Zoen branding and the
choose-a-miniapp placeholder without chat, profile or other private content. No
Widget was added and no content preference changed during that check. Selected
content updates, timeline refresh, Live Activities and Siri remain separate journeys.

Signed v8 recovers the exact approved Plan that v7 could not open. The original
certified QA Agent proposal and ordinary one-time owner approval are unchanged.
The corrected confirmed fold binds their exact authenticated opening and receipt,
preserving Agent authorship, history and domain guards. A normal GUI edit saves
`9223372036854775807` BRL cents; two independent canonical peers return that exact
line/total string. Every displayed digit is visible in the iPhone UI. A one-cent
overflow is refused without another version; Metro-off cold reopen retains the
maximum. Normal GUI restoration produces `12345` cents/version 3 on both peers.

The same signed build makes the native glass tab bar dark with white icons when
Dark is selected. A Metro-off cold launch retains the selection; System removes
the override. This uses React Native 0.88's supported app-window appearance API,
not a custom replacement tab bar. Dynamic Type's native-menu fallback still needs
its own runtime acceptance.

V8 source/artifact manifest SHA256:
`e185609cc67bb58599bc30a371e1ac9f5eed7cee2f33b76143b3732e374d4a4e`.
JS bundle SHA256:
`a40bb211968cf234cbef22eede165d66b1f3e73f532094974cd01379565193dc`.
Full native Rust 367/367, focused Plan 71/71, scoped Clippy, Rust formatting and
app TypeScript/Oxlint/Oxfmt plus 123/123 tests pass. These journeys do not establish
real Apple/Google entry, physical-device distribution or complete product parity.

The final signed v9 changes only two rendered Agent captions from v8: it removes
the legacy hardcoded claim that every Agent executes on this device with Apple
Intelligence. A separately owner-authorized Agent principal does not establish
that location or model. Both You and Agent detail now omit that unsupported claim;
owner administration remains available. A Metro-off cold launch accepts both
surfaces and retains the profile photo, native Dark preference and normal
`12345`-cent Plan. The money boundary/overflow matrix was not repeated for this
caption-only delta.

Final v9 source/artifact manifest SHA256:
`e8a6f4180e4aacab89a3f2a3f0541db4800e3a5153606813e9a515d901fc5a9f`.
JS bundle SHA256:
`30242242ae2fddb70e5993c4652e53f34029caa98708bc5f22fb713f074f50da`.
The simulator app contains 136,264,462 bytes. Its source checks and 123 app tests
pass. The earlier functional v8 acceptance and unchanged native Rust artifact
remain identified separately above.
