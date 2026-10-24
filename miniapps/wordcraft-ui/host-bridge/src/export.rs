//! Export only the already authorized accepted package; never the visible draft.
use crate::{ExportFormat, Result, failure, profile};
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
            let bytes = wordcraft_pdf::export(&document, &wordcraft_pdf::PdfOptions::default())
                .map_err(|_| failure("pdf_export_failed"))?;
            if bytes.is_empty() || bytes.len() > MAX_EXPORT {
                return Err(failure("pdf_export_size"));
            }
            Ok((bytes, "pdf", "application/pdf"))
        }
    }
}
