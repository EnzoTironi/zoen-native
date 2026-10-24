//! Export only the already authorized accepted package; never the visible draft.
use crate::{ExportFormat, Result, failure, profile};
use wordcraft_layout::{
    LayoutCache, LayoutOptions,
    display::{DisplayOptions, Draw, page_display},
};
// Keep every worker frame within the existing WasmTier 1-MiB stdout bound,
// including the four-byte prefix and maximum 64-KiB metadata. Browser uses
// this same conservative output contract instead of widening worker limits.
const MAX_EXPORT: usize = 1024 * 1024 - 64 * 1024 - 4;
pub(crate) fn accepted(
    bytes: &[u8],
    format: &ExportFormat,
) -> Result<(Vec<u8>, &'static str, &'static str)> {
    match format {
        ExportFormat::Docx => Ok((bytes.to_vec(), "docx", profile::MIME)),
        ExportFormat::Pdf => {
            let document = profile::parse(bytes)?;
            let options = wordcraft_pdf::PdfOptions::default();
            let layout = wordcraft_layout::layout(
                &document,
                &mut LayoutCache::new(),
                &LayoutOptions {
                    hide_deleted: !options.include_markup,
                    comment_balloons: options.comments,
                    ..Default::default()
                },
            );
            // Inspect the same real draw items/fonts the pinned PDF writer uses. A
            // .notdef glyph would silently replace content with a box/NUL in the PDF.
            let display = DisplayOptions {
                dim_header: false,
                markup: options.include_markup,
                ..Default::default()
            };
            if layout.pages.iter().any(|page| {
                page_display(&document, page, &display)
                    .iter()
                    .any(missing_glyph)
            }) {
                return Err(failure("pdf_missing_glyph"));
            }
            let bytes = wordcraft_pdf::export_layout(&document, &layout, &options)
                .map_err(|_| failure("pdf_export_failed"))?;
            if bytes.is_empty() || bytes.len() > MAX_EXPORT {
                return Err(failure("pdf_export_size"));
            }
            Ok((bytes, "pdf", "application/pdf"))
        }
    }
}
fn missing_glyph(draw: &Draw) -> bool {
    match draw {
        Draw::Glyphs { glyphs, .. } => glyphs.iter().any(|(id, _, _)| *id == 0),
        Draw::Figure { draws, .. } => draws.iter().any(missing_glyph),
        Draw::Turned { items, .. } | Draw::Rotated { items, .. } | Draw::Clip { items, .. } => {
            items.iter().any(missing_glyph)
        }
        _ => false,
    }
}
