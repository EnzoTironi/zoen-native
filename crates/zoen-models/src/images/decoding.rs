use super::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::Cursor;

pub(super) fn image(
    bytes: Vec<u8>,
    raw: &Value,
    request: &ImageRequest,
) -> Result<GeneratedImage, OutputFailure> {
    let malformed = OutputFailure::MalformedResponse;
    let data = raw.get("data").and_then(Value::as_array).ok_or(malformed)?;
    if data.len() != 1
        || data[0].get("url").is_some_and(|v| !v.is_null())
        || data[0].get("b64_json").and_then(Value::as_str).is_none()
    {
        return Err(OutputFailure::UnsupportedOutput);
    }
    for (key, expected) in [
        ("output_format", json!("png")),
        ("size", json!(request.size)),
        ("quality", json!(request.quality)),
        ("background", json!(request.background)),
    ] {
        if raw.get(key).is_some_and(|actual| actual != &expected) {
            return Err(malformed);
        }
    }
    if bytes.len() as u64 > request.max_image_bytes {
        return Err(OutputFailure::ResponseTooLarge);
    }
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(malformed);
    }
    let mut decoder = png::Decoder::new(Cursor::new(&bytes));
    decoder.ignore_checksums(false);
    decoder.set_limits(png::Limits {
        bytes: 16 * 1024 * 1024,
    });
    let mut reader = decoder.read_info().map_err(|_| malformed)?;
    let (width, height) = request.size.dimensions();
    if reader.info().width != width
        || reader.info().height != height
        || reader.info().animation_control.is_some()
    {
        return Err(OutputFailure::UnsupportedOutput);
    }
    let size = reader
        .output_buffer_size()
        .ok_or(OutputFailure::ResponseTooLarge)?;
    if size > 16 * 1024 * 1024 {
        return Err(OutputFailure::ResponseTooLarge);
    }
    let mut pixels = vec![0; size];
    reader.next_frame(&mut pixels).map_err(|_| malformed)?;
    reader.finish().map_err(|_| malformed)?;
    drop(reader);
    Ok(GeneratedImage {
        sha256: hex::encode(Sha256::digest(&bytes)),
        bytes,
        mime: "image/png",
        width,
        height,
    })
}
fn counter(value: &Value, key: &str) -> Result<Option<u64>, ()> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_u64().map(Some).ok_or(()),
    }
}
fn detail(value: &Value, group: &str, key: &str) -> Result<Option<u64>, ()> {
    match value.get(group) {
        None | Some(Value::Null) => Ok(None),
        Some(v) if v.is_object() => counter(v, key),
        _ => Err(()),
    }
}
pub(super) fn usage(raw: Option<&Value>) -> (UsageEvidence, ImageUsageEvidence) {
    let Some(raw) = raw.filter(|v| !v.is_null()) else {
        return (UsageEvidence::Missing, ImageUsageEvidence::Missing);
    };
    let parsed = (|| {
        if !raw.is_object() {
            return Err(());
        }
        let input = counter(raw, "input_tokens")?;
        let output = counter(raw, "output_tokens")?;
        let total = counter(raw, "total_tokens")?;
        if let (Some(a), Some(b)) = (input, output) {
            let sum = a.checked_add(b).ok_or(())?;
            if total.is_some_and(|t| sum != t) {
                return Err(());
            }
        }
        let details = ImageTokenDetails {
            input_text_tokens: detail(raw, "input_tokens_details", "text_tokens")?,
            input_image_tokens: detail(raw, "input_tokens_details", "image_tokens")?,
            output_text_tokens: detail(raw, "output_tokens_details", "text_tokens")?,
            output_image_tokens: detail(raw, "output_tokens_details", "image_tokens")?,
        };
        for (a, b, parent) in [
            (details.input_text_tokens, details.input_image_tokens, input),
            (
                details.output_text_tokens,
                details.output_image_tokens,
                output,
            ),
        ] {
            if [a, b]
                .into_iter()
                .flatten()
                .any(|v| parent.is_some_and(|p| v > p))
            {
                return Err(());
            }
            if let (Some(a), Some(b), Some(p)) = (a, b, parent) {
                if a.checked_add(b) != Some(p) {
                    return Err(());
                }
            }
        }
        if total.is_some_and(|t| [input, output].into_iter().flatten().any(|v| v > t)) {
            return Err(());
        }
        Ok((
            UsageEvidence::Reported(ReportedUsage {
                input_tokens: input,
                output_tokens: output,
                total_tokens: total,
                cache_read_tokens: None,
                reasoning_tokens: None,
            }),
            ImageUsageEvidence::Reported(details),
        ))
    })();
    parsed.unwrap_or((UsageEvidence::Invalid, ImageUsageEvidence::Invalid))
}
