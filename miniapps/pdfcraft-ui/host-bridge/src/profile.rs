//! Full engine admission, in addition to the shared lightweight Core structure.
//! This owns no canonical state and never reconstructs a PDF into another model.
use crate::{Result, failure, structure};
use pdfcraft_cos::{Document, Object};
use pdfcraft_engine::{DocId, Session};
use std::{io::Cursor, sync::Arc};

pub const ENGINE_REVISION: &str = "d445410807e7eefbe5b96f9662ab963ae8dd2a3a";
const MAX_DECODED_STREAM: usize = 8 << 20;
const MAX_DECODED_TOTAL: usize = 32 << 20;

fn jpeg(stream: &pdfcraft_cos::Stream) -> bool {
    match stream.dict.get(b"Filter") {
        Some(Object::Name(name)) => name == b"DCTDecode",
        Some(Object::Array(filters)) if filters.len() == 1 => {
            filters[0].as_name() == Some(b"DCTDecode")
        }
        _ => false,
    }
}

fn streams(bytes: Arc<Vec<u8>>) -> Result<()> {
    let document =
        Document::open_with_stream_limit(bytes, None, structure::INSPECTION_STREAM_BYTES)
            .map_err(|_| failure("invalid_pdf"))?;
    let mut decoded_total = 0_usize;
    for (_, object) in document.scan_objects_checked() {
        let object = object.map_err(|_| failure("incomplete_pdf"))?;
        if let Object::Stream(stream) = object.as_ref() {
            if jpeg(stream) {
                if stream.dict.name(b"Subtype") != Some(b"Image") {
                    return Err(failure("unsupported_stream"));
                }
                // Reuse the real image codec to read actual dimensions, without
                // allocating an image. A false PDF Width/Height cannot hide a bomb.
                let dimensions = image::ImageReader::with_format(
                    Cursor::new(&stream.raw[..]),
                    image::ImageFormat::Jpeg,
                )
                .into_dimensions()
                .map_err(|_| failure("invalid_jpeg"))?;
                if stream.dict.int(b"Width") != Some(i64::from(dimensions.0))
                    || stream.dict.int(b"Height") != Some(i64::from(dimensions.1))
                {
                    return Err(failure("image_dimensions_changed"));
                }
            } else {
                let remaining = MAX_DECODED_TOTAL
                    .saturating_sub(decoded_total)
                    .min(MAX_DECODED_STREAM);
                let data = stream
                    .decoded_within(remaining)
                    .map_err(|_| failure("unsupported_or_oversized_stream"))?;
                decoded_total = decoded_total
                    .checked_add(data.len())
                    .ok_or_else(|| failure("decoded_limit"))?;
            }
        }
    }
    Ok(())
}

/// # Errors
/// Refuses an incompatible or lossy source, bounded decode violations, scripts,
/// XFA, embedded files, signatures or repairs. Original bytes remain untouched.
pub fn open(bytes: &[u8]) -> Result<(Session, DocId)> {
    let structure = structure::parse(bytes).map_err(failure)?;
    let bytes = Arc::new(bytes.to_vec());
    streams(bytes.clone())?;
    let mut session = Session::new();
    session.set_javascript(false);
    let id = session
        .open("Document.pdf", None, bytes.clone(), None)
        .map_err(|_| failure("engine_open_refused"))?;
    let document = session.get(id).ok_or_else(|| failure("not_open"))?;
    if document.info.pages.len() != structure.declared_pages
        || document.info.encrypted
        || document.read_only_reason.is_some()
        || !document.info.warnings.is_empty()
        || document.info.has_javascript
        || document.info.xfa.is_some()
        || !document.info.attachments.is_empty()
        || !document.signatures.is_empty()
        || document.info.pages.iter().any(|page| {
            !page.width.is_finite()
                || !page.height.is_finite()
                || page.width <= 0.0
                || page.height <= 0.0
                || page.width > 2048.0
                || page.height > 2048.0
        })
    {
        return Err(failure("unsupported_pdf_profile"));
    }
    if session
        .save_bytes(id)
        .map_err(|_| failure("engine_save_refused"))?
        != bytes
    {
        return Err(failure("source_would_be_normalized"));
    }
    Ok((session, id))
}

/// # Errors
/// Reopens the actual engine output through the same lossless admission path.
pub fn checked(bytes: &[u8]) -> Result<()> {
    open(bytes).map(|_| ())
}

/// # Errors
/// The genuine upstream writer creates every result. Periodic full saves bound
/// the internal PDF revision chain; Core still retains each exact chosen version.
pub fn encode(session: &Session, id: DocId) -> Result<Vec<u8>> {
    let document = session.get(id).ok_or_else(|| failure("not_open"))?;
    if !document.edit_notices.is_empty() {
        return Err(failure("representation_loss"));
    }
    let cos = Document::open_with_stream_limit(
        document.bytes.clone(),
        None,
        structure::INSPECTION_STREAM_BYTES,
    )
    .map_err(|_| failure("incomplete_pdf"))?;
    let bytes = if cos.revisions().len() >= 32 {
        session.save_full_bytes(id)
    } else {
        session.save_bytes(id)
    }
    .map_err(|_| failure("engine_save_refused"))?;
    session
        .check_save(id, &bytes)
        .map_err(|_| failure("engine_output_refused"))?;
    Ok(bytes.as_ref().clone())
}
