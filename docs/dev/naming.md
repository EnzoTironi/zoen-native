# Zoen naming and compatibility

The product, default agent, repository, application bundle and relay use Zoen. Some technical identifiers still use the project's former name, Roda. They are inherited implementation names, not a second product or service.

## Migration inventory

| Existing identifier | Target | Required verification |
| --- | --- | --- |
| Nine `crates/roda-*` packages and `roda_*` Rust imports | `crates/zoen-*` and `zoen_*` | Workspace metadata, lockfile, all callers, feature combinations, full CI and journey setup. |
| `RodaCore`, `RodaFFI`, `RodaEngine`, `roda_ffi` generated bindings | `ZoenCore`, `ZoenFFI`, `ZoenEngine`, `zoen_ffi` | Regenerate Swift/Kotlin and native libraries together; verify each supported ABI and native consumer. |
| Apple `Roda…` development flags and preferences | `Zoen…` | Read existing preferences, preserve explicit old launch arguments during migration, write new keys and test relaunch. |
| `buildRodaCore` and generated build directories | Zoen-named tasks/directories | Gradle inputs/outputs, build scripts, CI, callback patching and clean rebuilds. |
| Local database paths containing the former name | Zoen path for new installs with legacy discovery | Preserve existing account/database ownership and migration versions; verify upgrade without creating a second account. |
| Built-in resource URIs already stored in signed Items | Zoen namespace for new resources with legacy resolution | Old mini-apps must still open; never rewrite signed history to change a label. |
| Signature/HKDF domains and persisted protocol bytes | Stable compatibility identifiers | A branding change must not invalidate signatures, encrypted state, backups or enrollment proofs. A protocol change needs its own versioned migration. |

Source/package naming can migrate without renaming cryptographic domains. Existing account data, signed history and published protocol compatibility need explicit preservation tests. The remaining legacy names in API/build documentation describe the current code until those changes are integrated.

Backend and native work are being updated in parallel. Publish the migration on a dedicated branch, integrate the latest reviewed source first, then regenerate bindings and validate old-data/new-client journeys before merging it. Avoid moving another owner's files while their changes are uncommitted.
