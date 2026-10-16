# Living Space local verification

Status: local follow-up checks are recorded below. The experiment remains off by default. This report must not be read as production deployment or end-to-end UI/relay verification.

## Core checks

| Check | Command / method | Result |
| --- | --- | --- |
| Native core | `cargo test -p roda-ffi --lib --no-run --message-format=json`, then the resulting immutable test executable with no filters | Passed: 339 tests, zero failures (489.22 seconds) |
| Shared domain and browser core | `cargo test -p zoen-web-core -p roda-types --lib` | Passed: 19 domain and 28 browser tests |
| Native lint | `cargo clippy -p roda-ffi --all-targets -- -D warnings` | Passed |
| Browser lint | `cargo clippy -p zoen-web-core --all-targets -- -D warnings` | Passed |
| WASM target | `cargo check -p zoen-web-core --target wasm32-unknown-unknown` | Passed; existing unused `roda-mls::group::leaf_keys` warning |
| Development WASM | `cargo build -p zoen-web-core --target wasm32-unknown-unknown --lib -j 2`, then `wasm-bindgen --target web` | Passed |
| Actual WASM initialization | `node scripts/test-living-space-wasm.mjs` | Passed: loaded real WASM memory and all 12 Space/page/file methods, no account or relay |
| Development web bundle | `node web/build.mjs --skip-wasm` after fresh binding generation | Passed; final UI source will be rebuilt after the current application checks |
| Source formatting | `cargo fmt -p roda-ffi -p roda-types -p zoen-web-core -- --check` | Passed |
| Evidence inventory tests | `python3 scripts/test-living-space-evidence.py` | Passed: 3 tests; this does not verify application behavior |

The first full native run failed three tests (335 passed): ordinary anonymous
conversation, page and approval reads were accidentally subjected to an
unconditional local-account lookup. The failure was reproduced in isolation,
guards were scoped to protected resources without weakening protected anonymous
access, all three original cases passed individually, and the full corrected
339-test run passed. No tests were skipped or disabled.

## What the behavioral tests exercise

- Default-off entry points and owner/admin/member announcement permissions.
- Accepted pending outbox events in home/hackathon/tier projections, so immediate
  offline follow-up operations do not wait for a relay acknowledgement.
- Real bounded MLS conversation allocation with a two-member cap in the test,
  hidden child destinations, and refusal to post conversation text to the parent.
- Real MLS team creation, registration/phase and judge/vote guards, signed team
  consent, and idempotent winner publication.
- Tier grants and revocation across two real local accounts: key-package claim,
  Welcome delivery, successful decryption before revocation, epoch advancement,
  and refusal of future decryption after the member is removed.
- An actual protected page and file published through signed MLS frames: a member
  reads them before revocation; cached page/item/file APIs deny access after it.
  An old chunk key cannot decrypt new file content. Previously received plaintext
  is not erased and is not claimed to be retroactively revoked.
- Scoped, expiring, revocable invitations, admin-only persisted invitation metadata
  without raw capability tokens/digests, paid-source refusal, concurrent unrelated
  tier grants, and sticky revocation that stale issuance cannot undo.
- Real browser MLS admission/removal and durable failed admission, encrypted
  consent, file encryption/authentication, signed upload plans, and page edits
  from two certified devices converging from sequenced signed relay frames.
- Page snapshots, stale-editor merge, idempotent edit IDs, collision refusal,
  authenticated page-blob loading, and encrypted page creation/reopen.

## Application and visual evidence

Recovery checks: `cargo fmt -p roda-ffi -p roda-types -p zoen-web-core -- --check`
and combined `cargo clippy -p roda-ffi -p zoen-web-core --all-targets -- -D warnings`
passed. A fresh `cargo test -p zoen-web-core -p roda-types --lib` passed (19 domain,
28 browser tests). Actual WASM initialization passed again without an account or
relay. Web `npm run typecheck:web` passed and `npm run test:web` passed 81 tests,
with no skips. The first web run reproduced two stale expectation failures
(78/80); updated expectations and an added list/detail/back test passed.

The initial Android APK assembly failed in D8 on a non-local return generated in
`ZoenAppKt`; the recovered source contains the scoped fix. Current Android
instrumentation and fresh Apple capture results are recorded in the follow-up
section below; Kotlin compilation alone is not proof of packaging or UI success.

The first frozen web capture batch contained an empty Notifications state rather
than a join request and an unexpanded membership screen. Those captures need
replacement and are not approval-flow evidence. Final platform results and
inventory will be recorded here after the current runs.
Screenshots belong in `~/run-shots/`, not a public media host. The
inventory expects nine screens in light/dark across iPhone, iPad, Android,
narrow web and wide web. Its file-presence result is not parity approval.

The canonical iPhone viewport is 440 × 956 points and the canonical iPad viewport
is 1194 × 834 points. The shared synthetic cover is exported from UIKit, not a
screenshot crop. Synthetic people and an isolated account are used; no WhatsApp
research captures, third-party phone numbers or real-account stores are used.

Native screenshot journeys use real app components and core operations. The
signed local demo membership-request fixture is not MLS transport proof. Expanded
native event/team/tier journeys use an actual isolated local account with real MLS
state, a reserved test relay address and no live network synchronization. Debug
fixture keys are volatile and never read from a real user's Keychain.

