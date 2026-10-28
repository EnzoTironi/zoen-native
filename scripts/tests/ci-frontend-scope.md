# Frontend-only Rust CI selection

The existing `ci-documentation-scope.py` selector now emits `run_rust` alongside
its documentation result. Only a positive, verified pull-request class emits
`run_rust=false`. Missing outputs, selector failure, unknown events and pushes
keep the full Rust job. Its timeout, Clippy flags, failures and journey assertions
are unchanged. Existing running CI is never cancelled or relabelled by this code.

The frontend class accepts regular added/modified TypeScript under
`apps/zoen/src`, with a single exact ancestral base. When the PR branch is behind its event base,
the workflow instead classifies base-to-candidate changes only after verifying
the actual checkout has the exact event base/head parents and the canonical
conflict-free merge tree. The same resource, reader and pin checks apply at the
base and tested candidate; missing or forged candidates retain Rust. Generated code, vendor
resources, platform/native bridges, critical Core type/driver contracts and
non-TypeScript files are excluded. Mixed changes, renames, deletions, symlinks,
mode changes, empty/binary inputs and uncertain Git history retain Rust.

`ci_frontend_inputs.py` is data for this same selector, not another job authority.
It pins reviewed Cargo roots, CI/Expo commands, app package coverage, resource
recipes and manifests at both endpoints. The embedded-manifest inventory must
match; unreadable, malformed, missing, new or drifted inputs retain Rust. The
actual Deck manifest's compiled `sources` closure and other nested provenance
are checked: changing a declared frontend input retains Rust even without a
literal `include_str!` of that TypeScript. Cross-boundary references in tracked
text/shared JSON and broad Rust app readers also retain Rust. Filename tokens
must match exactly; `attachment-composer.tsx` is not the unrelated filename
`durable-attachment-composer.tsx`.

Two exact pinned Rust bodies use the string `apps` as data: disabled Codex
features and Apple's association JSON. They are not filesystem readers. Any
change to those bodies disables the optimization; new/unknown occurrences keep
Rust selected. Pin drift is a full fallback, never permission to ignore a failed
check. Future source/recipe changes need their own reviewed evidence before a
pin refresh can restore the optimization.

Only the heavy `rust` job uses the new output. Expo client/Android/release-web,
web E2E, file-size and local-integrity checks retain their selection rules.
Existing source-guard, QA/Clef choice and Expo entrypoint checks move to
`local-integrity`, including their actual commands. Those tooling fixtures can
compile tiny isolated Rust workspaces even when application Rust is skipped.
No omitted application compilation or journeys are claimed as a test PASS.

Run the deterministic classifier/workflow tests without Cargo:

```sh
python3 -m unittest discover -s scripts/tests -p 'test_ci*scope.py' -v
```

Tests create isolated real Git histories, with automatic maintenance disabled,
and cover positive selection, full fallback, JSON/output contracts, direct and
transitive resource inputs, invalid/stale/ambiguous bases and guard preservation.
This efficiency slice is separate from the cold `agent-inspect` defect #519.

Roadmap JSON references are evidence links only when the existing static-evidence
validator proves their consumer boundary at both endpoints. A new material
consumer or unknown evidence file retains Rust. One complete reviewed reader,
`test-android-core-contract.py`, builds synthetic files under its own temporary
root; its whole-file digest and regular mode must match at both endpoints.
All other basename readers, including qualified paths and delayed transformations,
remain conservative inputs. No per-route exceptions or roadmap-content hashes
are added.

## Standalone photo geometry

Codex proposed this narrow #200 optimization after #666 selected all four Rust
shards for only `ZoenPhotoEditorImage.swift` and `PhotoEditorImageTests.swift`.
The helper imports only Foundation/CoreGraphics. The maintained macOS fixture
also imports AppKit/PencilKit for #666's real PKDrawing/extended-range RGB recrop
reproduction; those imports are admitted only for that fixture. PencilKit's
profile is OS-dependent, so a second explicit float/extended-RGB pixel case
qualifies that fallback without weakening the real PencilKit recrop assertions. Both compile and link as
an independent pixel executable, and have no verified Rust/resource consumer.
The selector reuses the existing Cargo/resource pins and cross-boundary reader
checks at both endpoints; unknown readers, imports, FFI declarations, mixed
paths, modes, deletions, stale/unverified merge trees or pin drift retain Rust.
This is an exact two-path exception, not a Swift/native directory exemption.

The existing Node pixel fixture skips on Linux. A conditional macOS job now runs
that exact fixture without skipping and typechecks the helper against the
actual iPhoneSimulator SDK with Swift 6 strict concurrency/warnings as errors.
The existing required `rust` check demands native success for this lane: failure,
cancellation or skipped/missing qualification cannot become a successful Rust
replacement. Changes to the workflow/runner also select this proof alongside
the full Rust suite. The CI-policy PR itself therefore exercises both lanes.
The runner and workflow are pinned; changing/removing their protection restores
full Rust selection until reviewed. Other checks and branch protections remain.

Hosted run 40500782585 compiled successfully but exceeded the former 5-second
native execution deadline. The executable now has a bounded 15-second deadline
and writes stage timings directly to stderr; all assertions remain. Compilation
stays bounded at 45 seconds and the overall test at 60 seconds. A timeout still
fails the required native proof; the timings distinguish progress from a hang.

This removes application Rust tests that cannot observe these image pixels.
It does not claim an App build, editor/journal integration, device acceptance or
coverage of arbitrary native code. Those existing qualifications remain separate;
no running/historical CI job is cancelled or relabelled by this policy.
