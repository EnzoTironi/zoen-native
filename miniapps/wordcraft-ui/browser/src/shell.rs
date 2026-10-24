use crate::{api, layout::Theme};
use wasm_bindgen::{JsCast as _, JsValue};
use wordcraft_doc::Document;
use wordcraft_engine::Session;
use wordcraft_ui_egui::WordApp;
use wordcraft_zoen_host::browser::BrowserBridge;

pub async fn start() -> Result<(), JsValue> {
    let canvas = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("wordcraft_canvas"))
        .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok())
        .ok_or_else(|| JsValue::from_str("missing_canvas"))?;
    let options = eframe::WebOptions {
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::WebRunner::new()
        .start(
            canvas,
            options,
            Box::new(|cc| {
                if api::retired() {
                    return Err("host_retired".into());
                }
                // No standalone preferences, inbox, file/print/link/font services, storage, recovery
                // path or collaboration is installed. These are the actual upstream canvas and keys.
                let mut app = WordApp::new(Session::new(Document::new()), Default::default());
                app.autosave = false;
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
    app: WordApp,
    bridge: BrowserBridge,
    wiped: bool,
}
impl eframe::App for Shell {
    fn logic(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        if api::retired() {
            if !self.wiped {
                self.app = WordApp::new(Session::new(Document::new()), Default::default());
                self.bridge = BrowserBridge::default();
                self.wiped = true;
            }
            return;
        }
        api::drain(&mut self.bridge, &mut self.app);
        if let Some(theme) = api::layout().and_then(|l| l.theme) {
            self.app.ui.theme = match theme {
                Theme::Dark => wordcraft_ui_egui::theme::Appearance::Dark,
                Theme::Light => wordcraft_ui_egui::theme::Appearance::Light,
            };
        }
        if self.bridge.is_open() {
            self.app.logic(ctx);
        }
    }
    fn raw_input_hook(&mut self, _: &egui::Context, raw: &mut egui::RawInput) {
        raw.dropped_files.clear();
        raw.hovered_files.clear();
        if let Some(layout) = api::layout() {
            raw.safe_area_insets = Some(layout.safe_insets());
        }
        if api::retired() || !self.bridge.is_open() {
            raw.events.clear();
            return;
        }
        self.app.raw_input_hook(raw);
    }
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        if api::retired() || !self.bridge.is_open() {
            return;
        }
        self.app.ui(ui);
        if self.app.session.managed_frozen {
            // History, explicit host pause and a pending agent proposal cannot accept typing.
            ui.ctx().output_mut(|o| {
                o.ime = None;
                o.mutable_text_under_cursor = false;
            });
        }
        api::changed(&self.bridge, &self.app);
    }
}
