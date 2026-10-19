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
| `cargo test --locked -p client-core --lib recovery_ui -- --test-threads=1` | 3 passed. Mock HTTP upload failure/retry reopens the same encrypted backup key with the confirmed recovery key and rejects a wrong key; malformed input is rejected; password mode is refused without an open request. 317 unrelated tests filtered out. Not an actual relay/lost-device journey. |
| Same malformed-key test with the original permissive parser temporarily restored | Failed at `backup::recovery_keys(&key).is_err()`. Restoring the fix made the same test pass. No key value was printed. |
| `cargo clippy --locked -p client-core --lib --tests -- -D warnings` | Passed. |
| `cargo fmt --all --check`; `git diff --check` | Passed. |
| `swiftc apple/Shared/Features/Settings/RecoveryKeyInput.swift apple/Tests/RecoveryKeyInputJourney.swift -o /tmp/zoen-recovery-input && /tmp/zoen-recovery-input` | Passed full-key confirmation, wrong/malformed key and secure relay-origin checks with synthetic fixtures. |
| `npm ci --ignore-scripts`; `npm run typecheck:web`; `npm run test:web` (in `packages/zoen-ui`) | Passed; all 13 web tests passed. |
| Native bindings | Regenerated Swift and Kotlin with `target/debug/uniffi-bindgen generate --library target/debug/libclient_core.dylib`, language-specific output and Android config, plus `scripts/fix-android-uniffi-callbacks.py`. Generated artifacts are ignored build output, not hand-edited source. All three new methods appear in both generated interfaces. |
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

At the original delivery, PR #121 was **draft**, conflicted against main and had no
CI checks. The original base was `3804b61`; observed main was `502a03f`.
Reconciliation was initially withheld under the no-merge/no-rebase restriction.
The subsequent explicitly authorized integration is recorded below; hosted PR/CI
state must not be inferred from local checks.

Actual loss of all
original devices, restored peer messaging/history, successful full-key UI confirmation,
UI failure/retry/cancellation under native lifecycle interruption, iPhone/Android builds,
physical-device execution and browser-after-native-recovery remain unverified.

Devices ownership was coordinated with front f: this change avoids Devices sections,
pickup helpers, pairing/session behavior and device requests. No changes were made to
revoked-device admission, #97, #98, #103/#104, #105, #109, #112/#113/#114/#117, or merges
in the original delivery.

## Authorized main integration

The requester subsequently authorized a merge commit from `origin/main`, without
rebasing, force-pushing or merging the PR. Integrated main: `14167d6c`.
The published branch and inherited local branch had identical trees but different
commit identities; the published ancestry was retained before starting the merge.

Only Android `ui/Onboarding.kt` conflicted. Resolution keeps the recovery action
available before account creation and preserves main's `GlassOutlinedButton` invite
control, clipboard validation and route handling. Other main changes were merged
without feature edits. No generated bindings were hand-edited.

Fresh local checks on the integrated tree:

- `python3 scripts/test-recovery-ui.py`: **passed**, 5 source-wiring guards.
- `cargo test --locked -p client-core --lib recovery_ui -- --test-threads=1`:
  **passed**, 3 regressions, 321 unrelated tests filtered out.
- `cargo clippy --locked -p client-core --lib --tests -- -D warnings`: **passed**.
- `cargo fmt --all --check` and staged/unstaged `git diff --check`: **passed**.
- `cargo build --locked -p client-core -p uniffi-bindgen`: **passed**.
- Swift and Kotlin bindings regenerated from that host dylib using
  `target/debug/uniffi-bindgen generate --library target/debug/libclient_core.dylib`,
  language-specific output in ignored `build/recovery-bindings`, Kotlin config
  `android/uniffi.toml`, and `scripts/fix-android-uniffi-callbacks.py`.
- Swift recovery-input journey: **passed** using the command in the original table.
- `npm ci --ignore-scripts`, `npm run typecheck:web`, `npm run test:web`:
  **passed**, 26 web tests.

Hosted PR reads failed at the Reason integration dispatch layer during this
continuation; these local results do not establish a green CI run. Android remains
**not run** locally: `java -version` still reports no Java runtime. The full Apple
build was **not rerun** in this integration; its earlier partial result is recorded
above. No simulator/emulator or native runtime journey was run. No new visual
capture was made: this reconciliation preserves the existing controls rather than
introducing a new presentation. Prior visual evidence retains its stated limits.
Disk remained above 5 GiB free (over 250 GiB at the integration check).

## Key-only password-attempt accounting fix

The previous mode check happened after `/restore/start` had already spent a
password guess. Key-only clients now omit the password OPRF point and send
`recovery_only: true`. Older relays reject the missing point before accounting;
updated relays reject key-only requests against password vaults before password
vault access or accounting, even if a caller supplies a point. Existing password
clients retain their request behavior.

Verification on this fix:

- Temporarily restoring the original client request made
  `cargo test --locked -p client-core --lib recovery_ui_tests::recovery_ui_refuses_password_mode_before_spending_a_guess -- --exact --test-threads=1`
  fail at the new no-OPRF-point assertion. Restoring the fix and running
  `cargo test --locked -p client-core --lib recovery_ui -- --test-threads=1`
  passed all 3 tests.
- The freshly built native unit-test executable with `--test-threads=2` passed
  all 324 tests, zero ignored or failed.
- With isolated PostgreSQL databases and FoundationDB cells,
  `cargo test --locked -p zoen-cli --test journey_backup key_only_restore_never_spends_password_guesses -- --exact --nocapture`
  passed: explicit key-only requests, with and without an OPRF point, leave the
  password guess counter unchanged. Initial local attempts failed to locate the
  fixture binary/library; correcting the absolute build output and FDB library
  linkage made the same test pass.
- `cargo test --locked -p zoen-cli --test journey_backup a_restored_recovery_key_device_rejoins_encrypted_chats -- --exact --nocapture`
  passed the existing real-relay recovered-device encrypted-chat journey.
- After requester-authorized Java 17 installation, Android
  `./gradlew :app:testDebugUnitTest --tests xyz.tironi.zoen.ui.RecoveryKeyInputTest -PcoreProfile=dev -PandroidAbis=arm64-v8a --no-daemon`
  passed both tests, with no skipped tests. This built the Android core and
  regenerated Kotlin bindings through `scripts/build-android-core.sh`; no
  generated bindings were hand-edited.
- The Swift recovery-input journey, 5 Python recovery wiring guards,
  `cargo fmt --all --check` and `git diff --check` passed.

No simulator/emulator, full Apple app build or native UI journey was run in this
fix. The encrypted-chat journey uses the CLI/core against a local relay; it is not
physical-device loss or native lifecycle evidence. No UI presentation changed,
so no new screenshot was captured. Hosted CI is separate from these local checks.
