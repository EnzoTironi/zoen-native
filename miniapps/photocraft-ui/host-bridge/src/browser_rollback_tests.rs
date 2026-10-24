//! An upstream transform may succeed while its sparse result exceeds our profile.
use super::*;
use crate::{
    Operation,
    managed_tests::{head, mutation, scope},
};
use photocraft_doc::{ColorMode, Document, Layer, Rect, SampleType, Size};

#[test]
fn rejected_sparse_transform_preserves_actual_document_history_and_baseline() -> Result<()> {
    let mut doc = Document::new(
        "Sparse original",
        Size::new(320, 240),
        ColorMode::Rgb,
        SampleType::U8,
    );
    let mut layer = Layer::raster("Image", doc.pixel_format());
    layer
        .surface_mut()
        .ok_or("missing_surface")?
        .fill_rect(Rect::new(-2048, 0, -2047, 1), &[1.0, 0.0, 0.0, 1.0]);
    doc.layers.push(layer);
    let bytes = photocraft_format::save_to_bytes(&doc, &photocraft_format::SaveOptions::default())
        .map_err(|_| failure("fixture_encoding"))?;
    managed::checked(&bytes)?;
    let mut bridge = BrowserBridge::default();
    let mut session = Session::new();
    bridge.open(
        &mut session,
        Open {
            scope: scope(),
            head: head("1", &bytes),
        },
        &bytes,
    )?;
    let revision = session.active().ok_or("missing_document")?.revision;
    assert!(
        bridge
            .execute(
                &mut session,
                Command {
                    mutation: mutation(&bytes, "off_canvas"),
                    action: Operation::FlipHorizontal
                }
            )
            .is_err()
    );
    let current = photocraft_format::save_to_bytes(
        &session.active().ok_or("missing_document")?.doc,
        &photocraft_format::SaveOptions::default(),
    )
    .map_err(|_| failure("fixture_encoding"))?;
    assert_eq!(
        current, bytes,
        "refused profile must restore the real current document"
    );
    assert_eq!(
        session.active().ok_or("missing_document")?.revision,
        revision
    );
    assert!(!session.undo(), "refused edit must not leave an undo entry");
    let status = bridge.status(&session)?;
    assert_eq!(status["dirty"], false);
    assert!(status["pendingMutationId"].is_null());
    assert_eq!(
        bridge
            .export(
                &session,
                &Export {
                    scope: scope(),
                    head: head("1", &bytes),
                    format: managed::ExportFormat::Pcraft
                }
            )?
            .bytes,
        bytes
    );
    Ok(())
}