Web component captures are explicitly labelled synthetic: they render production
components through a local test harness with no connected account or backend.
They prove rendering and exercised component interactions only, not a live relay
journey. Actual protocol/crypto behavior is established separately by Rust tests.

## Known boundaries and remaining gaps

- ADR 0027 keeps community parents `Closed` and relay-readable. Root home pages,
  announcements and event/entitlement metadata retain that policy; child
  conversations, teams and restricted membership spaces are MLS-encrypted. Do not
  call the entire Space home end-to-end encrypted.
- Conversation partitions are independent cohorts, not one global transcript.
  Community-wide encrypted fan-out and billion-user capacity/load are not proved.
- Join/tier invitation flows require the community to be known locally. Complete
  unknown-community discovery/hydration is not established by the local checks.
- The tier crypto journey publishes its protected page after admission. Catch-up
  of pre-existing encrypted pages/events for a newly admitted member is not
  established by that test; do not infer historical-content access from key
  delivery for future epochs.
- Finalization automatically publishes a winner announcement once, but an
  authorized client must execute finalization after the deadline. This is not an
  unattended server-side content scheduler.
- Removing a team creator can fail closed if the acting community administrator
  lacks authority in that child's MLS group; ownership transfer is not supplied by
  this experiment. The API must not claim revocation while keys remain.
- Browser files and downloaded page history are bounded to 64 MiB. No production
  deployment, real-relay UI journey, physical-device haptic check or pixel-perfect
  cross-platform parity is claimed by the core tests.
- Agents and mini-app execution remain frozen in the experimental surfaces.
  The submission demo slot is intentionally inert. Paid entitlement sources are
  reserved and rejected in v1; no payments were built.

## Local follow-up checks

Fresh checks on the recovered `feat/living-space` source:

- `cargo test -p zoen-web-core -p roda-types --lib`: passed (28 browser, 19 domain).
- `cargo test -p roda-ffi --lib living_space -- --test-threads=1`: passed, six tests, including default-off, real MLS hackathon teams/winner publication and admission partitions.
- `cargo clippy -p roda-ffi -p zoen-web-core --all-targets -- -D warnings`: passed.
- `cargo fmt -p roda-ffi -p roda-types -p zoen-web-core -- --check`: passed.
- Swift projection command documented in `apple/Tests/LivingSpaceVerification.md`: passed, nine cases.
- `npm run typecheck:web` and `npm run test:web` in `packages/zoen-ui`: passed, 81 tests, no skips, after final source changes.
- `node scripts/test-living-space-wasm.mjs`: passed actual WASM initialization and 12 entry points, without account/relay.
- `python3 scripts/test-living-space-evidence.py`: passed, three tests.
- Android core contract and UniFFI callback checks: passed, six and five tests respectively.
- Full `cargo test -p roda-ffi --lib` reruns were interrupted under severe host memory pressure; they are **partial**, not fresh full-suite passes. The historical 339-test pass above is separate evidence.

Web fixtures produced 36 fresh screenshots in `~/run-shots/`. The comparison
sheet is `web-ipad-comparison.png`: Home and Notifications, light/dark. The
sidebar is 320 points; Home card x397/y106/width720/radius40, cover height160,
title x421/y302 at 34 points. Remaining measured differences include the Live
page link and Events approximately eight points higher, Notifications card
about four points lower/taller and section label about 22 points farther left.
Native SF Symbols, glass/shadow/text rendering, sidebar-toggle control and OS
chrome are not identical. Synthetic notification names differ. These component
fixtures establish rendering, not account/relay behavior.

Android Notifications recapture remains blocked: after a cold boot the API35
emulator reached boot-completed and notification permission was confirmed granted,
but both targeted instrumentation attempts died before reporting tests. Logcat
records signal 9 and system-wide startup ANRs while the host used about 31 GiB
of swap. No fresh clean Android Notifications image is claimed. Extracted older
images have a `-previous` suffix and are not accepted evidence. Relevant local
logs are `/tmp/android-notification-clean-check.log` and
`/tmp/android-notification-recovery-tests.log`. No unrelated build was stopped.

Fresh iPad `test-without-building` on the current build exercised baseline and
expanded light/dark journeys: three tests passed, BaselineDark failed at the
post-deletion save acknowledgement timeout. Its failure hierarchy already showed
`page.saved` / Saved, with no save error. The editor debounces saves for four
seconds, then crosses Core and updates the toolbar; host UI idling was slow.
Only that acknowledgement wait was increased from 10 to 30 seconds. No persistence
assertion was removed. A fresh `build-for-testing` passed and the same
BaselineDark case passed (one test, zero failures, 102.289 seconds). Logs:
`/tmp/living-fresh-ipad.log`, `/tmp/living-fresh-ipad-fixed.log` and corresponding
xcresults. All 18 fresh iPad screenshots and 18 metrics are under `~/run-shots/`,
with `apple-fresh-provenance.json` and original/fixed attachment manifests.
The recovered older iPhone captures remain historical, not a fresh iPhone run.
