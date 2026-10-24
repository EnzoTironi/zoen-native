//! Consume host insets once; an already-safe parent must not acquire extra padding.
#[derive(Default)]
pub(crate) struct Chrome {
    compact: Option<bool>,
    wide_dock: Option<(bool, bool)>,
}
impl Chrome {
    pub(crate) fn update(
        &mut self,
        panels: &mut photocraft_ui_egui::state::Panels,
        available: egui::Vec2,
    ) {
        // The desktop dock must not consume the entire phone canvas. Preserve
        // the upstream panel choices across rotations, without changing the doc.
        let compact = available.x < 700.0 || available.y < 450.0;
        if self.compact == Some(compact) {
            return;
        }
        self.compact = Some(compact);
        if compact {
            self.wide_dock = Some((panels.dock, panels.rail));
            panels.dock = false;
            panels.rail = false;
        } else if let Some((dock, rail)) = self.wide_dock.take() {
            panels.dock = dock;
            panels.rail = rail;
        }
    }
}
pub(crate) fn show(ui: &mut egui::Ui, render: impl FnOnce(&mut egui::Ui)) {
    let safe = ui
        .available_rect_before_wrap()
        .intersect(ui.ctx().content_rect());
    if safe.is_positive() {
        ui.scope_builder(
            egui::UiBuilder::new()
                .id_salt("embedded_safe_canvas")
                .max_rect(safe),
            |ui| {
                ui.set_clip_rect(ui.clip_rect().intersect(safe));
                render(ui);
            },
        );
    }
}
