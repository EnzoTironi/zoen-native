# Android on current Main

Source checkpoint: 12 October 2026, shared `main` checkout. The merged PR71 revision `ff5b519` is the minimum ancestor, not a frozen Apple UI reference. The current revision and all Core generation inputs are resolved into the verification artifact. Dirty working source has the `working-tree-dirty` label and its own input digest. Earlier [Android native results](android-native-experience.md) and PR41 media retain their original revisions and do not certify this integration.

## Source-gate tools — 19 October 2026

The source verifier reads actual Cargo TOML at each immutable revision. Python 3.11 or newer includes the parser. With macOS's Python 3.9, create a tool environment and install the pinned dependency before running Android build scripts:

```sh
python3 -m venv .dev/python-tools
source .dev/python-tools/bin/activate
python -m pip install -r scripts/requirements-python.txt
python3 scripts/test-android-core-contract.py
```

Keep that environment active for build scripts that invoke `python3`. The verifier never installs packages or resolves Cargo dependencies itself. The historical results below retain their original source and runtime scope.

During prelaunch, internal Rust/UniFFI APIs may change atomically with their consumers. The gate reports the actual Main/current library paths and raw ABI additions, removals and shape changes without historical aliases or a compatibility registry. Current-source generated Kotlin/JNI metadata and checksum validation, Main ancestry and original-art integrity remain blocking. Revisit the API compatibility policy before public launch.

## First Plan unit

The current Plan screen consumes `PlanPageDto` in supplied block order, including prose, exact text and UTF-16 marks. Signed costs use integer cents throughout display and decimal editing. Negative stored costs remain visible; existing legacy writers still reject negative replacements. Duplicate legacy line IDs disable ambiguous controls while distinct document block IDs preserve rendering order. An unrepresentable total has an explicit unavailable label.

Editing, toggle, add, and removal pass the `content_hash` already shown into the Core writer. That hash is the PlanDoc, including while the screen shows an unsaved private draft. Readiness, admission, and that hash are checked under the same Engine lock as the write. A mismatch is stale and appends nothing. Restore captures the hash when the confirmation opens and calls `restorePlanVersion` with it. `PlanPagePresentation` no longer re-reads admission in a separate lock.

Removal has a separate native confirmation. Canonical Plan mutation and historical access retain the Core cutover gate. History and read-only previews expose disabled native controls. The accepted shared document version remains distinct from a locally signed pending legacy version.

`PlanPagePresentationTest` covers ordered blocks, duplicate IDs, signed cents beyond floating-point precision, integer bounds, exact Unicode/line-separator spans and refused or changed admission. `PlanPageJourneyTest` drives native one-cent edit/save, activity recreation, cancelled edit/removal, confirmed removal, every saved history row, read-only preview and restoration as a new signed version. It also checks loss of membership and cold Keystore/database reopening with exact large cents. Its accounts and database are owned test fixtures. These new Kotlin/device cases have not run at this source checkpoint.

## File versions and memory limits

Item sharing, current file preview/export/open-with and historical preview/sharing reuse `readFileVersion`. It pins `fileInfoAt` and `fileBytesBounded` to the visible signed version, so newer name/MIME metadata cannot be paired with older bytes or cached under another version. The Android budget is 20 MiB, checked before JNI transfers a payload; the bounded Core API also validates the document, cache lengths and hashes before allocating or returning it. The new APIs are present in actual Core source and still require the coordinated Kotlin/JNI generation.

Missing chunks produce a pending state with no export/open-with actions, retried when repository state changes. Invalid metadata, damaged cache rows and files over the budget produce an error. Historical sharing stays disabled until that exact version's bytes arrive. Preview image decoding retains its 1600-pixel target; full-resolution image markup export first reads dimensions without allocating pixels and refuses more than 16 megapixels. Large images below the file-byte budget can still be previewed and exported. The limits have English and Portuguese messages.

`FileVersionJourneyTest` covers changed historical filename/MIME and exact payload across cold reopening, refusal over 20 MiB, missing chunks and malformed owned cache rows without changing signed history. Its native journey removes one owned demo chunk, observes the pending screen, restores it and verifies retry in the same version, checks a malformed-cache error, then opens the original history after an encoding/name change. These JNI and device cases run in the scheduled Android workflow on API 28 and 35. Other consumers, including voice transport, are outside this reader migration.

## Android storage and cache fixtures

`ZoenApplication` points Rust `TMPDIR` at the app-owned `cacheDir` before opening the Core. Android backup snapshots and recovery staging can then create temporary databases inside the app sandbox. API 28 rejects the old `/data/local/tmp/zoen-backup-*.db` path during the recovery journey.

The Core owns its SQLite connection exclusively. `FileVersionJourneyTest` closes its fixture engine before changing cache rows with Android SQLite, checkpoints the fixture writes, then reopens the engine. Native demo previews use the same sequence under the repository gate through `withDemoDatabaseClosed`, which is restricted to debug demo databases. This avoids two SQLite implementations writing the same live file and preserves every signed version assertion. Opening a database has no retry loop.

Local validation of the F4 APK before the F7 lifecycle correction passes all four file-version methods on API 28 and 35. Callback contract checks, 112 Android unit tests, and lint also pass. Selected device suites remain partial: recovery reaches backup and restore but fails at immediate cold reopening on both APIs. The Plan history visibility assertion fails on the 320-by-640 API 28 viewport in both main and U1 APKs. All four Approval methods pass in an isolated main API 28 run.

