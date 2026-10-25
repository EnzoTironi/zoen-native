//! Consume current host insets once, as in the qualified managed editor foundation.
pub(crate) fn show(ui: &mut egui::Ui, render: impl FnOnce(&mut egui::Ui)) {
    let safe = ui
        .available_rect_before_wrap()
        .intersect(ui.ctx().content_rect());
    if safe.is_positive() {
        ui.scope_builder(
            egui::UiBuilder::new()
                .id_salt("managed_pdf_canvas")
                .max_rect(safe),
            |ui| {
                ui.set_clip_rect(ui.clip_rect().intersect(safe));
                render(ui);
            },
        );
    }
}
