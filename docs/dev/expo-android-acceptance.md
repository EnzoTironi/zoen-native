# Expo Android acceptance

Recorded on 19 October 2026 for **`apps/zoen`**, Expo SDK 58 / React Native
0.88, on a newly created API 35 arm64 emulator with synthetic accounts. The older
`android/` client and its fixture repairs do not establish this app's acceptance.

## Build and source boundary

The local development APK assembled successfully together with
`:zoen-core:testDebugUnitTest`: **14 JVM tests passed** (eight process-lifecycle
and six erasure/journal/lease tests). These are not device instrumentation tests.
They cover owner replacement, overlapping erasure, exact account scope and waiting
for old requests **and engine destruction** before reopening shared files.

The preserved APK is `zoen-v96-arm64-debug.apk` (243,541,703 bytes). Its JNI comes
from the canonical core build after the builtin resources were regenerated.
Reproducing `llvm-strip --strip-unneeded` matched the packaged JNI exactly.

| Artifact | SHA256 |
| --- | --- |
| Preserved APK | `19e6270d0306470854ba39a8582750182b14e26dc35bac248a664ab3103bedaf` |
| Generated `libroda_ffi.so` | `85f5961f409c4905b5f04162598fc218b7f1b4d11c69e637172adb6e40ff6d85` |
| Packaged, stripped JNI | `fa463d010ca416981ab923c17cb0dbda72009c41bb90c333dae154925476b80b` |
| Android core build stamp | `39a376a1f0efa9a4e1b4394bc8a883cdceb0ecf219565848c4eb6c376177ee76` |
| Fresh application Metro bundle | `923bf7bf8eeb608ccac0874ed9571057a85ba2e99a1b6f7703860ab7aa35bef3` |

`final-core-resource-binding.json` binds the generated Kotlin/JNI and all 18
HTML/manifest resource hashes. `final-android-source-freeze.json` records the
12-file native/cleanup scope; verification found no drift.

The initial Metro ran with `CI=1` and retained its original JavaScript snapshot.
It proved Page creation/read, cancellation, native erasure and subsequent cold
onboarding, but **did not validate the later accessibility/layout fixes**. Its
recorded device bundle hash belonged to the Expo DevLauncher shell, not the app.

Metro was then restarted with watching enabled and IPv4 localhost binding. The
fresh application bundle above was served and the formerly crashing journey was
repeated. The receipt binds these seven source files, matching `762ceb51`:

- Status roles: `components/{account-access,authorize-device,history-transfer}.tsx`
  and `app/backup.tsx`, under `apps/zoen/src/`.
- Header/insets: `app/_layout.tsx`, `app/(tabs)/(chats)/_layout.tsx` and
  `components/app-navigation.tsx`.

Their individual hashes are in `runtime-final-acceptance.json`. The combined
`npm run check` passed TypeScript, Oxlint, Oxfmt and **795 tests**, including the
concurrent shared auth error changes. Its log SHA256 is
`6921996d0baa20f3460063aa5a2ebffb30ffce60e18f1a2291db0c1fec901eec`.

## Observed behavior

- Normal **Recover access → another device** enrollment used the canonical CLI
  primary. With fresh JavaScript, both **Account connected** and **2 messages
  received** status text mounted without the previous Android role exception.
- A Page was created and read through the UI, listed in Files and subsequently
  recovered from the primary's history after local erasure and re-enrollment.
  This does not establish arbitrary Page editing or collaborative conflict handling.
- Headerless tabs respected the measured top inset. The New page title input
  began at normalized `y=0.176`, below the header's bottom `0.118`; previously
  `y=0.058` overlapped the header.
- Cancellation preserved the account twice, including one uninterrupted recorded
  replay. Two confirmed UI erasures returned onboarding. The fresh replay ended
  with `account=null`, no conversations, `erasePending=false` and no notice.
- Before erasure, four encrypted vault files and the account database existed.
  Afterwards, the keys directory was empty and the durable erasure marker absent;
  the subsequently reopened database represented an empty account. Native erasure
  also verifies deletion of its Android Keystore alias before acknowledging success.
