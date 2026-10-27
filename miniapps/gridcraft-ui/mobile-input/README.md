# Grid primary-touch input recipe

Status: source 4b5e45575c5c33b4b30185706c59824661b20d2f passed host/WASM Clippy, actual14 UI, Release WASM and actual sandbox browser touch/IME/content/Undo/Redo/freeze plus current-owner focus negatives. Original 6d05 assets remain immutable; new bytes require a new runtime prefix. iPhone software-keyboard and landscape acceptance remain pending.

This recipe extends the existing pinned Grid producer. It does not replace its source pin, managed-command policy, host bridge, browser ABI or document authority.

Order in the existing producer:

1. Materialize the existing pinned Grid source and patches 0001–0003.
2. Copy the current host/browser sources as usual (including `ui_tests.rs`).
3. Run `apply.py PREPARED --eframe-crate EFRAME_ARCHIVE --egui-crate EGUI_ARCHIVE` with Python 3.12 or later. The manifest pins the exact registry archives and verifies each patch and baseline source before mutation.
4. Apply the existing backend 0004 refused-drag undo patch.
5. Run normal offline Cargo metadata to generate the lockfile. The only registry-source changes are the same-version eframe/egui path overrides; do not hand-edit the lockfile.
6. Record all recipe inputs in the existing producer provenance. Run the normal compiler/native/browser qualification. Use the separately qualified JS/WASM hashes when packaging; retain old immutable runtime URLs.

The helper is ephemeral input ownership. A real primary contact must still own the current document, sheet, canonical cell and actual field. The custom trigger publishes IME geometry from the actual rendered TextEdit output. Output admission reads the last completed widget registration without replaying `read_response`, whose normal click-elsewhere policy can surrender focus. Neither global focus policy nor document persistence changes.

`tests/touch_ui_tests.rs` is a real egui input regression leaf, registered after the host sources are copied. It covers first formula entry, cell entry and handoff, actual values and XLSX snapshot, cancellation, replaced documents, stale/wrong/no-IME pulses, frozen CAS and one-shot consumption. Existing keyboard/undo regressions remain intact.

The scratch replay receipt proves exact reconstructed bytes, not compilation or browser acceptance. Browser proof does not prove the iPhone software keyboard.

An admitted trusted tap retries canvas focus on release before computing current
IME output. Press-time refusal must not leave the first text tap inactive.
The existing owner gate excludes drags, cancellation and secondary contacts;
an already focused text agent retains its composition and buffer.
For the real Word bootstrap regression, set `ZOEN_TEST_PRESS_FOCUS_REFUSAL=1`.
This test-only policy refuses press-time canvas focus without focusing any element
or changing events. The existing first-touch/input/content checks must still pass.
It is a fault model, not evidence of the iPhone's exact focus policy; current
patches still require normal per-consumer compilation and native qualification.

When the rendered platform output has no current IME/editor, the DOM text agent
must leave hit-testing so its old caret cannot obstruct adjacent cells. The same
agent becomes interactive again when the current editor supplies IME output,
before the existing contact/owner activation gate runs. This does not clear its
buffer, move its range, interrupt composition or add a focus/blur workaround.
The update runs even when IME geometry is cached (including repeated `None`).

The browser suite includes an independent synthetic document: trusted A1 edit →
Enter → landscape resize → touch the old caret footprint in B1. It verifies the
actual hit target, B1 content, trusted composition-start/input and caret (CDP composition-end is observed untrusted), normal Undo/Redo,
cancellation and frozen editing. CSS values are observations, not the behavioral
contract. The original native observation is narrower: B1 did not activate after
rotation; Chrome reproduces a concrete stale-target mechanism, but does not prove
the exact native hit coordinate or the iPhone software keyboard.
