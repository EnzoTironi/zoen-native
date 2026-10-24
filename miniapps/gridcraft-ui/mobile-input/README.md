# Grid primary-touch input recipe

Status: source 6d05c8452c7dde7449c8fb0b81e0d2b3bf395563 qualified by check14 Clippy×2 + actual13 UI + Release WASM and browser14/15; iPhone software-keyboard and deployment acceptance remain pending.

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
