# Recovery-key UI (front g)

## Scope and behavior

- Apple and Android replace password activation with a dedicated recovery-key flow.
  Generation is local and side-effect free. The person must save the key outside the
  device and enter the entire key again before activation performs network writes.
- An upload error keeps the same key in view memory for retry. Closing/recreating the
  view does not persist that key: the person has already confirmed an external copy.
  No clipboard, logging, analytics or saved-instance/defaults storage is added.
  Android dialogs request secure window flags; Apple hides the displayed key while
  the scene is inactive. Confirmation and restore fields are concealed.
- The initial native screen offers recovery before account creation. The dedicated
  core entry point rejects malformed keys before network access and refuses a
  password-mode backup before submitting an open request/consuming a guess.
- Existing password-protected backups are not deleted, rotated or migrated. Backup-now
  remains available. The older combined core/CLI methods are retained; native UI no
  longer calls password activation.
- Web offers plain Portuguese guidance: recover in the native application, then pair
  this browser. It does not accept keys or pretend native backup bytes are a browser
  session. **Browser account recovery is not implemented.**

## Verification

All commands ran in this front's assigned managed laptop checkout; no simulator,
Android emulator, personal checkout, sudo, brew, merge, rebase or force-push was used.
Disk checks remained above 5 GiB (last observed 276 GiB available).

| Check | Observed result / boundary |
| --- | --- |
| `python3 scripts/test-recovery-ui.py` | Baseline: 4 failed assertions and 1 missing-module error. After: 5 passed. These are source-wiring guards, not UI execution. |
| `cargo test --locked -p roda-ffi --lib recovery_ui -- --test-threads=1` | 3 passed. Mock HTTP upload failure/retry reopens the same encrypted backup key with the confirmed recovery key and rejects a wrong key; malformed input is rejected; password mode is refused without an open request. 317 unrelated tests filtered out. Not an actual relay/lost-device journey. |
| Same malformed-key test with the original permissive parser temporarily restored | Failed at `backup::recovery_keys(&key).is_err()`. Restoring the fix made the same test pass. No key value was printed. |
| `cargo clippy --locked -p roda-ffi --lib --tests -- -D warnings` | Passed. |
| `cargo fmt --all --check`; `git diff --check` | Passed. |
| `swiftc apple/Shared/Features/Settings/RecoveryKeyInput.swift apple/Tests/RecoveryKeyInputJourney.swift -o /tmp/zoen-recovery-input && /tmp/zoen-recovery-input` | Passed full-key confirmation, wrong/malformed key and secure relay-origin checks with synthetic fixtures. |
| `npm ci --ignore-scripts`; `npm run typecheck:web`; `npm run test:web` (in `packages/zoen-ui`) | Passed; all 13 web tests passed. |
| Native bindings | Regenerated Swift and Kotlin with `target/debug/uniffi-bindgen generate --library target/debug/libroda_ffi.dylib`, language-specific output and Android config, plus `scripts/fix-android-uniffi-callbacks.py`. Generated artifacts are ignored build output, not hand-edited source. All three new methods appear in both generated interfaces. |
| Native Mac build | Partial: fresh host-only XCFramework and generated Swift were used. Both `RecoveryKeyFlow.o` and `RecoveryKeyInput.o` were produced. Full `xcodebuild -project apple/Zoen.xcodeproj -scheme ZoenMac -configuration Debug -destination 'platform=macOS' -derivedDataPath build/recovery-native CODE_SIGNING_ALLOWED=NO` failed because existing `apple/Shared/Sync/PushBridge.swift:58` references `.ephemeral`, unavailable on macOS. That file is unchanged by this PR and unchanged between the original checkout and observed main. Build/polish is excluded (#105); no fix here. |
| Android build/unit/instrumentation tests | Not run locally: `java -version` reports no Java runtime; no Android SDK was found at the standard path. Generated bindings alone are not ABI/runtime proof. |
| Native UI tests | Existing disabled-restore-placeholder assertions were replaced with stronger key-flow guards: malformed/wrong confirmation cannot activate, no password field, cancel before activation leaves backup off. Tests were updated, **not executed**. |

## Visual evidence

The real `RecoveryHelp` React component was mounted and expanded in the browser;
DOM inspection confirmed zero input controls. An isolated static render of that
same component and its checked-in CSS was captured with headless Chrome through
Playwright, inspected, and published in the PR. The image explicitly labels the
component preview as **not a connected session**. It is not proof of native recovery,
browser pairing, an authenticated web boot, or a cross-device journey.

There are no native screenshots: the full Mac build failed and no simulator/emulator
was allowed. No screenshot contains a recovery key.

## Human blockers / unverified acceptance

PR #121 remains **draft**. GitHub reports a conflict against main and returns no CI
checks. The original base was `3804b61`; observed main was `502a03f`. Changed-file
intersection includes Android `Onboarding.kt` and `Settings.kt`, and Apple
`OnboardingFlow.swift`; GitHub does not identify exact conflict hunks in its snapshot.
No reconciliation was attempted under the no-merge/no-rebase restriction.

A human-authorized integration is required before CI can start. Actual loss of all
original devices, restored peer messaging/history, successful full-key UI confirmation,
UI failure/retry/cancellation under native lifecycle interruption, iPhone/Android builds,
physical-device execution and browser-after-native-recovery remain unverified.

Devices ownership was coordinated with front f: this change avoids Devices sections,
pickup helpers, pairing/session behavior and device requests. No changes were made to
revoked-device admission, #97, #98, #103/#104, #105, #109, #112/#113/#114/#117, or merges.
