# Zoen · shared app

Expo SDK 58, React Native 0.88 and React 19.3, targeting iOS, Android and web.
This is an active reconstruction. `acceptance.json` records the required journeys;
the existing clients remain the comparison baseline until those journeys pass.

## Architecture

- Expo Router owns navigation. `@expo/ui` supplies native SwiftUI/Compose controls.
  FlashList handles message history; Keyboard Controller handles the composer.
- `modules/zoen-core` owns each native Rust engine and its Keychain/Keystore vault.
  Private signing keys stay outside JavaScript. Generated UniFFI bindings are build artifacts.
- `src/core/generated` is the TypeScript contract exported from Rust with `ts-rs`.
  Signed 64-bit money, timestamps and sequence numbers cross the boundary as decimal strings.
- `ClientStore` holds disposable UI projections. Rust owns durable account, conversation,
  outbox and document state. Presentation preferences use a separate device-only store.
- The browser adapter uses the existing authenticated Rust/WASM worker, protected browser
  vault and exclusive Web Lock. Browser feature parity is still in progress.
- Native projects use Continuous Native Generation. Configure them through `app.json`
  and module/config plugins; do not edit generated `ios/` or `android/` files.

## Prerequisites

Node.js, npm, Rust/Cargo, and the platform's native toolchain are required.
For iOS, use full Xcode and CocoaPods; for Android, use a JDK and Android SDK/NDK.
Point `DEVELOPER_DIR` at Xcode if `xcode-select` selects Command Line Tools.

```sh
rustup target add aarch64-apple-ios aarch64-apple-ios-sim \
  aarch64-linux-android x86_64-linux-android wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked
cd apps/zoen
npm ci
```

Native Rust output is generated before the development build. Expo Go cannot load
the local Rust module; use a development build.

```sh
npm run ios          # Apple Silicon simulator
npm run ios:device   # Physical iPhone; includes device + simulator Rust slices
npm run android
npm run web          # Rebuilds and pins the WASM worker before starting Metro
```

`ZOEN_ANDROID_ABIS=arm64-v8a` can narrow a local Android build. The default includes
arm64 and x86_64. Native cache identity includes the Rust source, profile and target
architectures, so a simulator or debug artifact cannot satisfy a device/release request.

```sh
npm run core:types
npm run check        # TypeScript, Oxlint, Oxfmt, client tests
npm run format       # Oxfmt
npm run ios:release
npm run android:release
npm run export:web   # Release WASM + static Expo web export
```

The Expo CI workflow runs the generated-contract check, TypeScript, Oxlint, Oxfmt,
the shared client tests and Expo Doctor. `npm run lint` uses Oxlint; `npm run format`
uses Oxfmt. Generated bindings and native projects stay outside formatter/linter input.

## Web export and hosting

`npm run export:web` exports the application and its isolation policy into `dist/`.
The [Cloudflare deployment guide](../../infra/cloudflare/README.md) describes the
production Workers preview, fresh production builds and the outstanding canonical
domain/relay setup. Local acceptance uses the actual Pages runtime; an export with
loopback QA configuration must never be deployed.

The app and its MLS signing worker require `Cross-Origin-Opener-Policy: same-origin`
and `Cross-Origin-Embedder-Policy: require-corp`. Only the `/sign-in` document and its
static HTML aliases use `same-origin-allow-popups` and `unsafe-none`, so official
provider popups work. Entering the app requires a full document navigation back to
an isolated page. OAuth credentials stay in memory and are posted to the relay;
they must never appear in navigation URLs. The sign-in entry must not start the
WASM signing worker.

[Pages combines overlapping header values](https://developers.cloudflare.com/pages/configuration/headers/).
The exact entry rules explicitly detach both headers before adding their policy;
adding a second value without detachment breaks browser policy parsing. Keep zone
response-header rules from appending another COOP/COEP value. If deployment later
uses Pages Functions, attach the policy to those responses in the function, because
static `_headers` rules do not cover function responses.

Verify the actual Pages header engine locally, with no Cloudflare credentials or deployment:

```sh
# From the repository root; Wrangler is a separate tool, not an app runtime dependency.
npx --yes --package=wrangler@4.151.1 -c 'node scripts/check-expo-web-headers.mjs'
# Or point WRANGLER_CLI at an already installed Wrangler executable.
```

This fixture checks the canonical entry, both clean-URL redirects, a query variation,
lookalike paths, the worker asset and 404s. OAuth provider origins and callback URLs
must separately match the public IDs in the app and the relay audience/origin
configuration in [provider-auth.md](../../docs/dev/provider-auth.md).

The development-only demonstration uses a separate Rust database. It never replaces
an account or contacts the relay. Real account and device-link tests should use an
isolated local relay and dedicated simulators/emulators.

## Verification and remaining work

The whole-application iPhone acceptance baseline remains signed v85; the latest
installed v92 artifact has bounded MCP/gesture/Home evidence. The later v94/v95
Page input and session work has source/host proof without a new signed editor
recovery journey. Exact artifacts and the immutable history are in
[`acceptance.json`](acceptance.json).

The [Web release journey](../../docs/dev/expo-web-acceptance.md) passed 19 local
checkpoints in each of English and Portuguese, including normal linking, encrypted
messages, Markdown drafts, a builtin MCP list, plain files and local erasure.
Onboarding accessibility and public HTTP hosting have separate bounded evidence.
Android's current APK and 14 JVM tests pass; live emulator acceptance is pending.

Current validation covers iOS, Web and Android; desktop is deferred. Full product
parity, provider/physical-device paths, remaining media/document workflows,
extensions and broad accessibility/performance remain open. Keep the existing
clients until their required replacement journeys pass.
