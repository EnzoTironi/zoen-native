# Standalone-client retirement

The product application is [React Native/Expo in `apps/zoen`](../../apps/zoen/README.md),
with the shared Rust core and active Swift/Kotlin platform modules. The earlier
standalone SwiftUI and Compose clients are retired from the current tree. Their
complete source, fixtures and test history remain available at
[revision 5149ae48be3b104fbe23b4e6a91bef80f131844c](https://github.com/EnzoTironi/zoen-native/tree/5149ae48be3b104fbe23b4e6a91bef80f131844c).

## Active build inputs

- Swift and Kotlin under `apps/zoen/modules/zoen-core` remain active application
  code. Generated native projects live under `apps/zoen/ios` and
  `apps/zoen/android`; configure them through Expo and module plugins.
- Android UniFFI configuration moved byte-for-byte from the standalone root to
  `apps/zoen/modules/zoen-core/android/uniffi.toml`. Both the Android generator and
  its source/ABI contract include that path. Generated Kotlin/native output is
  under `apps/zoen/modules/zoen-core/generated/android`.
- `scripts/build-expo-core.mjs`, `scripts/build-android-core.sh`, the core-contract
  and callback checks, and `.github/workflows/expo.yml` build and verify the shared
  application. The Rust/backend CI workflow remains active.
- The standalone `android.yml` and `apple-design.yml` workflows and their
  standalone-only build, device, art-producer and journey scripts are removed.
  Historical Apple setup/media guides are labeled and link to the archived tree.
  Current setup is [here](setup.md); the local backend composition is documented
  in [infra/local](../../infra/local/README.md).

## Retained assets and tests

The reusable shell keeps all 16 original semantic light/dark color pairs in
[`semantic-palette.json`](../../packages/zoen-ui/preview/fixtures/semantic-palette.json),
with their immutable source revision/path/hash. Its tests compare the active CSS
against that contract instead of reading a retired Swift file.

MapTap keeps its existing public-domain Natural Earth coastline bytes and globe
math. The globe test pins the retained asset's SHA256 rather than depending on a
second copy in the archived client. Existing Expo miniapp art remains unchanged;
its README identifies archived sources as provenance, not active inputs.

The active Android core-contract suite retains its ABI/source/callback checks and
adds configuration-change invalidation at the new path. The raw namespace delta
helper moved into the active contract module. Four tests specific to the retired
standalone art producers and Compose source-proof importer retire with those
producers; they are not substitutes for current application acceptance. The
standalone native test suites remain reviewable in Git history.

The current application tests, assets, 30 acceptance journeys and readiness flags
are preserved. Source retirement is not proof of feature parity or completed
platform acceptance. iOS remains the manual acceptance priority; Android, Web and
desktop retain their stated product requirements. Existing local build outputs
under the retired roots stay ignored; this change does not migrate or clear user
accounts, databases, vaults or installed applications.
