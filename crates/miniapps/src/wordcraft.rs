//! Closed, lossless plain DOCX storage profile for the pinned upstream writer.
//! No engine/layout/fonts, editing, network or browser availability is provided here.
#![warn(clippy::pedantic, clippy::perf)]
use serde_json::json;
use std::{
    collections::BTreeMap,
    io::{Cursor, Read},
};
mod archive;
mod plain;
#[cfg(test)]
mod tests;
pub const APP: &str = "wordcraft";
/// Reserved identity for canonical documents; no browser resource is registered yet.
pub const RESOURCE: &str = "ui://zoen/wordcraft/index.html";
pub const MIME: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
pub const ENGINE_REVISION: &str = "d1b0bfc3f9dd83126b162a42b667facecb003b7f";
pub const MAX_BYTES: usize = 512 * 1024;
pub const MAX_TEXT: usize = 32 * 1024;
pub const EMPTY: &[u8] = include_bytes!("../resources/wordcraft-empty.docx");
const MAX_PART: usize = 1024 * 1024;
const MAX_TOTAL: usize = 4 * MAX_PART;
const INVALID: &str = "Invalid or unsupported managed plain DOCX document";
pub struct Document {
    pub text: String,
    pub paragraphs: usize,
    pub words: usize,
}
impl Document {
    /// Validate without normalizing or discarding any package part.
    /// # Errors
    /// Refuses unsupported markup/metadata, malformed archives or exceeded bounds.
    pub fn parse(bytes: &[u8]) -> Result<Self, &'static str> {
        let mut template = parts(EMPTY)?;
        let mut parts = parts(bytes)?;
        let document = parts.remove("word/document.xml").ok_or(INVALID)?;
        let baseline = template.remove("word/document.xml").ok_or(INVALID)?;
        let text = plain::text(&document, &baseline)?;
        let words = text
            .split(|c: char| c.is_whitespace() || c == '\u{fffc}')
            .filter(|word| word.chars().any(char::is_alphanumeric))
            .count();
        let paragraphs = text.split('\n').count();
        let app = parts.remove("docProps/app.xml").ok_or(INVALID)?;
        let baseline_app = template.remove("docProps/app.xml").ok_or(INVALID)?;
        let expected_app = std::str::from_utf8(&baseline_app)
            .map_err(|_| INVALID)?
            .replace("<Words>0</Words>", &format!("<Words>{words}</Words>"))
            .replace(
                "<Paragraphs>1</Paragraphs>",
                &format!("<Paragraphs>{paragraphs}</Paragraphs>"),
            );
        if app != expected_app.as_bytes() || parts != template {
            return Err(INVALID);
        }
        Ok(Self {
            text,
            paragraphs,
            words,
        })
    }
    #[must_use]
    pub fn metadata(&self, title: &str) -> String {
        // Content remains in the encrypted FileDoc, never duplicated in Item metadata.
        json!({"title":title,"paragraphs":self.paragraphs,"words":self.words,
            "profile":"plain-text-v1","engineRevision":ENGINE_REVISION})
        .to_string()
    }
}
#[must_use]
pub fn valid_text(text: &str) -> bool {
    text.len() <= MAX_TEXT
        && !text.chars().any(|c| {
            !matches!(c, '\t' | '\n' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}')
                || (c.is_control() && c != '\n' && c != '\t')
        })
}
fn parts(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, &'static str> {
    if bytes.is_empty() || bytes.len() > MAX_BYTES {
        return Err(INVALID);
    }
    let expected = archive::preflight(bytes, 32, MAX_PART, MAX_TOTAL)?;
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| INVALID)?;
    if zip.len() != expected {
        return Err(INVALID);
    }
    let mut parts = BTreeMap::new();
    let mut total = 0usize;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|_| INVALID)?;
        let name = entry.name().to_owned();
        if entry.is_dir()
            || entry.size() > MAX_PART as u64
            || parts.contains_key(&name)
            || name.is_empty()
            || name.contains(['\\', '\0'])
            || name
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
        {
            return Err(INVALID);
        }
        let declared = usize::try_from(entry.size()).map_err(|_| INVALID)?;
        if total.checked_add(declared).is_none_or(|n| n > MAX_TOTAL) {
            return Err(INVALID);
        }
        let mut data = Vec::new();
        entry
            .by_ref()
            .take((MAX_PART + 1) as u64)
            .read_to_end(&mut data)
            .map_err(|_| INVALID)?;
        total = total.checked_add(data.len()).ok_or(INVALID)?;
        if data.len() != declared || data.len() > MAX_PART || total > MAX_TOTAL {
            return Err(INVALID);
        }
        parts.insert(name, data);
    }
    Ok(parts)
}
