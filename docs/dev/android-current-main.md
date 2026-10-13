# Android on current Main

Source checkpoint: 12 October 2026, shared `main` checkout. The merged PR71 revision `ff5b519` is the minimum ancestor, not a frozen Apple UI reference. The current revision and all Core generation inputs are resolved into the verification artifact. Dirty working source has the `working-tree-dirty` label and its own input digest. Earlier [Android native results](android-native-experience.md) and PR41 media retain their original revisions and do not certify this integration.

## First Plan unit

The current Plan screen consumes `PlanPageDto` in supplied block order, including prose, exact text and UTF-16 marks. Signed costs use integer cents throughout display and decimal editing. Negative stored costs remain visible; existing legacy writers still reject negative replacements. Duplicate legacy line IDs disable ambiguous controls while distinct document block IDs preserve rendering order. An unrepresentable total has an explicit unavailable label.

Editing, toggle, add, and removal pass the `content_hash` already shown into the Core writer. That hash is the PlanDoc, including while the screen shows an unsaved private draft. Readiness, admission, and that hash are checked under the same Engine lock as the write. A mismatch is stale and appends nothing. Restore captures the hash when the confirmation opens and calls `restorePlanVersion` with it. `PlanPagePresentation` no longer re-reads admission in a separate lock.

Removal has a separate native confirmation. Canonical Plan mutation and historical access retain the Core cutover gate. History and read-only previews expose disabled native controls. The accepted shared document version remains distinct from a locally signed pending legacy version.

`PlanPagePresentationTest` covers ordered blocks, duplicate IDs, signed cents beyond floating-point precision, integer bounds, exact Unicode/line-separator spans and refused or changed admission. `PlanPageJourneyTest` drives native one-cent edit/save, activity recreation, cancelled edit/removal, confirmed removal, every saved history row, read-only preview and restoration as a new signed version. It also checks loss of membership and cold Keystore/database reopening with exact large cents. Its accounts and database are owned test fixtures. These new Kotlin/device cases have not run at this source checkpoint.

## File versions and memory limits

Item sharing, current file preview/export/open-with and historical preview/sharing reuse `readFileVersion`. It pins `fileInfoAt` and `fileBytesBounded` to the visible signed version, so newer name/MIME metadata cannot be paired with older bytes or cached under another version. The Android budget is 20 MiB, checked before JNI transfers a payload; the bounded Core API also validates the document, cache lengths and hashes before allocating or returning it. The new APIs are present in actual Core source and still require the coordinated Kotlin/JNI generation.

Missing chunks produce a pending state with no export/open-with actions, retried when repository state changes. Invalid metadata, damaged cache rows and files over the budget produce an error. Historical sharing stays disabled until that exact version's bytes arrive. Preview image decoding retains its 1600-pixel target; full-resolution image markup export first reads dimensions without allocating pixels and refuses more than 16 megapixels. Large images below the file-byte budget can still be previewed and exported. The limits have English and Portuguese messages.

`FileVersionJourneyTest` covers changed historical filename/MIME and exact payload across cold reopening, refusal over 20 MiB, missing chunks and malformed owned cache rows without changing signed history. Its native journey removes one owned demo chunk, observes the pending screen, restores it and verifies retry in the same version, checks a malformed-cache error, then opens the original history after an encoding/name change. These new JNI and device cases are source-only until matching generation and scheduled execution complete. Other consumers, including voice transport, are outside this reader migration.

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
python3 scripts/android-core-contract.py finish --host-library "$CARGO_TARGET_DIR/debug/libroda_ffi.dylib" --abis arm64-v8a --profile dev
```

The Linux host input ends in `.so`. The `finish` step reads only the host's UniFFI metadata and checksum exports; Android libraries are parsed without executing them.

## Scheduled validation

After the matching coordinated generation, the minimum first-unit build is:

```sh
cd android
./gradlew :app:testDebugUnitTest --tests xyz.tironi.zoen.pages.PlanPagePresentationTest :app:assembleDebug :app:assembleDebugAndroidTest -x :app:buildRodaCore -PandroidAbis=arm64-v8a -PcoreProfile=dev
./gradlew :app:connectedDebugAndroidTest -x :app:buildRodaCore -PandroidAbis=arm64-v8a -PcoreProfile=dev -Pandroid.testInstrumentationRunnerArguments.class=xyz.tironi.zoen.PlanPageJourneyTest,xyz.tironi.zoen.FileVersionJourneyTest
```

The existing CI matrix retains API28 and API35, both native ABIs, full JVM/lint and owned relay/device journeys. It records the pre-build source report and verifies the generated Core after building. Current-source release, complete device results and actual image/video evidence remain pending. The coordinator captures stable app/test artifacts and attaches real images and video with `gh --attach` to the existing UI review.

## Final source checkpoint

Root reran the unchanged contract-verifier tests: six passed. All 374 Android/workflow/scripts/docs files were frozen for the direct-main checkpoint. Current Kotlin/JNI/device acceptance, Plan atomic writers, bounded voice migration and the pixel-limit fixture remain pending. The new [prelaunch policy](prelaunch-policy.md) requires replacing canonical signatures/callers together rather than adding `From` compatibility aliases. Resume from [the complete handoff](handoff-2026-10-12.md).
