# Expo Web release acceptance

The `expo / release-web-e2e` job exercises `apps/zoen`'s exported Expo Router
application. The older `ci / web-e2e` job exercises the separate
`packages/zoen-ui/web` messenger shell and does not establish Expo acceptance.

The current runner retains the previous 19 checkpoints and adds three real browser
checks for canonical Excalidraw documents. The current local release passed all
**22 checkpoints separately in `en-US` and `pt-BR`**, locally and in CI. Each run uses fresh synthetic CLI identities, browser
context/process and Pages state.
Locale expectations are authored independently in `expo-web-expectations.mjs`.

## Accepted journey

These are the checkpoints in `acceptance.json`, in execution order. Steps 17–19
are the drawing additions; the previous 19 retain their acceptance scope.

1. Inspect the actual release files and verify the application's pinned worker
   integrity and the worker's pinned WASM bytes.
2. Serve that export through Wrangler Pages and check ten HTTP paths: SPA
   fallback, exact sign-in isolation/308 aliases, worker and WASM content types.
3. Create an encrypted DM seed and confirm it is durably readable by both
   canonical CLI peers before browser linking.
4. Use normal device linking, compare both sides' verification digits and
   recover the older encrypted history in the browser.
5. Exchange messages between the Expo composer and canonical peer; require the
   peer's persisted decrypted history and no private test plaintext in outbound
   relay frames.
6. Reload `/chat/<id>` and retain keys and history without onboarding or alerts.
7. Import a synthetic UTF-8 Markdown file through the browser file chooser.
8. Check a task, retain that draft through a cold route reload, then uncheck it
   and return to the published state with sharing disabled.
9. Edit a Markdown body block and retain the draft through another route reload.
10. Publish the edit, reload and download the actual browser Markdown export.
11. Require exact export bytes from the independent canonical peer.
12. In the builtin list iframe, verify the sandbox remains `allow-scripts` and
    exercise checkbox, mouse Add and Enter through canonical MCP tools.
13. Reload the list route and retain those canonical changes.
14. Require the same items and checked state in the canonical peer's app view.
15. Click the attachment menu and File with the mouse, use the real file chooser
    and publish a plain encrypted file.
16. Open/download the file in the browser and download it through the canonical
    peer; require both outputs to equal the original fixture bytes.
17. Open a canonical native-peer Excalidraw Item inside the unchanged strict
    `allow-scripts` sandbox and verify the actual MCP scene-read bytes.
18. Draw a rectangle and type text through the actual canvas controls; require
    both edits in the independent peer's decrypted document.
19. Cold-reload the drawing and compare the guest's actual reconstructed read
    bytes with the peer's file, while retaining the original encrypted version.
20. Cancel local erasure, reopen the chat and retain its encrypted history.
21. Confirm local erasure, reach the popup-safe sign-in document and retain
    onboarding after reload, without a composer or erasure action.
22. Require the independent canonical peer to retain the earlier messages.

The runner uses visible UI and the independent Rust CLI, without IDB/session
injection, fake provider tokens or production accounts. It rejects non-loopback
relays. An authored runner or passing unit tests alone do not establish acceptance.

## Recorded source and artifacts

### Current local drawing release

Adapter `10329c8c` supports drawing read/save/copy and installation through the
existing encrypted snapshot/upload pipeline. Scene validation is shared with the
native client in `miniapps::excalidraw` (`ac7f6cb7`). The shared library's 17 tests,
16 existing native document regressions, 65 Web core tests including eight drawing
regressions, and strict all-target Clippy passed. Tests cover immutable retries,
stale writes, upload barriers, unchanged saves, copied ownership and denied current
or historical descriptors after revocation, membership loss or a broken log.

Runner `fc4feece` passed 22/22 in both locales on source `46128a65` plus the exact
Web-only header inset now committed as `d4a0590d`. Each locale passed on its first
attempt without uncaught JavaScript errors. A passive message observer reads real
host-to-guest MCP responses; it never replaces transport, changes application
state or injects keys. The fresh CLI's SHA256 is
`6a9b639ae3d37af83fe1963667bebb0a1f03602c660e750dc15371ee634807c6`.

