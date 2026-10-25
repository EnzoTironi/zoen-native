//! Structural admission and byte preservation for the managed PDF App.
//! The pinned engine validates full page semantics, active data and edit fidelity.
//! Core never repairs, decodes images, rasterizes, edits or rewrites the document.
#![warn(clippy::pedantic, clippy::perf)]
pub mod action;
pub mod agent_tools;
pub mod structure;
#[cfg(test)]
mod tests;
use serde_json::json;
pub const APP: &str = "pdfcraft";
pub const RESOURCE: &str = "ui://zoen/pdfcraft/index.html";
pub const MIME: &str = "application/pdf";
pub const ENGINE_REVISION: &str = "d445410807e7eefbe5b96f9662ab963ae8dd2a3a";
pub const MAX_BYTES: usize = structure::MAX_BYTES;
pub const EMPTY: &[u8] = include_bytes!("../resources/pdfcraft-empty.pdf");
pub struct Document {
    /// Catalog page-tree declaration; the engine verifies the complete page tree.
    pub declared_pages: usize,
}
impl Document {
    /// # Errors
    /// Refuses structural ambiguity or exceeded bounds without normalizing bytes.
    pub fn parse(bytes: &[u8]) -> Result<Self, &'static str> {
        let source = structure::parse(bytes)?;
        Ok(Self {
            declared_pages: source.declared_pages,
        })
    }
    #[must_use]
    pub fn metadata(&self, title: &str) -> String {
        // Content remains solely in the encrypted FileDoc.
        json!({"title":title,"declaredPages":self.declared_pages,
            "profile":"managed-pdf-v1","engineRevision":ENGINE_REVISION})
        .to_string()
    }
}
