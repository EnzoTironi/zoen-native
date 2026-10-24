use pdfcraft_engine::DocId;
use pdfcraft_ui_egui::PdfCraftApp;
use pdfcraft_zoen_host::{Accept, ActorKind, Head, Mutation, Open, Scope, browser::BrowserBridge};
use sha2::{Digest, Sha256};
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub fn source() -> Result<Vec<u8>> {
    let mut doc = pdfcraft_create::from_text(
        "Review",
        "Original paragraph.\n\nSecond paragraph.",
        (595.0, 842.0),
        16.0,
    )?;
    let root = doc.root().ok_or("missing_root")?;
    doc.update_dict(root, |dict| {
        dict.set(
            b"ZoenOpaque",
            pdfcraft_cos::Object::String(pdfcraft_cos::PdfString::text(
                "Retain original publisher data",
            )),
        )
    })?;
    Ok(pdfcraft_cos::write_full(
        &doc,
        &pdfcraft_cos::SaveOptions::default(),
    )?)
}
pub fn scope() -> Scope {
    Scope {
        artifact_id: "shared-pdf".into(),
        actor_id: "reviewer".into(),
        actor_kind: ActorKind::Human,
    }
}
pub fn head(version: &str, bytes: &[u8]) -> Head {
    Head {
        version: version.into(),
        sha256: format!("{:x}", Sha256::digest(bytes)),
    }
}
pub fn mutation(base: &Head, id: &str) -> Mutation {
    Mutation {
        scope: scope(),
        base: base.clone(),
        mutation_id: id.into(),
    }
}
pub fn accepted(id: &str, version: &str, bytes: &[u8]) -> Accept {
    Accept {
        scope: scope(),
        mutation_id: id.into(),
        head: head(version, bytes),
    }
}
pub struct Editor {
    pub app: PdfCraftApp,
    pub bridge: BrowserBridge,
}
impl Editor {
    pub fn open(bytes: &[u8]) -> Result<Self> {
        let mut app = PdfCraftApp::new();
        app.session.set_javascript(false);
        let mut bridge = BrowserBridge::default();
        bridge.open(
            &mut app,
            Open {
                scope: scope(),
                head: head("1", bytes),
            },
            bytes,
        )?;
        Ok(Self { app, bridge })
    }
    pub fn id(&self) -> Result<DocId> {
        self.app
            .active_ids()
            .map(|(_, id)| id)
            .ok_or_else(|| "missing_document".into())
    }
}
impl eframe::App for Editor {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.app.logic(ctx, frame);
    }
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        crate::viewport::show(ui, |ui| self.app.ui(ui, frame));
    }
}
