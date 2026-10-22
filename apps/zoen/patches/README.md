# Maintained dependency patches

The app's existing `postinstall` runs `patch-package --error-on-fail`.
`react-native-enriched-markdown@1.1.1` uses the numeric sequence supported by
the pinned `patch-package@8.0.1`:

1. `001+native-contracts`: podspec, native props/codegen and draft contracts.
2. `002+draft-snapshot-engine`: detached snapshot and attribute restoration.
3. `003+input-lifecycle`: native mutation boundaries and input lifetime guards.
4. `004+draft-checkpoint-commands`: the complete draft-checkpoint method section.
5. `005+native-no-media`: parser admission and both native renderer/cache paths.
6. `006+public-js-bindings`: input props plus Markdown JS/types/platform gates.

Each patch is an authored maintained file below 20,000 bytes. They are sequential
diffs of complete package states, not arbitrary slices of a large diff. The two
input stages preserve the same original Objective-C implementation and all method
bodies. Only the complete final sequence is a buildable, qualified dependency.

The series reproduces the previous monolithic patch's complete final package
bytes, modes and paths. Qualify changes from the exact pristine 1.1.1 tarball
against `package-lock.json` integrity. Run real patch-package apply, repeat apply,
reverse, and partial/mismatched-state checks; compare the entire resulting package,
not only the edited files. Sequence bookkeeping adds `.patch-package.json`; check
that metadata separately from package source equality.

Patch-package records applied patch hashes and refuses changed applied stages.
A failed stage must exit nonzero and stop later stages. On a pristine install it
cannot infer that a disjoint patch was deleted from version control: verify all
six expected stages and the full final source manifest during qualification.

For edits to a non-final stage, use patch-package's documented `--rebase` workflow
and regenerate successor diffs as needed. Do not hand-split hunks or substitute
the installed package for the pristine baseline. Retained generated/vendor assets
in a qualified native overlay are additional build inputs; preserve and verify
them separately instead of replacing that overlay with a source-only package.
