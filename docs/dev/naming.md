# Zoen naming cutover

The product and built-in agent are Zoen. Some source packages, generated bindings, preferences and development flags still use Roda. Their presence is unfinished naming work. Zoen has not launched and has no production users or data; the [prelaunch policy](prelaunch-policy.md) supersedes the previous compatibility-preservation plan.

## Atomic cutover inventory

| Current identifiers | Target | Required coordinated work |
| --- | --- | --- |
| `crates/roda-*`, `roda_*` Rust imports | `crates/zoen-*`, `zoen_*` | Move packages, workspace/lock/features, callers, scripts and tests together; verify complete CI/setup. |
| `RodaCore`, `RodaFFI`, `RodaEngine`, `roda_ffi` | `ZoenCore`, `ZoenFFI`, `ZoenEngine`, `zoen_ffi` | Regenerate Swift/Kotlin and native libraries together; update every client and supported ABI. |
| Apple `Roda…` preferences and launch flags | `Zoen…` | Replace names and callers directly; reset affected disposable preferences/fixtures, without old aliases or dual reads. |
| `buildRodaCore`, generated build directories | Zoen task/directory names | Update Gradle inputs/outputs, producer/verifier scripts and CI in one unit; clean rebuild. |
| Old local database names and resource URI namespaces | One current Zoen namespace | Reset affected development/test databases and seed fixtures coherently. Do not add old-path discovery or URI aliases. |
| Cryptographic/protocol domains, if changed | One current explicit domain/version | Update both peers, proof/signature/encryption tests and setup atomically. Preserve domain separation and authenticated-byte correctness; recreate affected disposable signed history instead of silently rewriting it. |

Complete each coherent unit on shared main with one owner/coordinator, update documentation and validate its callers. Do not move another owner's files while they are being edited. Database migration-baseline consolidation is a separate explicit coordinated operation with affected database resets; this handoff performs no consolidation.

The [12 October checkpoint](handoff-2026-10-12.md) records the current names and unfinished bindings/caller work. It does not claim that this cutover is complete. Revisit the prelaunch policy before the first production deployment.
