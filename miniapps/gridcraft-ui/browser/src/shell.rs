use crate::{api, layout::Theme};
use gridcraft_engine::Session;
use gridcraft_ui_egui::SheetApp;
use gridcraft_zoen_host::HostBridge;
use wasm_bindgen::{JsCast as _, JsValue};

pub async fn start() -> Result<(), JsValue> {
    let document = web_sys::window()
        .and_then(|w| w.document())
        .ok_or_else(|| JsValue::from_str("missing_document"))?;
    let canvas = document
        .get_element_by_id("gridcraft_canvas")
        .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok())
        .ok_or_else(|| JsValue::from_str("missing_canvas"))?;
    // Glow/WebGL rather than the desktop WebGPU defaults; the browser sandbox
    // owns canvas/clipboard capabilities and allows no engine network requests.
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
                let mut app = SheetApp::new(Session::new(), Default::default());
                app.chrome.title_bar = false;
                app.chrome.file_tab = false;
                // Never restore standalone localStorage/eframe state or install file,
                // link, print, inbox or collaboration services from the upstream shell.
                app.ui.autosave = false;
                app.ui.system_theme = false;
                let language = gridcraft_ui_egui::i18n::Language::system();
                if matches!(
                    language,
                    gridcraft_ui_egui::i18n::Language::En | gridcraft_ui_egui::i18n::Language::PtBr
                ) {
                    app.ui.language = language;
                }
                SheetApp::setup_context_for_language(&cc.egui_ctx, false, app.ui.language);
                api::install(cc.egui_ctx.clone());
                Ok(Box::new(Shell {
                    app,
                    bridge: HostBridge::default(),
                    wiped: false,
                }))
            }),
        )
        .await
        .map_err(|e| JsValue::from_str(&format!("runtime_start_failed: {e:?}")))
}
struct Shell {
    app: SheetApp,
    bridge: HostBridge,
    wiped: bool,
}
impl eframe::App for Shell {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if api::retired() {
            if !self.wiped {
                self.app = SheetApp::new(Session::new(), Default::default());
                self.bridge = HostBridge::default();
                self.wiped = true;
            }
            return;
        }
        api::drain(&mut self.bridge, &mut self.app);
        if let Some(l) = api::layout()
            && let Some(theme) = l.theme
        {
            self.app.ui.dark = matches!(theme, Theme::Dark);
        }
        self.app.logic(ctx);
    }
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        // Dragged files cannot bypass the Core open contract. File pickers and
        // downloads are absent; ordinary paste still follows the command policy.
        raw.dropped_files.clear();
        raw.hovered_files.clear();
        if let Some(l) = api::layout() {
            raw.safe_area_insets = Some(l.safe_insets());
        }
        if api::retired() {
            raw.events.clear();
            return;
        }
        self.app.raw_input_hook(raw);
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if api::retired() {
            return;
        }
        self.app.ui(ui);
        if self.app.session.active().is_some() {
            egui::Area::new("zoen-grid-export".into())
                .anchor(egui::Align2::RIGHT_BOTTOM, [-8.0, -8.0])
                .show(ui.ctx(), |ui| {
                    let label = if self.app.ui.language == gridcraft_ui_egui::i18n::Language::PtBr {
                        "Compartilhar"
                    } else {
                        "Share"
                    };
                    if ui.button(label).clicked() {
                        let _ = api::export_intent();
                    }
                });
        }
        api::changed(&self.bridge, &self.app);
    }
}
