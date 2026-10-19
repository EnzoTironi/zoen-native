# Expo Web release acceptance

The `expo / release-web-e2e` job exercises `apps/zoen`'s exported Expo Router
application. The older `ci / web-e2e` job exercises the separate
`packages/zoen-ui/web` messenger shell and does not establish Expo acceptance.

The current runner retains the original 14 checkpoints and adds five real browser
checks for a sandboxed builtin list and plain file attachments. The latest exact-source
CI release passed all **19 checkpoints separately in `en-US` and `pt-BR`**. Each run
uses fresh synthetic CLI identities, browser context/process and Pages state.
Locale expectations are authored independently in `expo-web-expectations.mjs`.

## Accepted journey

These are the checkpoints in `acceptance.json`, in execution order. Steps 12–16
are the five additions; the other 14 retain the earlier acceptance scope.

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
17. Cancel local erasure, reopen the chat and retain its encrypted history.
18. Confirm local erasure, reach the popup-safe sign-in document and retain
    onboarding after reload, without a composer or erasure action.
19. Require the independent canonical peer to retain the earlier messages.

The runner uses visible UI and the independent Rust CLI, without IDB/session
injection, fake provider tokens or production accounts. It rejects non-loopback
relays. An authored runner or passing unit tests alone do not establish acceptance.

## Recorded source and artifacts

[CI run 39194529436](https://github.com/EnzoTironi/zoen-native/actions/runs/39194529436)
passed all 19 checkpoints in both locales on Chromium `155.0.8059.39`. It tested
merge `10276df12927fd6c562ca9e64920089224639770` for head
`e2b0ea0736a51ff23737a5de3ddc48f3c86c982c`. Artifact
`expo-release-web-evidence` (ID `12161960284`) contains both successful receipts,
with the same 90-file manifest:

```text
manifest SHA256:
44abaf303e2e6890431895ba275ce99e4965cbd65b5d6c03de00bf3e848270c7
worker integrity:
sha384-L+9+jxotxheYp9vbLHFT/KfMrU8ylmtaxzcR1mU6cbJiA47p+Ozdyg03C3VnJx2S
core/zoen_web_core_bg.wasm: 25,721,313 bytes; SHA256:
95c666cd9451d48fcc40b2fe251dc1087973b5edb36f62536c813e8b25c44701
```

The downloaded proof is
`/private/tmp/zoen-cloudflare-web-v96/ci-e2b-evidence/{en-US,pt-BR}/acceptance.json`,
indexed by `ci-e2b-receipts.json` in the same parent directory. Manifest hashes
mean SHA256 of `JSON.stringify(artifact.manifest)`. Temporary local paths identify
retained evidence; they are not portable downloads. The CI artifact is the hosted
receipt. Its source already includes the decorative-sky accessibility correction.

The following ten-file source change, committed as
`762ceb51f36196f638074e59fc1ab8bf429004c5`, passed a separate local full app check:
TypeScript, Oxlint, Oxfmt and **795/795 tests**, zero failures or skips. This adds
Android access/layout corrections and the localized sign-in connection failure;
it is not part of the 19-checkpoint CI source above. The exact file hashes and
source-diff hash are in `/private/tmp/zoen-web-v96/final-app-check.json`; its log
SHA256 is `6921996d0baa20f3460063aa5a2ebffb30ffce60e18f1a2291db0c1fec901eec`.

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
each locale's `passed` flag and 19 completed steps before accepting the run.

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
`30341e79-f049-4c4c-88b9-bb5fb4942fbf` at
[zoen-web.agenttironi.workers.dev](https://zoen-web.agenttironi.workers.dev)
passed all ten public HTTP paths and exact signing-worker integrity. This fresh
production build includes the decorative accessibility and localized connection
failure corrections. Receipt:
`/private/tmp/zoen-cloudflare-web-v96/live-hosting-03.json`. Its worker integrity is
`sha384-5lhNMH1yUGXCJcds6hQwwqxce9rFjyYH5ABCkzRUpP70btnwC/2vOLvpz4P6rhtw`;
this is distinct from the CI build's bytes. Neither 19-step journey ran against
the public preview.

A separate empty-profile browser observation belongs to the preceding deployment
`bd443adb-9461-452d-9c30-9a4f4005c23a`. It rendered Portuguese onboarding, observed
root `crossOriginIsolated: true` and `/sign-in: false`, and recorded no unhandled
page errors. Both provider controls were unavailable; Google challenge preparation
showed handled `Failed to fetch` against the unavailable production relay. No
provider or device-link action was taken. Receipt and screenshot:
`/private/tmp/zoen-web-v96/public-preview/{acceptance.json,onboarding.png}`.
This does not establish WASM initialization, authentication or public encrypted
sync, nor a rendered check of the newer friendly error message.

See the [Cloudflare deployment guide](../../infra/cloudflare/README.md) for
production reproduction and the outstanding domain/relay boundary. Shared Rust
admission extraction is being validated; it is not an accepted or deployed relay
adapter. No DNS/provider-origin change or R2 activation is implied.

Additional manual baseline checks confirmed quoted replies, thread replies and
scoped search against the canonical peer. Warm offline queuing was bounded:
after history had settled, the peer did not receive the queued message while the
browser was offline and did receive it automatically after reconnect. History
temporarily appeared empty offline. Cutting connectivity during cold chat startup
instead produced a closed-session error after reconnect; page reload did not
recover it. A full browser restart recovered history and delivered the preserved
queue. That startup edge remains open; PWA cold offline loading was not tested.

Canonical Excalidraw remains blocked by unsupported `app_document_read` in the
Web driver, with document save/copy integration also outstanding. A loaded builtin
list does not establish Excalidraw or all-miniapp acceptance. Plain file upload,
preview and exact download are accepted above; file captions, inline replies,
threads and immutable mutation/retry metadata remain incomplete. Collaborative
conflict/history, account recovery, Google/Apple login, Safari/WebKit, camera and
microphone permission paths, manual keyboard/screen-reader/contrast coverage and
performance still need their own journeys.

Retained evidence includes actual file hashes/sizes, source, steps, browser
version, sanitized diagnostics and synthetic screenshots. Pairing captures mask
codes and verification digits in both languages. Do not upload HAR/trace files,
raw frames, IndexedDB dumps, CLI homes, vault keys, provider tokens or relay logs.
The runner removes its private fixture homes and Pages state after each run.
