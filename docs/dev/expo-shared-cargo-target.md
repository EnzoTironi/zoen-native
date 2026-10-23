# Expo Core with an explicit shared Cargo target

`scripts/build-expo-core.mjs types|ios` routes builds through the existing
`cargo-source-guard.py` when `CARGO_TARGET_DIR` is explicitly set. Python 3.11+
is required; select it with `ZOEN_CARGO_GUARD_PYTHON` if `python3` is older.
The default, unset-target path retains its existing isolated Cargo behavior.

The guard owns the lease, source/configuration stamp, local-workspace invalidation
and third-party cache preservation. These are cooperative cache guarantees,
not a security sandbox against an unrelated process writing into the target.
The builder does not automatically adopt a cache or wait for another owner's lease.
An unknown nonempty cache fails with the guard's diagnostic and a receipt path.

After confirming that such a target is idle, an operator can explicitly adopt it
with the public guard. For example, from the workspace root:

```sh
python3 scripts/cargo-source-guard.py \
  --manifest-path "$PWD/Cargo.toml" --target-dir "$CARGO_TARGET_DIR" \
  --receipt /absolute/new/adoption-receipt.json --adopt-idle-target \
  -- build --locked -p client-core
```

The receipt's parent must already exist and its filename must be new. This is an
explicit operator decision, not an instruction to adopt a target that is busy.
The actual builder will then qualify its own commands and configurations.
Guarded builds are offline/locked; acquire missing dependencies separately.

Each invocation creates a private directory beneath
`apps/zoen/modules/zoen-core/generated/cargo-source-guard/`. Its individual guard
receipts bind the actual Cargo manifest, source inputs, toolchain, configuration
and output hashes. The builder declares its tooling files and the SDK/deployment,
library search and DeckCraft resource-manifest environment inputs. Guarded builds
consolidate `CARGO_BUILD_BUILD_DIR` into the selected target, as the guard requires.
No environment values are added to builder receipts; the guard records hashes.

The builder verifies and copies each required executable/library into that private
directory before consuming it. Hash mismatch, missing/ambiguous output or changed
workspace inputs between commands refuses qualification. This protects later
generator/framework steps from a cooperating build replacing shared target files
after a command releases its lease. Executable copies are owner-executable and
read-only; library copies are owner-readable and read-only. There is no automatic
retry or cleanup of another invocation's receipt/artifact directory.
Use one builder per checkout's generated-output tree; the Cargo lease does not
serialize two generators writing into the same checkout. Private copies and
receipts can be reclaimed explicitly after their outputs are no longer needed.

The types exporter is still a dev-profile example with `client-types`. The iOS
host/device/simulator builds retain their selected profiles, SDKs and deployment
target, while the binding generator retains its original dev profile. Generated
names, exact framework inventories and the resource-manifest input remain the
same. A guarded invocation always requalifies its Cargo commands rather than
accepting an older generated-output stamp. On success the existing platform
stamp references `qualification.json`; on failure it is not replaced. Initial
guard/build failure leaves the previous generated iOS framework intact.

Android still delegates to the existing `build-android-core.sh` pipeline, including
`cargo-ndk`/`cargo rustc`, which the existing guard does not support. Explicit shared
Android targets retain their prior behavior and are **not** source-qualified by
this integration. #192 remains open for that path. This slice is not a native app
build, runtime, signing or device-acceptance claim.

Entrypoint checks (tiny Rust fixture only, no Core/native build):

```sh
ZOEN_CARGO_GUARD_PYTHON=/path/to/python3.12 \
  node --test scripts/tests/build-expo-core.test.mjs
```
