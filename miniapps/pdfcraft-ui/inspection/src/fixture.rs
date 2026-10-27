//! Contributor-authored review PDF made by the actual upstream PDF writer.
use pdfcraft_cos::{Document, Object, PdfString, SaveOptions, write_full};
use pdfcraft_engine::{DocId, Session};
use std::sync::Arc;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub const UNKNOWN_KEY: &[u8] = b"ZoenConformancePayload";
pub const UNKNOWN_VALUE: &str = "Opaque publisher data must survive save, Undo and recovery.";

pub fn source() -> Result<Arc<Vec<u8>>> {
    let mut doc = pdfcraft_create::from_text(
        "Shared review",
        "Zoen product review\n\nHuman and agent edits share one PDF.\n\nReview the delivery plan and retain the source data.\n\nResponsabilidade, revisao e persistencia.\u{c}Second page\n\nApproved changes should survive a cold restart.\n\nExport the chosen version, including when newer edits are pending.",
        (595.0, 842.0),
        16.0,
    )?;
    let root = doc.trailer().reference(b"Root").ok_or("missing_catalog")?;
    doc.update_dict(root, |dict| {
        dict.set(UNKNOWN_KEY, Object::String(PdfString::text(UNKNOWN_VALUE)));
    })?;
    Ok(Arc::new(write_full(&doc, &SaveOptions::default())?))
}

pub fn unknown_data(bytes: Arc<Vec<u8>>) -> Result<()> {
    let cos = Document::open(bytes)?;
    let root = cos.trailer().get(b"Root").ok_or("missing_catalog")?;
    let catalog = cos.resolve(root);
    let value = catalog
        .as_dict()
        .and_then(|dict| dict.get(UNKNOWN_KEY))
        .and_then(Object::as_string)
        .ok_or("opaque_catalog_value_missing")?;
    if value.to_text() != UNKNOWN_VALUE {
        return Err("opaque_catalog_value_changed".into());
    }
    Ok(())
}

pub fn open(bytes: Arc<Vec<u8>>) -> Result<(Session, DocId)> {
    // Fixed synthetic date; production uses the host's real wall clock.
    let mut session = Session::new().with_clock(|| 1_792_843_200);
    session.set_javascript(false);
    let id = session.open("Shared review.pdf", None, bytes, None)?;
    if session.javascript() {
        return Err("document_javascript_enabled".into());
    }
    Ok((session, id))
}
