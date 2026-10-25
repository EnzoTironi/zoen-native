//! Shared structural admission. The real engine owns complete PDF semantics and
//! fidelity; this parser never writes, repairs, rasterizes or normalizes bytes.
use pdfcraft_cos::{Document, Object};
use std::sync::Arc;

pub const MAX_BYTES: usize = 8 << 20;
pub const MAX_PAGES: usize = 32;
pub const MAX_IMAGE_PIXELS: u64 = 2_000_000;
pub const MAX_IMAGE_SIDE: i64 = 2048;
pub const INSPECTION_STREAM_BYTES: usize = 1 << 20;
const MAX_OBJECTS: usize = 8192;
const MAX_REVISIONS: usize = 64;
const INVALID: &str = "unsupported_pdf_structure";

pub struct Structure {
    pub declared_pages: usize,
}

/// # Errors
/// Refuses oversized, encrypted, repaired, ambiguous or structurally incomplete
/// sources. The page-tree Count is declared metadata, not a full semantic walk.
/// Raster fidelity, actual page count, active data and image encodings are checked
/// again by the pinned engine bridge before opening or proposing a new version.
pub fn parse(bytes: &[u8]) -> Result<Structure, &'static str> {
    if bytes.is_empty() || bytes.len() > MAX_BYTES || !bytes.starts_with(b"%PDF-") {
        return Err(INVALID);
    }
    let end = bytes
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())
        .ok_or(INVALID)?
        + 1;
    if !bytes
        .get(..end)
        .is_some_and(|tail| tail.ends_with(b"%%EOF"))
    {
        return Err(INVALID);
    }
    let document =
        Document::open_with_stream_limit(Arc::new(bytes.to_vec()), None, INSPECTION_STREAM_BYTES)
            .map_err(|_| INVALID)?;
    if document.security().is_some()
        || document.header_offset() != 0
        || document.reconstructed()
        || !document.repair_log().is_empty()
        || !document.xref_oddities().is_empty()
        || document.revisions().is_empty()
        || document.revisions().len() > MAX_REVISIONS
    {
        return Err(INVALID);
    }
    let numbers = document.object_numbers();
    if numbers.is_empty()
        || numbers.len() > MAX_OBJECTS
        || numbers
            .iter()
            .any(|number| !document.xref_entry_is_exact(*number))
    {
        return Err(INVALID);
    }
    let root = document.root().ok_or(INVALID)?;
    let catalog = document.get(root);
    let catalog = catalog.as_dict().ok_or(INVALID)?;
    if catalog.name(b"Type") != Some(b"Catalog") {
        return Err(INVALID);
    }
    let pages = document.resolve(catalog.get(b"Pages").ok_or(INVALID)?);
    let pages = pages.as_dict().ok_or(INVALID)?;
    let declared_pages =
        usize::try_from(pages.int(b"Count").ok_or(INVALID)?).map_err(|_| INVALID)?;
    if pages.name(b"Type") != Some(b"Pages") || !(1..=MAX_PAGES).contains(&declared_pages) {
        return Err(INVALID);
    }
    // Use upstream's checked streaming scan, whose parse caches are private and
    // evicted in batches. Missing object data must never turn into a valid null.
    for (_, object) in document.scan_objects_checked() {
        let object = object.map_err(|_| INVALID)?;
        if let Object::Stream(stream) = object.as_ref() {
            if stream.raw.len() > MAX_BYTES {
                return Err(INVALID);
            }
            if stream.dict.name(b"Subtype") == Some(b"Image") {
                let width = stream.dict.int(b"Width").ok_or(INVALID)?;
                let height = stream.dict.int(b"Height").ok_or(INVALID)?;
                if !(1..=MAX_IMAGE_SIDE).contains(&width)
                    || !(1..=MAX_IMAGE_SIDE).contains(&height)
                    || width
                        .checked_mul(height)
                        .and_then(|value| u64::try_from(value).ok())
                        .is_none_or(|pixels| pixels > MAX_IMAGE_PIXELS)
                {
                    return Err(INVALID);
                }
            }
        }
    }
    Ok(Structure { declared_pages })
}
