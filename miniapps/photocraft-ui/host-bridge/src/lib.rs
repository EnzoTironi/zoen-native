//! Byte-only prototype: the existing Zoen host remains the authority for access,
//! selected version/hash, approval, CAS and persistence. No filesystem or UI API.
#![forbid(unsafe_code)]

mod archive;
#[cfg(feature = "browser")]
pub mod browser;
pub mod managed;

use photocraft_doc::{ColorMode, Document, LayerContent, SampleType};
use photocraft_engine::{EngineError, Session};
use photocraft_format::{LoadOptions, SaveOptions};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MAX_DOCUMENT_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_DECODED_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_CANVAS_SIDE: u32 = 1024;
pub const MAX_CANVAS_PIXELS: u64 = 1024 * 1024;
pub const MAX_MANIFEST_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Limit,
    InvalidDocument,
    UnsupportedProfile,
    SourceMismatch,
    Engine,
    Export,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Photo document operation refused: {self:?}")
    }
}
impl std::error::Error for Error {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    FlipHorizontal,
    FlipVertical,
    RotateClockwise,
    RotateCounterclockwise,
    Rotate180,
}
impl Operation {
    fn command(self) -> &'static str {
        match self {
            Self::FlipHorizontal => "image.imageRotation.flipCanvasHorizontal",
            Self::FlipVertical => "image.imageRotation.flipCanvasVertical",
            Self::RotateClockwise => "image.imageRotation.90cw",
            Self::RotateCounterclockwise => "image.imageRotation.90ccw",
            Self::Rotate180 => "image.imageRotation.180",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    pub width: u32,
    pub height: u32,
    pub layers: usize,
    pub engine_revision: &'static str,
}

fn format_error(error: &photocraft_format::FormatError) -> Error {
    match error {
        photocraft_format::FormatError::LimitExceeded(_) => Error::Limit,
        photocraft_format::FormatError::TooNew { .. }
        | photocraft_format::FormatError::Unsupported(_) => Error::UnsupportedProfile,
        _ => Error::InvalidDocument,
    }
}

fn profile(doc: &Document) -> Result<(), Error> {
    let layer = doc.layers.first().ok_or(Error::UnsupportedProfile)?;
    if doc.mode != ColorMode::Rgb
        || doc.depth != SampleType::U8
        || doc.layers.len() != 1
        || !matches!(layer.content, LayerContent::Raster(_))
        || layer.video.is_some()
        || doc.timeline.is_some()
        || !doc.patterns.is_empty()
        || !layer.effects.items.is_empty()
    {
        return Err(Error::UnsupportedProfile);
    }
    Ok(())
}

fn open(bytes: &[u8]) -> Result<Document, Error> {
    let original = archive::check(bytes)?;
    let doc = photocraft_format::load_from_bytes_with(
        bytes,
        &LoadOptions {
            max_manifest_bytes: MAX_MANIFEST_BYTES,
            max_blob_bytes: usize::try_from(MAX_DECODED_BYTES).map_err(|_| Error::Limit)?,
            max_total_bytes: MAX_DECODED_BYTES,
            preserve_ids: true,
            max_canvas_side: MAX_CANVAS_SIDE,
            max_canvas_pixels: MAX_CANVAS_PIXELS,
        },
    )
    .map_err(|error| format_error(&error))?;
    profile(&doc)?;
    // The pinned reader may normalize old/invalid state. Do not silently adopt
    // a repair or drop fields/entries from the selected immutable source.
    let saved = photocraft_format::save_to_bytes(&doc, &SaveOptions::default())
        .map_err(|error| format_error(&error))?;
    let reopened = archive::check(&saved)?;
    if original.document != reopened.document || original.entries != reopened.entries {
        return Err(Error::UnsupportedProfile);
    }
    Ok(doc)
}

fn bound_source(bytes: &[u8], expected: &[u8; 32]) -> Result<(), Error> {
    if bytes.len() > MAX_DOCUMENT_BYTES {
        return Err(Error::Limit);
    }
    let actual: [u8; 32] = Sha256::digest(bytes).into();
    if &actual != expected {
        return Err(Error::SourceMismatch);
    }
    Ok(())
}

/// Hash equality binds bytes, not authority: callers must obtain the expected
/// digest from the existing authorized selected-version projection.
///
/// # Errors
/// Refuses a different source, malformed/oversized document or unsupported profile.
pub fn inspect(bytes: &[u8], expected: &[u8; 32]) -> Result<Metadata, Error> {
    bound_source(bytes, expected)?;
    let doc = open(bytes)?;
    Ok(Metadata {
        width: doc.size.width,
        height: doc.size.height,
        layers: doc.layers.len(),
        engine_revision: "9b9643764f3fb77d9f0e48d7510983ae50621e11",
    })
}

fn authorize(command: &str, params: &serde_json::Value) -> photocraft_engine::Result<()> {
    if [
        Operation::FlipHorizontal,
        Operation::FlipVertical,
        Operation::RotateClockwise,
        Operation::RotateCounterclockwise,
        Operation::Rotate180,
    ]
    .iter()
    .any(|op| op.command() == command)
        && params.as_object().is_some_and(serde_json::Map::is_empty)
    {
        Ok(())
    } else {
        Err(EngineError::Other(
            "command outside managed photo profile".into(),
        ))
    }
}

/// Executes the original upstream command in a fresh, in-memory Session. The
/// result is a proposal only; only the normal host may approve/persist it.
///
/// # Errors
/// Refuses stale bytes, unsupported data, command failure or a non-reopenable result.
pub fn transform(bytes: &[u8], expected: &[u8; 32], op: Operation) -> Result<Vec<u8>, Error> {
    bound_source(bytes, expected)?;
    let mut session = Session::new();
    session.authorize = Some(authorize);
    session.add_document(open(bytes)?, None);
    session
        .execute(op.command(), serde_json::json!({}))
        .map_err(|_| Error::Engine)?;
    let current = session.active().ok_or(Error::Engine)?;
    let result = photocraft_format::save_to_bytes(&current.doc, &SaveOptions::default())
        .map_err(|_| Error::Engine)?;
    open(&result)?;
    Ok(result)
}

/// Explicit flattened PNG export of these exact accepted bytes. Native .pcraft
/// retains editing state; PNG is a separate output, never the canonical save.
///
/// # Errors
/// Refuses stale bytes, unsupported data or an oversized/failed export.
pub fn export_png(bytes: &[u8], expected: &[u8; 32]) -> Result<Vec<u8>, Error> {
    bound_source(bytes, expected)?;
    let doc = open(bytes)?;
    let options = photocraft_io::ExportOptions {
        xmp: photocraft_io::XmpEmbed::None,
        ..Default::default()
    };
    let exported = photocraft_io::export(&doc, "png", &options).map_err(|_| Error::Export)?;
    if exported.bytes.len() > MAX_DOCUMENT_BYTES {
        return Err(Error::Limit);
    }
    Ok(exported.bytes)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod managed_tests;
