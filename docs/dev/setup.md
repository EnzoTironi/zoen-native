# Development setup

Use current `main` for integrated development and the exact PR head when testing a change in review. The shared iOS, Android and Web application is [apps/zoen](../../apps/zoen/README.md). The older `apple/`, `android/` and `packages/zoen-ui/web` clients remain separate comparison and test targets. Their builds and journeys do not establish acceptance of the Expo application.

Keep another owner's checkout, devices and running services intact. Use an owned checkout, synthetic accounts and dedicated simulator/emulator storage for account, recovery and erasure journeys. The [prelaunch policy](prelaunch-policy.md) defines the durable-state and fixture boundaries.

## Shared Expo application

The application currently pins Expo SDK 58, React Native 0.88 and React 19.3 in [package.json](../../apps/zoen/package.json). Use Node.js 26 and npm, matching [Expo CI](../../.github/workflows/expo.yml). That workflow pins Rust 1.99.1; the backend CI toolchain is listed separately below.

From the repository root:

```sh
rustup toolchain install 1.99.1 --profile minimal
export RUSTUP_TOOLCHAIN=1.99.1
npm ci --prefix apps/zoen
```

Install the Rust targets and platform prerequisites listed in the [app README](../../apps/zoen/README.md#prerequisites) for the platform you will run. Web also requires the matching `wasm-bindgen-cli` version recorded there and in Expo CI.

Run the appropriate command from `apps/zoen`:

```sh
npm run ios          # Apple Silicon simulator; requires Xcode and CocoaPods
npm run ios:device   # Physical iPhone; requires signing and a dedicated test device
npm run android     # Requires JDK and Android SDK/NDK
npm run web         # Rebuilds the Rust/WASM worker before starting Metro
```

These scripts build the corresponding Rust core before starting Expo. Native uses the local `modules/zoen-core` module and requires a development build. Expo Go cannot load that module. Native projects use Continuous Native Generation: edit app configuration and config plugins instead of generated `ios/` or `android/` files. Use the [SDK 58 reference](https://docs.expo.dev/versions/v58.0.0/) for Expo APIs and `npx expo install <package>` for SDK-compatible dependencies.

Before declaring an app change complete, run from `apps/zoen`:

```sh
npm run core:types
git diff --exit-code -- src/core/generated
npm run check
```

`core:types` regenerates the checked-in TypeScript contract from Rust. For an intentional contract change, review and commit the generated changes with the callers; the diff check must be clean when checking that committed source. `check` runs TypeScript, Oxlint, Oxfmt and client tests. Use `npm run format` for Oxfmt. Regenerate matching native bindings and libraries after Rust interface changes; a passing TypeScript contract check does not verify a native ABI or runtime. Expo CI also runs Expo Doctor and platform-specific build/resource/release checks.

The Web adapter uses authenticated Rust/WASM, a protected browser vault and an exclusive browser lock. Current bounded evidence is recorded in [Web acceptance](expo-web-acceptance.md), [Android acceptance](expo-android-acceptance.md), [iPhone evidence](expo-native-surfaces.md) and the [acceptance manifest](../../apps/zoen/acceptance.json). Those records retain their exact artifact and journey boundaries; broader product parity remains incomplete. Hosting and required Web isolation headers are documented in the [app README](../../apps/zoen/README.md#web-export-and-hosting).

## Rust and service journeys

```sh
rustup toolchain install 1.99.0 --profile minimal --component rustfmt,clippy
export RUSTUP_TOOLCHAIN=1.99.0
rustup target add wasm32-wasip2
cargo fmt --all --check
```

Run `scripts/dev-stack.sh` in a separate terminal. It remains attached to the relay process.

The [owned local server environment](../../infra/local/README.md) provides the native server with PostgreSQL, FoundationDB and S3-compatible storage in isolated containers. Follow that guide for ownership, readiness, backup and restore. It is a separate workflow from the host-service journey setup below; do not start or reset an existing instance owned by someone else.

The complete CI setup also provisions PostgreSQL, FoundationDB, NATS, gVisor and Firecracker/browser images. Use [.github/workflows/ci.yml](../../.github/workflows/ci.yml) as the executable setup reference. A host without those dependencies cannot validate the complete suite by running `cargo test` alone.

## Existing Apple client

Install Xcode and XcodeGen, then build the core and generate the project:

```sh
rustup target add aarch64-apple-ios aarch64-apple-ios-sim aarch64-apple-darwin
scripts/build-core.sh
cd apple
xcodegen generate
```

The build script produces the XCFramework and matching UniFFI bindings. Regenerate both whenever shared-core APIs change. Existing package/build identifiers are listed in the [naming migration](naming.md).

Choose an available iOS simulator or the native Mac scheme in Xcode. Use a dedicated simulator, database and Keychain service for recovery tests. Demo UI recordings use explicit development flags and do not establish real-account encryption or transport.

For physical iPhone setup, read [installing on iPhone](instalar-no-iphone.md). For Apple Silicon's designed-for-iPhone destination, read [iPhone on Mac](iphone-no-mac.md). XCUITest requires a supported simulator or physical iPhone destination.

## Existing Android and Web clients

For the separate Kotlin/Compose client, use [android/README.md](../../android/README.md) and the [current-source binding contract](android-current-main.md). Match each native library with its generated bindings. A compile that skips the Rust ABI task is not an Android runtime result.

The reusable UI package and its separate authenticated Web shell are documented in [packages/zoen-ui/README.md](../../packages/zoen-ui/README.md) and the [Web source/build contract](../../packages/zoen-ui/web/README.md). A sample preview, shell test or shell build does not exercise the exported Expo Router application; its release journey runs in `expo / release-web-e2e`.

## Public staging

Current clients require protocol 4. The roadmap's public protocol-2 observation is historical, not a fresh compatibility check. Verify negotiation before using a public relay for a current-client journey. Use a same-source owned local relay for development rather than treating a health response or a hosted frontend as backend qualification. Provider login and production deployment retain their separate acceptance requirements.
