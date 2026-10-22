# Staged file size limit

Install after cloning (repeating the same installation is safe):

```sh
python3 scripts/install-git-hooks.py
```

Python 3 and Git are the only dependencies. The installer uses Git's effective
hooks directory, including `core.hooksPath` when configured. With the default
directory it also covers linked worktrees. It preserves other hooks and refuses
to replace an existing, different pre-commit hook or symlink; integrate the
versioned check into that hook manually in that case. When updating the versioned
hook, explicitly update the installed copy as part of that review; a different
existing hook is never silently overwritten.

The hook rejects staged maintained text files larger than **20,000 bytes**.
Estimated tokens are exactly **bytes / 4**: 20,000 bytes passes; 20,001 fails.
It inspects blobs in Git's index, including added, modified and renamed files.
Unstaged edits, untracked files and unchanged historical files are not inspected.
Symlinks, submodules and deleted files are skipped. No tokenizer or network is used.

Git cannot identify whether an agent or person wrote individual lines. The policy
therefore covers all maintained text and excludes artifacts by explicit repository
conventions in `.githooks/pre-commit`:

- Dependency lockfiles listed in `LOCKFILES` (Cargo, npm, pnpm, Yarn, Bun, CocoaPods,
  SwiftPM, Bundler, Composer, Poetry, uv, Pipenv, NuGet, Nix, Deno and Terraform).
- Code generation directories named `generated` or `__generated__`, filenames
  containing `.generated.`, and Expo's generated `apps/zoen/ios/` and
  `apps/zoen/android/` projects.
- Dependencies in `node_modules`, `vendor`, `vendored`, `third_party` or
  `third-party` directories. DeckCraft's pinned upstream license directory and
  `BOOTSTRAP-LICENSES.txt` are also excluded.
- Known binary media, archives, fonts and compiled/document formats listed in
  `BINARY_SUFFIXES`; other blobs containing NUL in their first 8 KiB are binary.
- The exact generated bundles and original external resources documented in
  [artifact exclusions](file-size-artifact-exclusions.md), with their actual
  producer or upstream provenance. Their authored source/builders remain checked.
- Exact unmodified upstream source imports listed in `PINNED_UPSTREAM_BLOBS`.
  Both the path and staged Git blob must match; editing that import removes its
  exception automatically. No content marker or commit-author claim is trusted.

Authored JSON, SVG, tests, documentation and upstream patch files remain subject
to the limit. Add exclusions only for actual generated or external artifacts,
with their producer or provenance established in review.

To split an oversized file:

1. Extract cohesive responsibilities into sibling modules. Keep the entry point
   and public API thin; keep every resulting file at or below 20,000 bytes.
2. Separate tests by behavior with intentional shared helpers/fixtures. Split
   documents by topic with index links; use supported includes for configurations.
3. Update imports, exports and callers, then run the affected checks.
4. Stage the revised files and new modules with `git add`, and commit again.

Do not minify, remove useful comments, arbitrarily slice by line count, or move
authored files into excluded paths to pass the check.

Run the real-index and installation regression suite:

```sh
python3 -m unittest discover -s scripts/git-hooks -p 'test_*.py' -v
```

Audit the whole tracked index with the **same policy**, including historical
files unchanged in the current commit:

```sh
python3 scripts/check-file-sizes.py
python3 scripts/check-file-sizes.py --json
```

The audit exits 1 while any violation remains, 0 only for zero violations, and 2
on an inspection error. Its scope is the tracked index; stage changes before
using it to assess the next commit. It does not change the staged-only hook.
Repository-wide remediation is tracked in #208; a passing staged hook alone
does not establish whole-codebase compliance.
