# Expo Web release acceptance

The `expo / release-web-e2e` job exercises `apps/zoen`'s exported Expo Router application. The older `ci / web-e2e` job exercises `packages/zoen-ui/web`'s separate messenger shell and does not establish Expo acceptance.

The first journey uses a real loopback Rust relay, disposable Postgres/FoundationDB, two canonical CLI identities and a fresh Chromium context:

1. Verify the exported application pins the exact worker and WASM bytes. Serve that same export through Wrangler's Cloudflare Pages runtime.
2. Check exact COOP/COEP values on sign-in aliases, root and worker routes; verify the WASM MIME type and an arbitrary dynamic chat URL returns application HTML.
3. Create a private DM and a synthetic message before the browser joins. Use the ordinary **Conectar com outro aparelho** and **Mostrar código** flow, authorize through the existing CLI, and compare both sides' verification digits.
4. Enter the isolated application and read the older encrypted message. Send from the Expo composer and require the real CLI peer's decrypted, persisted timeline to contain it. Send a reply from that peer and read it in Expo. Check outbound relay frames do not contain the private test text.
5. Reload the actual `/chat/<id>` URL and require the same encrypted session and all three messages without onboarding or error alerts.
6. Import a synthetic UTF-8 Markdown file through the real file chooser into that encrypted conversation. Check its task and require the checked local draft to survive a dynamic-route reload. Uncheck it and require the exact published state with sharing disabled. Edit a body block and require that local draft to survive another reload. Publish the edit, reload again, download the real Markdown export and compare its exact bytes with an export decrypted by the independent canonical CLI peer.
7. Open You and cancel **Sair e apagar dados**. Reopen the chat and require all three messages again.
8. Confirm local erasure, require the nonisolated sign-in document, reload it and require onboarding with no message composer or erasure action.
9. Read the independent canonical peer and require all three earlier messages to remain present.

Message assertions use the visible message's exact author and accessible button name,
then its body. A hidden conversation preview can repeat that body; matching only text
would make the assertion ambiguous. Checkpoint `03d787f` passed the messaging and
erasure journey, but screenshot review found a false account-change banner. The
reconnect correction, stricter zero-alert assertion and new Markdown journey need
their own successful revision-specific run before being accepted.

At `7d65d6b5`, run `39055735543` accepted the zero-alert chat reload, real
Markdown import, checked task with a saved draft and unchecked task returning to
the original content. It then failed an incorrect assertion that reversing this
edit must remain unsaved. The corrected runner requires a cold reload while the
task is checked, then a clean state after unchecking. A canonical Rust regression
also checks both states, exact Markdown and snapshot/reopen persistence. Actual
Release WASM and worker/Messages Node harnesses preserve the checked draft, using
in-memory checkpoints and offline transport. Those harnesses do not establish
browser-process or IndexedDB acceptance; the complete CI journey is still required.

No IDB/session injection, fake identity-provider tokens, authentication bypass or production account is involved. The runner refuses a non-loopback relay. Browser evaluation is limited to standard isolation capabilities; account and message assertions use visible UI and the independently running Rust CLI.

## Build and run

Use Node 26, Rust 1.99.1, the `wasm32-unknown-unknown` target, `wasm-bindgen-cli` 0.2.129, and a Chromium executable. The workflow provisions the ephemeral database/relay. For an independently authorized local run, start an isolated loopback relay first, then:

```sh
npm ci --prefix apps/zoen
npm ci --prefix scripts/expo-web-tests
cargo build --locked -p zoen-relay -p zoen-cli
EXPO_PUBLIC_ZOEN_RELAY_URL=http://127.0.0.1:8790 \
EXPO_PUBLIC_GOOGLE_IOS_CLIENT_ID='' \
EXPO_PUBLIC_GOOGLE_WEB_CLIENT_ID='' \
npm run export:web --prefix apps/zoen

RELAY=http://127.0.0.1:8790 \
CHROME=/path/to/chromium \
ZOEN_EXPO_EVIDENCE=/path/to/evidence \
node scripts/expo-web-e2e.mjs
```

This is a **test artifact** built with public loopback configuration and unavailable external providers. Do not deploy it. Build the deployable artifact separately with the intended relay and public provider identifiers. Existing device linking remains the normal production flow in both builds. Apple/Google confirmation and provider-account creation remain separate acceptance journeys.

`node scripts/expo-web-artifact.mjs apps/zoen/dist` can inspect an existing export without starting a browser. This checks real emitted files and integrity relationships; it does not establish runtime acceptance.

`node scripts/expo-web-hosting.mjs apps/zoen/dist` additionally starts Wrangler and checks the actual export over HTTP, without a browser, relay or accounts. It verifies SPA fallback, exact sign-in headers and 308 alias destinations, worker/WASM content types and isolation. This is independently runnable with only the export and the installed test-tool package; it does not establish the encrypted-message journey.

## Hosting and evidence

The test uses the exported `_headers` with Cloudflare Pages itself. It does not substitute a permissive Node server. The app uses SDK 58 `web.output: "single"`: private encrypted chat IDs are resolved by Expo Router after client startup. [Expo's configuration reference](https://docs.expo.dev/versions/v58.0.0/config/app/#output) defines this single-page export. [Cloudflare Pages](https://developers.cloudflare.com/pages/configuration/serving-pages/#single-page-application-spa-rendering) serves the root HTML for unknown paths when no top-level `404.html` exists. The artifact inspector rejects that file to preserve the fallback. Exact `/sign-in/` and `/sign-in.html` redirects canonicalize to `/sign-in` with 308; all three paths retain popup-safe isolation headers, while lookalike paths remain isolated. Since [Pages applies redirects before headers](https://developers.cloudflare.com/pages/configuration/headers/#match-a-path), the HTTP check asserts the actual redirect responses and destination policy. The real dynamic-route reload is therefore an acceptance condition: a missing fallback, incorrect content type, comma-merged isolation header or wrong auth policy fails before account creation. Hosting fixes belong in the exported configuration; weakening this check would conceal a broken deployed entry path.

The retained artifact contains a revision, hashes and sizes of the actual export, completed steps, browser version, sanitized diagnostics, and screenshots after onboarding/history/roundtrip/reload. Failure screenshots mask the QR, link text and verification digits. No HAR, trace, raw WebSocket frames, IndexedDB dumps, CLI homes, identity keys, provider tokens or relay logs are uploaded. Private fixture homes and Pages state are removed after the run. Screenshots contain only the journey's explicitly synthetic content.

An authored runner or passing syntax/type checks do not count as a passed journey. Use `acceptance.json`'s `passed` flag and completed steps from a successful CI run for revision-specific evidence. Broader media, collaborative document/conflict/history journeys, account recovery, provider login, Safari/WebKit, accessibility, offline/resume and performance acceptance remain additional work.