Receipt `/private/tmp/zoen-web-v96/drawing-release-22/acceptance.json` has SHA256
`6cd706271bb594da8b659214f332b211bc872fb5499c22a6f3a70b180500545c` and binds
16 frozen sources, the one-line inset patch, artifacts, logs and screenshots. The
final app check passed 795 tests, TypeScript, Oxlint and Oxfmt. The drawing's real
version-four bytes matched the native peer: 1,278 bytes in English and 1,277 in
Portuguese; version one remained unchanged.

The 90-file QA export has manifest SHA256
`745309d8ee986060b90b0c59df391fb2816d8e26a0e52d267bfacc0ff3db6f4d`.
Its worker integrity is
`sha384-dYc1Ds6ChZA3cp5LAy6/3BNbEuwGze2+MlxwJotYmJoXyigQWBHsMy9h5GHmbjwN`;
WASM is 25,876,496 bytes with SHA256
`f493705d54fe1c23165f351ae85ad9aa9fe551feafc3ebfe7442da7246d10088`.
The export used an external task-owned directory and left all previous production
export files unchanged. It was never deployed. Its earlier pre-inset acceptance
remains separately retained. This does not accept image import, every conflict or
fault path, public authentication or full comparative parity.

### Current CI and smaller release artifact

The subsequent [run 39207994304](https://github.com/EnzoTironi/zoen-native/actions/runs/39207994304)
also passes 22/22 in each locale on merge `85c7f2d58aef38fc5853fb7b8efab3641aee0d91`
for head `c727b1f9`, with zero browser errors. Its Directory extension passes71
PostgreSQL scenarios and 18 workerd cases. Web artifact `12167030781` contains
separate 90-file Linux manifest receipts with a 23,250,613-byte WASM; those bytes
are not equated with the deployed Mac artifact. The workflow has five passing
Expo jobs and an admission formatting failure before its tests; the correction is
in `8d64d762`. The prior fully green run below retains its original attribution.
Combined source/archive verification receipt:
`/private/tmp/zoen-cloudflare-web-v96/ci-c727-evidence/acceptance.json`
(SHA256 `47ba634a497895c0479035eb9b3241b18cc7ca11edb722a479eba66570c8b1d0`).


[CI run 39204333306](https://github.com/EnzoTironi/zoen-native/actions/runs/39204333306)
passed all six Expo jobs on merge `59998b677ce581bd49a045538ee62f3d17b55347`
for head `52da9729`. Actual browser receipts passed 22/22 per locale with no page
errors; the closed-group 17/4/5/4 and Directory 33 PostgreSQL/nine workerd checks
also passed. Four immutable GitHub artifact digests and 37 exact-merge source
bindings were checked. Receipt:
`/private/tmp/zoen-cloudflare-web-v96/ci-52da-evidence/acceptance.json` (SHA256
`433598e928827681ac6d3d062d30f5dceddc3eec8a9bc51e46d4c9d551b965ed`).
The Linux client check discovered 795 tests: 793 passed and exactly two explicitly
Darwin-only Swift harnesses were skipped. Both pass in the macOS 795-test check.
The separate Android receipt accepts the minified Release APK and 14 JVM tests;
its runtime has its own acceptance boundary.

Later release-only changes `945170ea` and `891d6d9b` remove the debug name section
and normalize esbuild's working directory. Every remaining WASM section is byte
identical: 25,876,496 becomes 23,251,769 bytes, saving 2,624,727 bytes (10.14%).
Development keeps debug names. Real builds from the repository and app directories
produce identical WASM, glue, worker and pin bytes. The resulting QA artifact
passed 22/22 in each locale again, with 795 local client tests and all checks.
Receipt `/private/tmp/zoen-web-v96/release-strip-01/acceptance.json` has SHA256
`33259fb5c08f0b6bbd7c7ec13c817ad8ae0e36f45f0a2a070465087e9bb0e63e`;
`reproducible-build.json` has SHA256
`a69eb6c19362ee8dacb3d98f2d87b9df2ea5c01af1b89f1719d7e96a1fab14ee`.
The QA artifact was never published; public production uses its own provider
configuration. This measures artifact bytes, without startup/performance acceptance.

### Prior CI release

[CI run 39201639333](https://github.com/EnzoTironi/zoen-native/actions/runs/39201639333)
passed the previous 19 browser checkpoints per locale and the private closed-group
differential step on merge `d126484348aded89d131b018933545dd858374c2` for head
`db60c33e`. Its downloaded proof is indexed at
`/private/tmp/zoen-cloudflare-web-v96/ci-db60-evidence/index.json`. It predates
the new drawing and Directory changes; the later 52da run above accepts that scope.

[CI run 39198364287](https://github.com/EnzoTironi/zoen-native/actions/runs/39198364287)
passed all 19 checkpoints in both locales on Chromium `155.0.8059.39`. It tested
merge `bfd35da7d52271a332e8b439b69d23850e87a973` for head
`7c71cecbf3e71fb502c9213b97bd909854dfa2d2`. Artifact
`expo-release-web-evidence` (ID `12162482735`) contains both successful receipts,
with the same 90-file manifest:

```text
manifest SHA256:
184a76ed4667d5d37a4acdce7f3a90fe55fa5d75e1595972f7643cf2bee74570
worker integrity:
sha384-L+9+jxotxheYp9vbLHFT/KfMrU8ylmtaxzcR1mU6cbJiA47p+Ozdyg03C3VnJx2S
core/zoen_web_core_bg.wasm: 25,721,313 bytes; SHA256:
95c666cd9451d48fcc40b2fe251dc1087973b5edb36f62536c813e8b25c44701
```

The downloaded proof is
`/private/tmp/zoen-cloudflare-web-v96/ci-7c71-evidence/{en-US,pt-BR}/acceptance.json`,
indexed by `ci-7c71-receipts.json` in the same parent directory. Manifest hashes
mean SHA256 of `JSON.stringify(artifact.manifest)`. Temporary local paths identify
retained evidence; they are not portable downloads. The CI artifact is the hosted
receipt. Its source already includes the decorative-sky accessibility correction.

The ten-file source change, committed as
`762ceb51f36196f638074e59fc1ab8bf429004c5`, passed a separate local full app check:
TypeScript, Oxlint, Oxfmt and **795/795 tests**, zero failures or skips. It adds
Android access/layout corrections and the localized sign-in connection failure,
and is included in the 19-checkpoint CI source above. The exact file hashes and
source-diff hash are in `/private/tmp/zoen-web-v96/final-app-check.json`; its log
SHA256 is `6921996d0baa20f3460063aa5a2ebffb30ffce60e18f1a2291db0c1fec901eec`.

The later font-only correction `75fffd98` has scoped TypeScript/Oxlint/Oxfmt and
actual canonical guest-render proof, with computed `system-ui` typography. The
795-test suite was not repeated for that style-only change.

Earlier evidence retains its original attribution. The first local v96 19-step
EN/PT proof used head `e76c74be4af4bfe483f4de4b63b6f932a83f0d8b` plus source patch
`7fe52d40a4d9e10964c5919c685465a988b6f80583caf7bda49d038c2f8dd365`,
Chromium `155.0.8059.85`, and manifest
`18eb0166ab4c911825bf25099152480c451cd2810a797e94d022074cb1188bc5`.
Its `tested-source.json`, `artifact-final.json`, `receipts.json` and
`final-evidence/{en-US,pt-BR}/acceptance.json` remain under
`/private/tmp/zoen-web-v96/`. The older
[14-checkpoint CI baseline](https://github.com/EnzoTironi/zoen-native/actions/runs/39189115561)
tested merge `165d67cd7c023e2e4e4e267b6f12bdc2e7582259` for that e76 head;
artifact `12158963205` predates the five new browser checkpoints.

### Accessibility evidence

The automated accessibility result belongs to a separate local release, manifest
`cc14c9d16e66e7d14025185a64299986ca7c4a71d77ac43ec62d601c11765970`.
It added the sky's `aria-hidden` and image `accessibilityLabel` correction to the
first local 19-step export; that pair differed only in common JavaScript and
`index.html`, with identical worker/WASM bytes. These are separate from the CI
artifact identities above.

Actual onboarding DOM inspection confirmed empty alternative text and a hidden
ancestor for both decorative images. Axe with `wcag2a`/`wcag2aa` reported **zero
violations**, 16 passes and one incomplete rule: manual color-contrast review
remains for eight nodes. Receipts are `a11y-artifact.json`,
`onboarding-final-wcag.json` and `onboarding-final.png` under the local v96 root.
The later 19-step CI includes the source correction but does not rerun this axe
check or establish whole-app WCAG, contrast or screen-reader acceptance.

## Reproduce

Use Node 26, Rust `1.99.1`, target `wasm32-unknown-unknown`,
`wasm-bindgen-cli` `0.2.129`, and a Chromium executable. The workflow provisions
its disposable database and relay. Locally, first start an independently
authorized isolated loopback relay. Keep at least 5 GiB free and coordinate heavy
builds with other platform validation. From the repository root:

```sh
npm ci --prefix apps/zoen
npm ci --prefix scripts/expo-web-tests
CARGO_BUILD_JOBS=2 cargo build --locked -p zoen-cli --bin zoen

qa_root="$(mktemp -d "${TMPDIR:-/tmp}/zoen-web-acceptance.XXXXXX")"
qa_chrome=/absolute/path/to/chromium
EXPO_NO_DOTENV=1 \
CARGO_BUILD_JOBS=2 \
EXPO_PUBLIC_ZOEN_RELAY_URL=http://127.0.0.1:8790 \
EXPO_PUBLIC_GOOGLE_IOS_CLIENT_ID='' \
EXPO_PUBLIC_GOOGLE_WEB_CLIENT_ID='' \
npm run export:web --prefix apps/zoen -- \
  --output-dir "$qa_root/dist" --max-workers 2

qa_status=0
for qa_locale in en-US pt-BR; do
  TMPDIR="$qa_root/" \
  RELAY=http://127.0.0.1:8790 \
  CHROME="$qa_chrome" \
  ZOEN_EXPO_DIST="$qa_root/dist" \
  ZOEN_EXPO_EVIDENCE="$qa_root/evidence" \
  ZOEN_EXPO_TEST_LOCALE="$qa_locale" \
  node scripts/expo-web-e2e.mjs || qa_status=1
done
test "$qa_status" -eq 0
```

Replace `qa_chrome` with the actual executable. Both locales run even if one
fails; either failure fails the combined result. The locale is mandatory. Inspect
each locale's `passed` flag and 22 completed steps before accepting the run.

The loopback export has external providers disabled for QA. **Do not deploy it.**
For independent artifact or HTTP checks, without accounts or a browser:

```sh
node scripts/expo-web-artifact.mjs "$qa_root/dist"
node scripts/expo-web-hosting.mjs "$qa_root/dist"
node scripts/expo-web-hosting.mjs "$qa_root/dist" --workers
```

Pages uses the emitted `_headers` and `_redirects`; Workers additionally applies
its production routing configuration. SDK 58's single-page export must retain
dynamic-route fallback and omit a top-level `404.html`. Exact `/sign-in/` and
`/sign-in.html` aliases redirect with 308 to popup-safe `/sign-in`; lookalike
paths remain isolated. The HTTP assertions check actual responses, not just
configuration text. Artifact/hosting checks do not establish encrypted messaging.

## Public hosting and remaining scope

On 19 October 2026, production frontend deployment
`359e0cd0-41c5-4e53-b64c-fef8e1b4d0e1` from clean source `891d6d9b` at
[app.tryzoen.com](https://app.tryzoen.com) and
[zoen-web.agenttironi.workers.dev](https://zoen-web.agenttironi.workers.dev)
passed all ten public HTTP paths, exact SPA/worker/WASM bytes and signing-worker
integrity on each origin. This fresh production export includes canonical Web
Excalidraw, the measured header inset and the smaller reproducible runtime. Receipt:
`/private/tmp/zoen-app-domain-v96/release-09-public/acceptance.json` (SHA256
`5c9210eccb0267bd60d59255c830ccdfc0f79fd29097532054e03a8444394ac0`).
Its worker integrity is
`sha384-pImaTmN20eB49OhAsS9/4uCr8hZDDmu0NhVTfWUyy5IEBLeeGXkVY+YR8YeYLGlH`;
WASM is 23,251,769 bytes with SHA256
`f1e077a46b914c8098d9448b42e694fd47bffdc944cb73a553d16c50ae17e096`.
Production's 90-file manifest is
`d182d994fcd239b770aaa2035e3f5914aa2d72e3e1456744f237ffdabfc83110`.
The authenticated 22-step journey ran only on the controlled loopback export;
public hosting uses production provider configuration.

The exact fail-closed `app.tryzoen.com/*` route uses the two existing DNS records
with Proxied enabled; their contents and Auto TTL were retained. `no-transform`
prevents the zone's analytics injection from modifying served HTML. Both public
origins and real local Pages/Workers checks verify that policy and exact bytes.

Empty-profile Chrome `155.0.8059.85` on **both** public origins rendered Portuguese
onboarding, the localized connection error and system Retry font with no unhandled
page errors or injected analytics. Root documents were isolated and sign-in
popup-safe. Both providers remain unavailable against the dead relay. No
provider/device-link action was taken. Receipts and screenshots are under
`/private/tmp/zoen-app-domain-v96/release-09-public/{canonical,workers-preview}/`.
This establishes guest rendering, without successful WASM initialization,
authentication or encrypted-sync acceptance. Prior releases07/06, canonical-domain
and preview observations remain separately attributed in the acceptance manifest.

See the [Cloudflare deployment guide](../../infra/cloudflare/README.md) for
production reproduction and the outstanding relay boundary. The
[shared Rust admission foundation](../../infra/cloudflare/admission.md) passed
native/Node/workerd checks; a complete relay adapter remains unfinished.
Provider origins and R2 activation are unchanged.

Additional manual baseline checks confirmed quoted replies, thread replies and
scoped search against the canonical peer. Warm offline queuing was bounded:
after history had settled, the peer did not receive the queued message while the
browser was offline and did receive it automatically after reconnect. History
temporarily appeared empty offline. Cutting connectivity during cold chat startup
instead produced a closed-session error after reconnect; page reload did not
recover it. A full browser restart recovered history and delivered the preserved
queue. That startup edge remains open; PWA cold offline loading was not tested.

Canonical Excalidraw now has checked read/save/copy and install operations, with
actual canvas editing and cold-reload/native-peer byte evidence above. Image
import, broader collaborative/fault paths and every miniapp capability still need
their own acceptance. Plain file upload, preview and exact download are accepted
above; file captions, inline replies,
threads and immutable mutation/retry metadata remain incomplete. Collaborative
conflict/history, account recovery, Google/Apple login, Safari/WebKit, camera and
microphone permission paths, manual keyboard/screen-reader/contrast coverage and
performance still need their own journeys.

Retained evidence includes actual file hashes/sizes, source, steps, browser
version, sanitized diagnostics and synthetic screenshots. Pairing captures mask
codes and verification digits in both languages. Do not upload HAR/trace files,
raw frames, IndexedDB dumps, CLI homes, vault keys, provider tokens or relay logs.
The runner removes its private fixture homes and Pages state after each run.
