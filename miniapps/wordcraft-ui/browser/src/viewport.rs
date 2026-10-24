//! Keep the actual Word renderer inside the host-provided safe content area.
use wordcraft_ui_egui::WordApp;

pub(crate) fn show(app: &mut WordApp, ui: &mut egui::Ui) {
    app.ui(ui);
}

#[cfg(test)]
mod tests {
    use super::*;
    use wordcraft_doc::Document;
    use wordcraft_engine::Session;
    use wordcraft_ui_egui::Services;
    use wordcraft_zoen_host::{ActorKind, Head, Open, Scope, browser::BrowserBridge};

    fn opened() -> Result<WordApp, String> {
        let bytes = wordcraft_zoen_host::profile::encode("")?;
        let mut session = Session::new(Document::new());
        BrowserBridge::default().open(
            &mut session,
            Open {
                scope: Scope {
                    artifact_id: "viewport-document".into(),
                    actor_id: "human".into(),
                    actor_kind: ActorKind::Human,
                },
                head: Head {
                    version: "1".into(),
                    sha256: wordcraft_zoen_host::hash(&bytes),
                },
            },
            &bytes,
        )?;
        Ok(WordApp::new(session, Services::default()))
    }

    #[test]
    fn real_word_canvas_and_caret_remain_in_current_host_safe_content() -> Result<(), String> {
        let mut app = opened()?;
        let original = app.session.doc.clone();
        let ctx = egui::Context::default();
        // The host close rectangle ends at 114; its existing exclusion adds 4.
        // Resize and keyboard changes must use the current content rect, not the root UI.
        for (width, height, top, bottom) in [
            (402.0, 874.0, 118.0, 34.0),
            (874.0, 402.0, 70.0, 20.0),
            (874.0, 220.0, 70.0, 0.0),
            (402.0, 874.0, 0.0, 0.0),
        ] {
            for _ in 0..6 {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, height),
                    )),
                    safe_area_insets: Some(egui::SafeAreaInsets(egui::epaint::MarginF32 {
                        top,
                        bottom,
                        left: 0.0,
                        right: 0.0,
                    })),
                    ..Default::default()
                };
                let mut output = ctx.run_ui(input, |ui| {
                    app.logic(ui.ctx());
                    show(&mut app, ui);
                });
                // This geometry harness has no GPU; explicitly dispose the real
                // texture updates rather than triggering egui's drop guard.
                output.textures_delta.clear();
            }
            let safe = ctx.content_rect();
            let canvas = app.canvas.canvas_rect.ok_or("missing actual canvas")?;
            assert!(
                safe.contains_rect(canvas),
                "canvas {canvas:?} escaped host content {safe:?}"
            );
            let page = app.canvas.page_rects.first().ok_or("missing actual page")?;
            assert!(
                page.top() >= safe.top(),
                "empty page is behind the host close exclusion"
            );
            let (caret, height) =
                wordcraft_ui_egui::canvas::caret_screen(&mut app).ok_or("missing real caret")?;
            assert!(
                safe.contains_rect(egui::Rect::from_min_size(caret, egui::vec2(1.0, height))),
                "caret must be visible below host chrome"
            );
            assert_eq!(
                app.session.doc, original,
                "responsive layout must not edit accepted content"
            );
        }
        Ok(())
    }
}
