use crate::fixture::Result;
use egui_kittest::Harness;
use pdfcraft_ui_egui::PdfCraftApp;
use serde_json::{Value, json};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

pub fn actual_app(bytes: Arc<Vec<u8>>, out: &Path) -> Result<Value> {
    let mut app = PdfCraftApp::new();
    app.session.set_javascript(false);
    app.open_bytes("Shared review.pdf", None, bytes.as_ref().clone())?;
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .build_eframe(move |_| app);
    harness.run_steps(4);
    let started = Instant::now();
    while harness.state().render_pending() && started.elapsed() < Duration::from_secs(20) {
        harness.run_steps(2);
        std::thread::sleep(Duration::from_millis(25));
    }
    harness.run_steps(3);
    if harness.state().render_pending() {
        return Err("actual_app_render_timeout".into());
    }
    let (_, id) = harness
        .state()
        .active_ids()
        .ok_or("actual_app_has_no_active_document")?;
    if harness
        .state()
        .session
        .get(id)
        .ok_or("actual_app_document_missing")?
        .info
        .pages
        .len()
        != 2
    {
        return Err("actual_app_page_count_changed".into());
    }
    harness
        .render()?
        .save(out.join("upstream-editor-original.png"))?;
    if !harness.state_mut().execute("edit.undo") {
        // No edit exists: this command must remain unavailable in the original App.
        return Ok(
            json!({"executed":true,"pages":2,"emptyUndoUnavailable":true,
            "originalUpstreamChrome":true,"nativeHarnessOnly":true}),
        );
    }
    Err("empty_document_exposed_undo".into())
}
