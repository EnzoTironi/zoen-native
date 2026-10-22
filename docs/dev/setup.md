# Development setup

The application is `apps/zoen`: React Native/Expo with active Swift/Kotlin modules
and a shared Rust core. Use the integrated main tree for the backend baseline and
the exact feature PR head for changes under review. Keep other owners’ checkouts,
simulators, accounts and retained service data intact.

## iOS application

Install the native dependencies described in the [application README](../../apps/zoen/README.md),
then use its current scripts:

```sh
cd apps/zoen
npm ci
npm run ios
```

`npm run ios:device` builds matching device and simulator core slices before the
physical-iPhone development client. `npm run ios:release` selects the Release
configuration. Signing and actual device acceptance remain separate from a build.
Do not edit generated Expo projects in `apps/zoen/ios` or `apps/zoen/android`.

Run `npm run check` for app changes. Regenerate the Rust TypeScript contract with
`npm run core:types` when its exported API changes, and regenerate native bindings
with the matching platform build script. Never pair bindings with a library from
another source revision.

## Rust and backend journeys

```sh
rustup toolchain install 1.99.1 --profile minimal --component rustfmt,clippy
export RUSTUP_TOOLCHAIN=1.99.1
cargo fmt --all --check
```

Run `scripts/dev-stack.sh` in a separate terminal. For the production-like local
composition, use [infra/local](../../infra/local/README.md). Current CI provisions
PostgreSQL, FoundationDB, NATS and sandbox prerequisites; see
[ci.yml](../../.github/workflows/ci.yml). `cargo test` without the required services
cannot qualify service journeys. Use disposable owned fixtures for failures and
restores; a health response alone does not establish protocol compatibility.

## Web and Android

Both use the same Expo application. Its `npm run android`, `android:release`,
`web` and `export:web` scripts select the correct binding and frontend builds.
The active Kotlin configuration is
`apps/zoen/modules/zoen-core/android/uniffi.toml`; source/ABI checks are part of
the Expo Android workflow.

Follow the [Web](expo-web-acceptance.md) and [Android](expo-android-acceptance.md)
acceptance records for their dated evidence and remaining gaps. iOS remains the
current manual acceptance focus. Desktop platforms retain their later delivery
phase in the product goal.

The [standalone-client retirement record](legacy-client-retirement.md) identifies
the archived source and migrated consumers. Its old build scripts and workflows
are no longer the application setup.
