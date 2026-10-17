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

Production web hosting must send `Cross-Origin-Opener-Policy: same-origin` and
`Cross-Origin-Embedder-Policy: require-corp`, as the local Metro middleware does.
The browser signing bridge needs the resulting cross-origin isolation.

The development-only demonstration uses a separate Rust database. It never replaces
an account or contacts the relay. Real account and device-link tests should use an
isolated local relay and dedicated simulators/emulators.

## Verification and remaining work

The iOS development app builds and opens on a dedicated simulator. Shared-client
concurrency/history/money tests and the existing web regression suite have passed.
See `acceptance.json` for the current evidence and limitations. Full media integration,
device linking/recovery, document-editor completion, mini-apps, native extensions,
Android runtime acceptance, web parity and release acceptance remain required.
Do not remove the current native clients or claim a completed migration from scaffolding
or compilation alone.
