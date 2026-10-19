# Expo Web release acceptance

The `expo / release-web-e2e` job exercises `apps/zoen`'s exported Expo Router
application. The older `ci / web-e2e` job exercises the separate
`packages/zoen-ui/web` messenger shell and does not establish Expo acceptance.

The current runner retains the original 14 checkpoints and adds five real browser
checks for a sandboxed builtin list and plain file attachments. The v96 local
release passed all **19 checkpoints separately in `en-US` and `pt-BR`**. Each run
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

The immutable [CI baseline, run 39189115561](https://github.com/EnzoTironi/zoen-native/actions/runs/39189115561),
passed the original 14 checkpoints in both locales on Chromium `155.0.8059.39`.
Its tested checkout was merge `165d67cd7c023e2e4e4e267b6f12bdc2e7582259`,
combining head `e76c74be4af4bfe483f4de4b63b6f932a83f0d8b` with base
`bf31bb2717d746344589c3762df7f883b79de5cc`. Artifact
`expo-release-web-evidence` (ID `12158963205`) contains its locale receipts.

The v96 local 19-checkpoint runs used Chromium `155.0.8059.85`, Node `26.12.0`,
Rust `1.99.1` and Expo SDK 58. Both receipts have `passed: true` and the same
90-file release manifest. This was the head above **plus working-tree fixes**,
not an export of the unmodified committed head. Its source receipt records the
diff over `apps/zoen/src`, `packages/zoen-ui/web`, `miniapps/builtins`,
`crates/roda-apps` and `crates/zoen-web-core`, plus individual test-script hashes.

```text
source patch SHA256:
7fe52d40a4d9e10964c5919c685465a988b6f80583caf7bda49d038c2f8dd365
19-checkpoint manifest SHA256:
18eb0166ab4c911825bf25099152480c451cd2810a797e94d022074cb1188bc5
worker integrity:
sha384-5lhNMH1yUGXCJcds6hQwwqxce9rFjyYH5ABCkzRUpP70btnwC/2vOLvpz4P6rhtw
core/zoen_web_core_bg.wasm: 25,718,302 bytes; SHA256:
25d558b47a6182bef979cccdc753100dc1ab508556490a112a89e8d325a6b896
```

Manifest hashes here mean SHA256 of `JSON.stringify(artifact.manifest)`. The
machine-local evidence root is `/private/tmp/zoen-web-v96/`: `tested-source.json`,
`artifact-final.json`, `receipts.json`, and
`final-evidence/{en-US,pt-BR}/acceptance.json`. These paths identify the retained
local proof, not a downloadable CI artifact or a promise that temporary files
exist on another machine. Later commits require their own source-specific run.

### Separate final accessibility export

The 19-checkpoint export includes the builtin form and attachment pointer fixes.
A subsequent release adds the decorative sky's `aria-hidden` and image
`accessibilityLabel` correction. Only the common JavaScript chunk and `index.html`
differ; the worker and WASM are identical. Its manifest SHA256 is:

```text
cc14c9d16e66e7d14025185a64299986ca7c4a71d77ac43ec62d601c11765970
```

Actual onboarding DOM inspection confirmed both decorative images have empty
alternative text and a hidden ancestor. Axe with `wcag2a`/`wcag2aa` reported
**zero violations**, 16 passes and one incomplete rule: manual color-contrast
review remains for eight nodes. This is onboarding evidence, not whole-app WCAG
or screen-reader acceptance. The final attribute delta did not receive another
19-checkpoint run. Its receipts are `a11y-artifact.json`,
`onboarding-final-wcag.json` and `onboarding-final.png` under the same local root.

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
`bd443adb-9461-452d-9c30-9a4f4005c23a` at
[zoen-web.agenttironi.workers.dev](https://zoen-web.agenttironi.workers.dev)
passed all ten public hosting paths and exact signing-worker integrity. It is a
separate production-configured build including the final decorative
accessibility correction. The local 19-checkpoint journeys were not run against this
public preview. Provider login and public encrypted sync remain unaccepted;
the configured production relay is unavailable. See the
[Cloudflare deployment guide](../../infra/cloudflare/README.md) for production
build commands and the current domain/relay boundary.

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
