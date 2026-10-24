//! One bounded DOCX contract over the pinned upstream model and codec.
//! No editor, layout, fonts, file service, network or document authority lives here.
pub mod action;
pub mod archive;
mod package;
#[cfg(test)]
mod tests;
pub use wordcraft_doc::{self as model, Document, StoryRef};
pub const ENGINE_REVISION: &str = "d1b0bfc3f9dd83126b162a42b667facecb003b7f";
pub const MAX_DOCUMENT: usize = 512 * 1024;
pub const MAX_TEXT: usize = 32 * 1024;
const MAX_PART: usize = 1024 * 1024;
const MAX_INFLATED: usize = 4 * MAX_PART;
const MAX_PARTS: usize = 32;
pub(crate) const INVALID: &str = "Invalid or unsupported managed DOCX document";
pub type Result<T> = std::result::Result<T, &'static str>;
fn failure(_: &str) -> &'static str {
    INVALID
}
/// # Errors
/// Keeps the existing bounded XML text contract for human and agent edits.
pub fn validate_text(text: &str) -> Result<()> {
    if text.len() > MAX_TEXT
        || text.chars().any(|c| {
            !matches!(c, '\t' | '\n' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}')
                || (c.is_control() && c != '\n' && c != '\t')
        })
    {
        return Err("unsupported_text");
    }
    Ok(())
}
fn inert(parts: &std::collections::BTreeMap<String, Vec<u8>>) -> Result<()> {
    use quick_xml::events::Event;
    for (name, bytes) in parts {
        if !name.ends_with(".rels") {
            continue;
        }
        let mut xml = quick_xml::Reader::from_reader(bytes.as_slice());
        loop {
            match xml.read_event().map_err(|_| INVALID)? {
                Event::Start(e) | Event::Empty(e) => {
                    for attribute in e.attributes() {
                        let attribute = attribute.map_err(|_| INVALID)?;
                        if attribute.key.local_name().as_ref() == b"TargetMode"
                            && attribute.value.as_ref() == b"External"
                        {
                            return Err("external_relationship");
                        }
                    }
                }
                Event::DocType(_) => return Err("unsupported_xml_doctype"),
                Event::Eof => break,
                _ => {}
            }
        }
    }
    Ok(())
}
/// Preserve the actual upstream rich model only when every source part survives.
/// # Errors
/// Rejects oversized packages, external relationships, opaque passthrough and any
/// reader repair/unknown markup that the same pinned writer cannot reproduce.
pub fn parse(bytes: &[u8]) -> Result<Document> {
    let original = package::parts(bytes)?;
    inert(&original)?;
    let document = wordcraft_docx::read(bytes).map_err(|_| "docx_read")?;
    validate_text(&document.plain_text(StoryRef::Body))?;
    if !document.passthrough.is_empty() {
        return Err("unsupported_opaque_parts");
    }
    let rewritten = wordcraft_docx::write(&document).map_err(|_| "docx_write")?;
    if original != package::parts(&rewritten)? {
        return Err("unsupported_document_profile");
    }
    Ok(document)
}
/// Write the actual current model; never rebuild it from plain text.
/// # Errors
/// Refuses a model that cannot retain its full meaning through the codec.
pub fn encode(document: &Document) -> Result<Vec<u8>> {
    validate_text(&document.plain_text(StoryRef::Body))?;
    let bytes = wordcraft_docx::write(document).map_err(|_| "docx_write")?;
    if parse(&bytes)? != *document {
        return Err("lossy_docx_model");
    }
    Ok(bytes)
}
