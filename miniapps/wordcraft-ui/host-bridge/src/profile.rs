//! Closed plain-text DOCX profile. Unknown or lossy imports are refused, never normalized.
use crate::{Result, failure};
use std::collections::BTreeMap;
use std::io::{Cursor, Read};
use wordcraft_doc::{Document, StoryRef};

pub const ENGINE_REVISION: &str = "d1b0bfc3f9dd83126b162a42b667facecb003b7f";
pub const MIME: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
pub const MAX_DOCUMENT: usize = 512 * 1024;
const MAX_PART: usize = 1024 * 1024;
const MAX_INFLATED: usize = 4 * MAX_PART;
const MAX_PARTS: usize = 32;
pub const MAX_TEXT: usize = 32 * 1024;

pub fn validate_text(text: &str) -> Result<()> {
    if text.len() > MAX_TEXT
        || text
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err(failure("unsupported_text"));
    }
    Ok(())
}

/// Inspect sizes/counts BEFORE decompression and bound actual reads too. No extraction paths.
fn parts(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>> {
    if bytes.is_empty() || bytes.len() > MAX_DOCUMENT {
        return Err(failure("document_size"));
    }
    let mut archive =
        zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| failure("invalid_zip"))?;
    if archive.len() > MAX_PARTS {
        return Err(failure("part_count"));
    }
    let mut declared = 0u64;
    for i in 0..archive.len() {
        let entry = archive.by_index(i).map_err(|_| failure("invalid_zip"))?;
        declared = declared
            .checked_add(entry.size())
            .ok_or_else(|| failure("inflated_size"))?;
        if entry.size() > MAX_PART as u64 || declared > MAX_INFLATED as u64 {
            return Err(failure("inflated_size"));
        }
    }
    let mut result = BTreeMap::new();
    let mut actual = 0usize;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|_| failure("invalid_zip"))?;
        let name = entry.name().to_owned();
        if entry.is_dir()
            || name.is_empty()
            || name.contains(['\\', '\0'])
            || name
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
            || result.contains_key(&name)
        {
            return Err(failure("invalid_part"));
        }
        let declared = entry.size();
        let mut data = Vec::new();
        entry
            .by_ref()
            .take((MAX_PART + 1) as u64)
            .read_to_end(&mut data)
            .map_err(|_| failure("invalid_zip"))?;
        actual = actual
            .checked_add(data.len())
            .ok_or_else(|| failure("inflated_size"))?;
        if data.len() as u64 != declared || data.len() > MAX_PART || actual > MAX_INFLATED {
            return Err(failure("inflated_size"));
        }
        result.insert(name, data);
    }
    Ok(result)
}

pub fn encode(text: &str) -> Result<Vec<u8>> {
    validate_text(text)?;
    let bytes =
        wordcraft_docx::write(&Document::from_text(text)).map_err(|_| failure("docx_write"))?;
    parts(&bytes)?;
    Ok(bytes)
}

pub fn parse(bytes: &[u8]) -> Result<Document> {
    // The upstream reader is lenient and preserves macros/unknown parts. Bound the package
    // first, then compare EVERY uncompressed part with the actual canonical plain writer.
    // This intentionally refuses even benign formatting/metadata not supported by this slice.
    let original = parts(bytes)?;
    let doc = wordcraft_docx::read(bytes).map_err(|_| failure("docx_read"))?;
    let text = doc.plain_text(StoryRef::Body);
    validate_text(&text)?;
    if original != parts(&encode(&text)?)? {
        return Err(failure("unsupported_document_profile"));
    }
    Ok(Document::from_text(&text))
}
