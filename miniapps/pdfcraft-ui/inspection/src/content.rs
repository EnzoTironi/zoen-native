//! Real paragraph editing, raster PDF import and multi-page inspection.
use crate::fixture::{Result, open, unknown_data};
use pdfcraft_cos::{SaveOptions, write_full};
use pdfcraft_engine::{BlockStyle, Edit};
use serde_json::{Value, json};
use std::{fs, path::Path, sync::Arc, time::Instant};

pub const REPLACEMENT: &str = "Human and agent approved this shared PDF.";

pub fn paragraph(bytes: Arc<Vec<u8>>, out: &Path) -> Result<Value> {
    let (mut session, id) = open(bytes)?;
    let blocks = session.get(id).ok_or("missing_document")?.text_blocks(0);
    let index = blocks
        .iter()
        .position(|block| block.text == "Human and agent edits share one PDF.")
        .ok_or("real_paragraph_not_found")?;
    session.apply(
        id,
        Edit::EditTextBlock {
            page: 0,
            block: index,
            text: REPLACEMENT.into(),
            style: BlockStyle::default(),
        },
    )?;
    let document = session.get(id).ok_or("missing_document")?;
    if !document.edit_notices.is_empty() {
        return Err("paragraph_edit_reported_representation_loss".into());
    }
    if !document
        .text_blocks(0)
        .iter()
        .any(|block| block.text == REPLACEMENT)
    {
        return Err("actual_engine_paragraph_not_replaced".into());
    }
    let edited = session.save_bytes(id)?;
    session.check_save(id, &edited)?;
    unknown_data(edited.clone())?;
    fs::write(out.join("paragraph-edited.pdf"), edited.as_slice())?;
    let raster = crate::render::page(edited.clone(), &out.join("paragraph-edited.png"))?;
    session.undo(id)?;
    if session
        .get(id)
        .ok_or("missing_document")?
        .text_blocks(0)
        .iter()
        .any(|block| block.text == REPLACEMENT)
    {
        return Err("paragraph_undo_failed".into());
    }
    session.redo(id)?;
    let (cold, cold_id) = open(edited.clone())?;
    if !cold
        .get(cold_id)
        .ok_or("missing_cold_document")?
        .text_blocks(0)
        .iter()
        .any(|block| block.text == REPLACEMENT)
    {
        return Err("paragraph_cold_recovery_failed".into());
    }
    Ok(
        json!({"bytes":edited.len(),"replacement":REPLACEMENT,"unknownPreserved":true,
        "undoRedo":true,"coldRecovery":true,"render":raster}),
    )
}

pub fn cases(out: &Path) -> Result<Value> {
    let start = Instant::now();
    let text = (1..=32).map(|page| format!("Shared review - page {page}\n\nOriginal contributor text for the real PDF engine.\n\nA canonical artifact retains every source revision.\n\nPeople and agents review the same document.\n\nComments, paragraphs and page geometry must survive recovery."))
        .collect::<Vec<_>>().join("\u{c}");
    let doc = pdfcraft_create::from_text("Long review", &text, (595.0, 842.0), 15.0)?;
    let bytes = Arc::new(write_full(&doc, &SaveOptions::default())?);
    let (mut session, id) = open(bytes.clone())?;
    if session
        .get(id)
        .ok_or("missing_long_document")?
        .info
        .pages
        .len()
        != 32
    {
        return Err("long_document_page_count_mismatch".into());
    }
    session.apply(
        id,
        Edit::RotatePages {
            pages: vec![31],
            degrees: 90,
        },
    )?;
    let changed = session.save_bytes(id)?;
    session.check_save(id, &changed)?;
    fs::write(out.join("review-32-pages.pdf"), changed.as_slice())?;
    let long = json!({"pages":32,"bytes":changed.len(),"lastPageRotation":90,
        "elapsedMillis":start.elapsed().as_millis()});
    let image = image_pdf()?;
    fs::write(out.join("original-image.pdf"), image.as_slice())?;
    let (mut session, id) = open(image.clone())?;
    if session
        .get(id)
        .ok_or("missing_image_document")?
        .info
        .pages
        .len()
        != 1
    {
        return Err("image_document_page_count_mismatch".into());
    }
    let raster = crate::render::page(image, &out.join("original-image.png"))?;
    session.apply(
        id,
        Edit::RotatePages {
            pages: vec![0],
            degrees: 90,
        },
    )?;
    let changed = session.save_bytes(id)?;
    session.check_save(id, &changed)?;
    let (cold, cold_id) = open(changed)?;
    if cold
        .get(cold_id)
        .ok_or("missing_image_document")?
        .info
        .pages[0]
        .rotation
        != 90
    {
        return Err("image_rotation_recovery_failed".into());
    }
    let jpeg = if let Some(path) = std::env::args().nth(2) {
        let input = fs::read(path)?;
        let doc = pdfcraft_create::from_images(&[("Contributor pattern.jpg".into(), input)])?;
        let bytes = Arc::new(write_full(&doc, &SaveOptions::default())?);
        fs::write(out.join("original-jpeg.pdf"), bytes.as_slice())?;
        let (mut session, id) = open(bytes.clone())?;
        let raster = crate::render::page(bytes, &out.join("original-jpeg.png"))?;
        session.apply(
            id,
            Edit::RotatePages {
                pages: vec![0],
                degrees: 90,
            },
        )?;
        let changed = session.save_bytes(id)?;
        session.check_save(id, &changed)?;
        fs::write(out.join("rotated-jpeg.pdf"), changed.as_slice())?;
        let (cold, cold_id) = open(changed.clone())?;
        if cold.get(cold_id).ok_or("missing_jpeg_document")?.info.pages[0].rotation != 90
            || cold.save_bytes(cold_id)? != changed
        {
            return Err("jpeg_rotation_recovery_failed".into());
        }
        json!({"executed":true,"render":raster,"coldRecoveryExact":true})
    } else {
        json!({"executed":false})
    };
    Ok(json!({"longDocument":long,"imageDocument":raster,"jpegDocument":jpeg}))
}

fn image_pdf() -> Result<Arc<Vec<u8>>> {
    let mut pixels = Vec::with_capacity(1600 * 1200 * 3);
    for y in 0_u16..1200 {
        for x in 0_u16..1600 {
            let stripe = if (x / 100 + y / 100) % 2 == 0 {
                20
            } else {
                220
            };
            pixels.extend([stripe, u8::try_from(x % 256)?, u8::try_from(y % 256)?]);
        }
    }
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, 1600, 1200);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header()?.write_image_data(&pixels)?;
    }
    let doc = pdfcraft_create::from_images(&[("Contributor pattern.png".into(), png)])?;
    Ok(Arc::new(write_full(&doc, &SaveOptions::default())?))
}
