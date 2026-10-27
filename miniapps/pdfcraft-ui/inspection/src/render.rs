use crate::fixture::Result;
use pdfcraft_render::{PageRenderer, RenderConfig, RenderRequest};
use serde_json::{Value, json};
use std::{fs::File, io::BufWriter, path::Path, sync::Arc};

pub fn page(bytes: Arc<Vec<u8>>, destination: &Path) -> Result<Value> {
    let mut renderer = PageRenderer::new(
        bytes,
        RenderConfig {
            reject_oversize: true,
            ..RenderConfig::default()
        },
    );
    let result = renderer.render(RenderRequest {
        scale: 1.0,
        ..RenderRequest::default()
    });
    if let Some(error) = result.error {
        return Err(error.into());
    }
    if !result.warnings.is_empty() {
        return Err(format!("render_warnings: {:?}", result.warnings).into());
    }
    let dark = result
        .rgba
        .chunks_exact(4)
        .filter(|pixel| pixel[0] < 80 && pixel[1] < 80 && pixel[2] < 80 && pixel[3] > 240)
        .count();
    if dark < 500 {
        return Err("real_text_did_not_render".into());
    }
    let mut encoder = png::Encoder::new(
        BufWriter::new(File::create(destination)?),
        result.width,
        result.height,
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()?
        .write_image_data(result.rgba.as_ref())?;
    Ok(
        json!({"width":result.width,"height":result.height,"darkPixels":dark,
        "millis":result.millis,"warnings":0,"renderer":"actual upstream hayro/vello"}),
    )
}
