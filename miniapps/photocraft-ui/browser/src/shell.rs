use crate::{api, layout::Theme};
use eframe::App as _;
use photocraft_engine::Session;
use photocraft_ui_egui::{PhotocraftApp, Services};
use photocraft_zoen_host::browser::BrowserBridge;
use wasm_bindgen::{JsCast as _, JsValue};

pub async fn start() -> Result<(), JsValue> {
    let canvas = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("photocraft_canvas"))
        .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok())
        .ok_or_else(|| JsValue::from_str("missing_canvas"))?;
    eframe::WebRunner::new()
        .start(
            canvas,
            eframe::WebOptions {
                renderer: eframe::Renderer::Glow,
                ..Default::default()
            },
            Box::new(|cc| {
                if api::retired() {
                    return Err("host_retired".into());
                }
                PhotocraftApp::setup_context(
                    &cc.egui_ctx,
                    photocraft_ui_egui::theme::ThemeKind::default(),
                );
                let mut app = PhotocraftApp::new(Session::new(), Services::default());
                app.background_jobs = false;
                api::install(cc.egui_ctx.clone());
                Ok(Box::new(Shell {
                    app,
                    bridge: BrowserBridge::default(),
                    wiped: false,
                }))
            }),
        )
        .await
        .map_err(|_| JsValue::from_str("runtime_start_failed"))
}
struct Shell {
    app: PhotocraftApp,
    bridge: BrowserBridge,
    wiped: bool,
}
impl eframe::App for Shell {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if api::retired() {
            if !self.wiped {
                self.app = PhotocraftApp::new(Session::new(), Services::default());
                self.bridge = BrowserBridge::default();
                self.wiped = true;
            }
            return;
        }
        api::drain(&mut self.bridge, &mut self.app);
        if let Some(theme) = api::layout().and_then(|l| l.theme) {
            let theme = match theme {
                Theme::Dark => photocraft_ui_egui::theme::ThemeKind::Pro,
                Theme::Light => photocraft_ui_egui::theme::ThemeKind::ProLight,
            };
            if self.app.ui.theme != theme {
                self.app.set_theme(ctx, theme);
            }
        }
        if self.bridge.is_open() {
            self.app.logic(ctx, frame);
        }
    }
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        raw.dropped_files.clear();
        raw.hovered_files.clear();
        if let Some(layout) = api::layout() {
            raw.safe_area_insets = Some(layout.safe_insets());
        }
        if api::retired() || !self.bridge.is_open() || self.app.session.managed_frozen {
            raw.events.clear();
            return;
        }
        self.app.raw_input_hook(ctx, raw);
    }
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if api::retired() || !self.bridge.is_open() {
            return;
        }
        crate::viewport::show(ui, |ui| {
            ui.add_enabled_ui(!self.app.session.managed_frozen, |ui| {
                self.app.ui(ui, frame)
            });
        });
        ui.ctx().output_mut(|o| {
            o.commands
                .retain(|command| !matches!(command, egui::OutputCommand::OpenUrl(_)));
            if self.app.session.managed_frozen {
                o.ime = None;
                o.mutable_text_under_cursor = false;
            }
        });
        api::changed(&self.bridge, &self.app);
    }
}