Cold recovery reopening is a separate lifecycle check. Before F7, a fresh API 35 run reaches backup and restore, then reports `sqlite: database is locked` when reopening the recovered device. An isolated Rust probe with no callback or extra Core reference confirms that the network task retains the internal engine after `stop_sync` returns. The original runtime shuts down in the background, so immediate database reopening can still fail. F7 releases network Engine ownership synchronously during teardown through a revocable lease, without waiting for runtime shutdown. See [Network lifecycle](network-lifecycle.md). No delay or storage-error retry is added to the app or journey.

## Source and generated artifact gate

Before building, run:

```sh
python3 scripts/test-android-core-contract.py
python3 scripts/check-android-upstream.py --source-only --output android/app/build/outputs/android-upstream-source.json
```

`--fetch` optionally refreshes only `origin/main`; the Git coordinator owns local fetches. `--main-ref` names an explicit Main revision, resolved into the report. The gate requires both the merged PR71 minimum and current Main ancestry, inventories the actual exported API and preserves original artwork/equivalence checks. Apple source hashes describe the current input snapshot without freezing unrelated UI at PR71. Closed proposal branches are historical evidence only.

The normal `build-android-core.sh` captures the input artifact before compilation, generates Kotlin from the host library, builds the requested JNI ABIs and records `core-contract.json` only when source inputs stayed stable. Every Android ELF must export the complete current UniFFI metadata and checksum inventory. Metadata bytes must agree across ABIs and with the host bindgen input. Kotlin ABI checksum values must agree with the host library. Subsequent verification checks source inputs, profile, ABI architecture, metadata and every generated Kotlin/JNI file hash. Missing generation or changed APIs/artifacts fail. `preBuild` runs this gate even if the producer task is excluded.

The six Python tests passed at this checkpoint. They reject changed cents types, missing APIs, stale same-named DTO metadata, modified native binaries, wrong/missing ABIs and the wrong profile. A source-only comparison passed against current Main with the original art fixtures. The preserved old JNI library fails the new current-API inventory. These checks do not establish device behavior, JNI loading, renderer parity, release packaging or public relay compatibility.

For a separately coordinated batched producer, use the same capture/finish sequence around the actual compilation and generation:

```sh
python3 scripts/android-core-contract.py capture --abis arm64-v8a --profile dev
# Compile the stable Main snapshot and generate Kotlin/JNI through the coordinated producer.
python3 scripts/android-core-contract.py finish --host-library "$CARGO_TARGET_DIR/debug/libclient_core.dylib" --abis arm64-v8a --profile dev
```

The Linux host input ends in `.so`. The `finish` step reads only the host's UniFFI metadata and checksum exports; Android libraries are parsed without executing them.

## Scheduled validation

After the matching coordinated generation, the minimum first-unit build is:

```sh
cd android
./gradlew :app:testDebugUnitTest --tests xyz.tironi.zoen.pages.PlanPagePresentationTest :app:assembleDebug :app:assembleDebugAndroidTest -x :app:buildClientCore -PandroidAbis=arm64-v8a -PcoreProfile=dev
./gradlew :app:connectedDebugAndroidTest -x :app:buildClientCore -PandroidAbis=arm64-v8a -PcoreProfile=dev -Pandroid.testInstrumentationRunnerArguments.class=xyz.tironi.zoen.PlanPageJourneyTest,xyz.tironi.zoen.FileVersionJourneyTest
```

The existing CI matrix retains API28 and API35, both native ABIs, full JVM/lint and owned relay/device journeys. It records the pre-build source report and verifies the generated Core after building. Current-source release, complete device results and actual image/video evidence remain pending. The coordinator captures stable app/test artifacts and attaches real images and video with `gh --attach` to the existing UI review.

## Final source checkpoint

Root reran the unchanged contract-verifier tests: six passed. All 374 Android/workflow/scripts/docs files were frozen for the direct-main checkpoint. Current Kotlin/JNI/device acceptance, Plan atomic writers, bounded voice migration and the pixel-limit fixture remain pending. The new [prelaunch policy](prelaunch-policy.md) requires replacing canonical signatures/callers together rather than adding `From` compatibility aliases. Resume from [the complete handoff](handoff-2026-10-12.md).

## Short viewport history correction (F8)

ItemScreen now opens its shared file/page/plan version sheet expanded with `skipPartiallyExpanded = true`. This removes the partial drag anchor. The observed API28 320-by-640 failure had its version4 row outside the modal root while the list reported zero scroll range; these observations do not establish Material3's internal constraints. No diagnostic hooks are included. Selection, read-only preview and restore admission still use the existing callbacks and gates, with dedicated runtime regression coverage required for the changed opening and dismiss behavior.

`ItemHistoryJourneyTest` uses the real owned demo database and JNI writers, without UI or repository doubles. Independent tests cover a 12-edit signed history with positive scroll range and actual changed scroll position, earliest/latest read-only selection, back, backdrop and downward drag dismissal, and membership loss before opening and while history is already open. The original Plan method and FileVersion shared-component smoke are unchanged. API28 and API35 receipts and actual before/after media live in the separate F8 evidence package; a local passing test does not accept U1 or R41, and F8 requires independent review before reconciliation.
