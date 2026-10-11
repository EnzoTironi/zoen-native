# Development setup

Use the integrated main tree for backend baseline work and fetch the exact feature PR head when testing changes in review. The [version ledger](../roadmap-status.md#version-ledger) identifies those heads. Keep another owner's dirty checkout intact.

## Rust and service journeys

```sh
rustup toolchain install 1.99.0 --profile minimal --component rustfmt,clippy
export RUSTUP_TOOLCHAIN=1.99.0
rustup target add wasm32-wasip2
cargo fmt --all --check
```

Run `scripts/dev-stack.sh` in a separate terminal. It remains attached to the relay process.

The complete CI setup also provisions PostgreSQL, FoundationDB, NATS, gVisor and Firecracker/browser images. Use [.github/workflows/ci.yml](../../.github/workflows/ci.yml) as the executable setup reference. A host without those dependencies cannot validate the complete suite by running `cargo test` alone.

## Apple

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

## Android and web

Until their PRs integrate, use the README and verification instructions in [Android PR 41](https://github.com/EnzoTironi/zoen-native/pull/41) and [web-shell PR 44](https://github.com/EnzoTironi/zoen-native/pull/44). Match each native library with its generated bindings. A compile that skips the Rust ABI task is not an Android runtime result.

The web shell uses sample data. It is suitable for layout and interaction review; account enrollment, encrypted messaging and browser storage remain separate completion gates.

## Public staging

Current clients require protocol 4. The public relay was last observed on protocol 2. Verify negotiation before using it for a current-client journey. Build a same-source isolated relay for development rather than treating a health response as protocol compatibility.