- A real Expo Image disk prefetch of the bundled public logo returned `true`
  and a cache path; after each erase the fixture's cache lookup returned `null`.
  The onboarding logo could populate a new static cache entry. This was an SDK
  cache fixture, not a profile-photo upload test.
- After the first erasure, a process restart remained signed out. The independent
  primary retained its original history and reconnected with no pending messages.

## Evidence and reproduction

Evidence is **machine-local temporary data**, not a published CI artifact:
`/private/tmp/zoen-android-v96/`. The entry point is
`runtime-final-acceptance.json`; the preserved APK is at that root. Supporting
files include `native-final-acceptance.json`, `junit-final/`,
`fresh-metro-bundle-receipt.json`, `fresh-post-erasure-{runtime,files}.json`,
`fresh-cache-before.json`, `peer-after-local-erasure.json` and `screenshots/`.
The combined app-check log is `/private/tmp/zoen-web-v96/final-app-check.log`.
These paths need not exist on another machine. Pairing codes, keys and private
database contents are not required in a shared report.

For a new run, generate the Expo Android project through its normal setup, use
JDK 17, SDK package `platforms;android-37.0`, build-tools 37.0.0 and NDK
27.1.12297006. Build the canonical Android core with two jobs, then run
`:app:assembleDebug :zoen-core:testDebugUnitTest --max-workers=2` for arm64.
Use a fresh owned emulator and synthetic CLI primary on a loopback relay. Start
Metro **without `CI=1`**; Node's `--dns-result-order=ipv4first` kept `--localhost`
reachable through scoped `adb reverse` for ports 8096 and the test relay.
Verify the actual served app bundle, then repeat enrollment/history, Page read,
cancel/confirm erasure, SDK cache lookup, cold onboarding and primary-history checks.
Retain receipts before stopping the tools. This run stopped only its own servers
and emulator and disposed only its new AVD, preserving APK/configuration/evidence.

CI correction `ecb83bc2` was **local/unpushed at this acceptance boundary**:
the Expo job pairs `:app:assembleRelease` with the module's actual
`:zoen-core:testDebugUnitTest` task. That configuration fix is not a successful
Release build or runtime receipt.

### Release shrinking correction

Later CI `39198364287` reached `:app:minifyReleaseWithR8` and failed on JNA's
optional desktop AWT references. Fix `db60c33e` puts consumer rules in the Expo
Rust module, preserving its native symbols, reflected Structure layouts and
callbacks while retaining application shrinking and obfuscation. Suppressions name
only the four optional AWT types found by the actual shrinker.

A real Android DEX/minSdk28 probe with JNA 5.18.1, R8 9.2.14 and 84 compiled
UniFFI classes passed. Native names, public RustBuffer fields, FieldOrder
annotations and callback methods survived; an unused control was removed.
Receipt: `/private/tmp/zoen-android-v96/r8-consumer-check/actual-uniffi-final.json`.
This is a focused shrinker proof, not a new APK/runtime acceptance.

[Run 39201639333](https://github.com/EnzoTironi/zoen-native/actions/runs/39201639333)
advanced past the missing-class failure but exhausted the default 2 GiB Gradle heap
inside R8. Its artifact contains only the generated native contract, with **no APK
or JVM test report**. Fix `52da9729` changes only CI: a 4 GiB heap, 1 GiB metaspace,
two workers and serial projects, while retaining application minification. A real
isolated Gradle 9.4.1 task using the exact workflow arguments confirmed those
effective settings; it did not compile an app. The failure and probe index is
`/private/tmp/zoen-android-v96/ci-39201639333-failure/index.json` (SHA256
`dfa19d6817bda33b0f0195f3a4da10a9cb5b4797bb5e74b9b7522d30736b3db6`).
The fresh [run 39204333306](https://github.com/EnzoTironi/zoen-native/actions/runs/39204333306)
on head `52da9729` must establish its own Release/JVM result.

Release runtime, API 28 minimum-runtime behavior, physical devices, public OAuth
and Cloudflare sync, Android media/voice/profile-photo parity, and forced
process-death erasure fault injection remain unaccepted. Performance also remains
unaccepted: initial cold development startup took about six minutes and a later
project selection-to-observed-onboarding bound was about 125 seconds, including
development tooling and observation overhead.
