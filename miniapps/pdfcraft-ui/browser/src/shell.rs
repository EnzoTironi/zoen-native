use crate::{api, layout::Theme};
use pdfcraft_ui_egui::{PdfCraftApp, theme::ThemePreference};
use pdfcraft_zoen_host::browser::BrowserBridge;
use wasm_bindgen::{JsCast as _, JsValue};

pub async fn start() -> Result<(), JsValue> {
    let canvas = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id("pdfcraft_canvas"))
        .and_then(|element| element.dyn_into::<web_sys::HtmlCanvasElement>().ok())
        .ok_or_else(|| JsValue::from_str("missing_canvas"))?;
    eframe::WebRunner::new()
        .start(
            canvas,
            eframe::WebOptions {
                renderer: eframe::Renderer::Glow,
                ..Default::default()
            },
            Box::new(|creation| {
                if api::retired() {
                    return Err("host_retired".into());
                }
                // Do not restore old guest layout/cursor state into another owner.
                creation
                    .egui_ctx
                    .memory_mut(|memory| *memory = Default::default());
                let mut app = PdfCraftApp::new();
                app.session.set_javascript(false);
                app.managed_export = Some(api::export_intent);
                api::install(creation.egui_ctx.clone());
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
    app: PdfCraftApp,
    bridge: BrowserBridge,
    wiped: bool,
}
impl eframe::App for Shell {
    fn persist_egui_memory(&self) -> bool {
        false
    }

    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if api::retired() {
            if !self.wiped {
                self.app = PdfCraftApp::new();
                self.bridge = BrowserBridge::default();
                ctx.memory_mut(|memory| *memory = Default::default());
                self.wiped = true;
            }
            return;
        }
        api::drain(&mut self.bridge, &mut self.app, api::Phase::BeforeInput);
        if let Some(theme) = api::layout().and_then(|layout| layout.theme) {
            let preference = match theme {
                Theme::Dark => ThemePreference::Dark,
                Theme::Light => ThemePreference::Light,
            };
            if self.app.theme_preference != preference {
                self.app.set_theme_preference(preference);
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
        api::drain(&mut self.bridge, &mut self.app, api::Phase::AfterInput);
        ui.ctx().output_mut(|output| {
            output
                .commands
                .retain(|command| !matches!(command, egui::OutputCommand::OpenUrl(_)));
            if self.app.session.managed_frozen {
                output.ime = None;
                output.mutable_text_under_cursor = false;
            }
        });
        api::changed(&self.bridge, &self.app);
    }
}
