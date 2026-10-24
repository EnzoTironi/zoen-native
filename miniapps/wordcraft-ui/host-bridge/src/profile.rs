//! Shared lossless DOCX validation; the actual model is never rebuilt from plain text.
use crate::Result;
use wordcraft_doc::Document;
pub use zoen_word_profile::{ENGINE_REVISION, MAX_DOCUMENT, MAX_TEXT};
pub const MIME: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
/// # Errors
/// Refuses text outside the shared bounded XML contract.
pub fn validate_text(text: &str) -> Result<()> {
    zoen_word_profile::validate_text(text).map_err(str::to_owned)
}
/// Creation convenience only: actual writer bytes, not a rich-model checkpoint.
/// # Errors
/// Refuses unsupported text, writer failure or a non-reopenable package.
pub fn encode(text: &str) -> Result<Vec<u8>> {
    encode_document(&Document::from_text(text))
}
/// # Errors
/// Refuses any document whose full model cannot be retained by the pinned codec.
pub fn encode_document(document: &Document) -> Result<Vec<u8>> {
    zoen_word_profile::encode(document).map_err(str::to_owned)
}
/// # Errors
/// Refuses unknown/lossy parts, external relationships or exceeded limits.
pub fn parse(bytes: &[u8]) -> Result<Document> {
    zoen_word_profile::parse(bytes).map_err(str::to_owned)
}
