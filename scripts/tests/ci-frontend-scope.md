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
consumer or unknown evidence file retains Rust. Basename collisions are ignored
only inside a complete unescaped repository-qualified literal naming another
regular source blob at both endpoints; bare, relative, constructed, unknown,
symlinked or changed targets remain conservative inputs. No per-route reader
exceptions or evidence-content hashes are added.
