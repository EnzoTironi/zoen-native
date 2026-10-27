//! Real upstream paint preview and the shared close/checkpoint sequence.
use super::*;
use photocraft_engine::{brush_cmds::LiveStroke, paint::StrokePoint};

#[test]
fn close_freeze_can_checkpoint_real_pixels_and_ack_remains_paused() -> Result<()> {
    let (mut h, mut bridge, original) = harness()?;
    stroke(&mut h, 40.0, 40.0)?;
    let painted = actual_bytes(&h)?;
    bridge.freeze(&mut h.state_mut().session, true)?;
    let proposal = bridge.snapshot(
        &mut h.state_mut().session,
        mutation(head("1", &original), "close-checkpoint"),
    )?;
    assert_eq!(proposal.bytes, painted);
    bridge.accept(
        &mut h.state_mut().session,
        &Accept {
            scope: scope(),
            mutation_id: "close-checkpoint".into(),
            head: head("2", &painted),
        },
    )?;
    assert!(h.state().session.managed_frozen);
    assert_eq!(bridge.status(&h.state().session)?["dirty"], false);
    stroke(&mut h, 100.0, 100.0)?;
    assert_eq!(actual_bytes(&h)?, painted);
    bridge.freeze(&mut h.state_mut().session, false)?;
    assert!(
        h.state_mut().session.undo(),
        "checkpoint must retain the real Undo"
    );
    assert_eq!(actual_bytes(&h)?, original);
    Ok(())
}

#[test]
fn live_preview_refuses_frozen_oversized_and_off_canvas_before_rendering() -> Result<()> {
    let (mut h, mut bridge, original) = harness()?;
    let params = json!({"points":[[40.0,40.0]],"target":"pixels"});
    let mut preview = LiveStroke::begin_with(&h.state().session, "paint.pencil", &params)
        .map_err(|_| failure("real_preview_refused"))?;
    let before = photocraft_format::save_to_bytes(&preview.doc, &SaveOptions::default())
        .map_err(|_| failure("preview_encoding"))?;
    assert_ne!(
        before, original,
        "preview must really render the initial dab"
    );
    for points in [
        vec![StrokePoint::new(-1.0, 40.0, 1.0)],
        vec![StrokePoint::new(320.0, 40.0, 1.0)],
        vec![StrokePoint::new(40.0, 40.0, 1.0); 4096],
    ] {
        assert!(preview.push(&points).is_err());
        assert_eq!(
            photocraft_format::save_to_bytes(&preview.doc, &SaveOptions::default())
                .map_err(|_| failure("preview_encoding"))?,
            before
        );
    }
    h.state_mut().session.tools.brush.size = 129.0;
    assert!(LiveStroke::begin_with(&h.state().session, "paint.pencil", &params).is_err());
    h.state_mut().session.tools.brush.size = 1.0;
    for p in [
        json!({"points":[[40,40]],"target":"mask"}),
        json!({"points":[[40,40]],"preset":"external"}),
        json!({"points":[[-1,40]],"target":"pixels"}),
    ] {
        assert!(LiveStroke::begin_with(&h.state().session, "paint.pencil", &p).is_err());
        assert!(h.state_mut().session.execute("paint.pencil", p).is_err());
    }
    bridge.freeze(&mut h.state_mut().session, true)?;
    assert!(LiveStroke::begin_with(&h.state().session, "paint.pencil", &params).is_err());
    assert_eq!(actual_bytes(&h)?, original);
    Ok(())
}
