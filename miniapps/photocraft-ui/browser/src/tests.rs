//! Actual upstream `PhotocraftApp`, pointer events, history and saved-byte renderer.
use egui::{Event, Modifiers, PointerButton};
use egui_kittest::Harness;
use photocraft_doc::{ColorMode, Document, Layer, Rect, SampleType, Size};
use photocraft_engine::Session;
use photocraft_format::SaveOptions;
use photocraft_ui_egui::theme::ThemeKind;
use photocraft_ui_egui::{PhotocraftApp, Services, canvas::ViewXform, state::Tool};
use photocraft_zoen_host::{Operation, browser::BrowserBridge, managed::*};
use serde_json::json;

type PhotoHarness = Harness<'static, PhotocraftApp>;
fn scope() -> Scope {
    Scope {
        artifact_id: "photo-ui-fixture".into(),
        actor_id: "human-fixture".into(),
        actor_kind: ActorKind::Human,
    }
}
fn head(n: &str, bytes: &[u8]) -> Head {
    Head {
        version: n.into(),
        sha256: hash(bytes),
    }
}
fn mutation(base: Head, id: &str) -> Mutation {
    Mutation {
        scope: scope(),
        base,
        mutation_id: id.into(),
    }
}
fn harness() -> Result<(PhotoHarness, BrowserBridge, Vec<u8>)> {
    let mut doc = Document::new(
        "Owned image",
        Size::new(320, 240),
        ColorMode::Rgb,
        SampleType::U8,
    );
    let mut layer = Layer::raster("Image", doc.pixel_format());
    layer
        .surface_mut()
        .ok_or("missing_surface")?
        .fill_rect(Rect::new(0, 0, 320, 240), &[1.0, 1.0, 1.0, 1.0]);
    doc.layers.push(layer);
    let bytes = photocraft_format::save_to_bytes(&doc, &SaveOptions::default())
        .map_err(|_| failure("seed_encoding"))?;
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
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .build_eframe(move |cc| {
            PhotocraftApp::setup_context(&cc.egui_ctx, ThemeKind::default());
            let mut app = PhotocraftApp::new(session, Services::default());
            app.ui.tool = Tool::Pencil;
            app.background_jobs = false;
            app
        });
    h.run_steps(6);
    Ok((h, bridge, bytes))
}
fn stroke(h: &mut PhotoHarness, x: f32, y: f32) -> Result<()> {
    let xf = ViewXform::active(h.state()).ok_or("missing_actual_canvas")?;
    let at = xf.to_screen(x, y);
    assert!(xf.rect.contains(at));
    h.input_mut().events.extend([
        Event::PointerMoved(at),
        Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        },
    ]);
    h.run_steps(1);
    h.input_mut().events.push(Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(3);
    Ok(())
}
fn actual_bytes(h: &PhotoHarness) -> Result<Vec<u8>> {
    photocraft_format::save_to_bytes(
        &h.state().session.active().ok_or("missing_document")?.doc,
        &SaveOptions::default(),
    )
    .map_err(|_| failure("native_encoding"))
}
#[test]
fn actual_pointer_checkpoint_delayed_ack_keeps_newer_pixels_undo_and_accepted_export() -> Result<()>
{
    let (mut h, mut bridge, original) = harness()?;
    stroke(&mut h, 40.0, 40.0)?;
    let first_bytes = actual_bytes(&h)?;
    assert_ne!(first_bytes, original, "real pointer must change pixels");
    let first = bridge.snapshot(
        &mut h.state_mut().session,
        mutation(head("1", &original), "manual-1"),
    )?;
    stroke(&mut h, 100.0, 100.0)?;
    let latest = actual_bytes(&h)?;
    assert_ne!(latest, first.bytes);
    assert_eq!(
        bridge
            .snapshot(
                &mut h.state_mut().session,
                mutation(head("1", &original), "manual-1")
            )?
            .bytes,
        first.bytes
    );
    assert_eq!(
        actual_bytes(&h)?,
        latest,
        "retry must not reset the visible painting"
    );
    let mut ack = Accept {
        scope: scope(),
        mutation_id: "wrong".into(),
        head: head("2", &first.bytes),
    };
    assert!(bridge.accept(&mut h.state_mut().session, &ack).is_err());
    ack.mutation_id = "manual-1".into();
    bridge.accept(&mut h.state_mut().session, &ack)?;
    assert_eq!(bridge.status(&h.state().session)?["dirty"], true);
    assert_eq!(actual_bytes(&h)?, latest);
    let png = bridge.export(
        &h.state().session,
        &Export {
            scope: scope(),
            head: ack.head.clone(),
            format: ExportFormat::Png,
        },
    )?;
    let rgba = photocraft_codecs::decode(&png.bytes)
        .map_err(|_| failure("png_decode"))?
        .to_rgba8();
    assert_eq!(
        &rgba[(40 * 320 + 40) * 4..(40 * 320 + 40) * 4 + 4],
        &[0, 0, 0, 255]
    );
    assert_eq!(
        &rgba[(100 * 320 + 100) * 4..(100 * 320 + 100) * 4 + 4],
        &[255, 255, 255, 255]
    );
    let second = bridge.snapshot(&mut h.state_mut().session, mutation(ack.head, "manual-2"))?;
    bridge.accept(
        &mut h.state_mut().session,
        &Accept {
            scope: scope(),
            mutation_id: "manual-2".into(),
            head: head("3", &second.bytes),
        },
    )?;
    assert!(h.state_mut().session.undo());
    assert_eq!(actual_bytes(&h)?, first.bytes);
    let mut cold = BrowserBridge::default();
    let mut session = Session::new();
    cold.open(
        &mut session,
        Open {
            scope: scope(),
            head: head("3", &second.bytes),
        },
        &second.bytes,
    )?;
    assert_eq!(
        cold.export(
            &session,
            &Export {
                scope: scope(),
                head: head("3", &second.bytes),
                format: ExportFormat::Pcraft
            }
        )?
        .bytes,
        latest
    );
    Ok(())
}
#[test]
fn actual_app_frozen_external_command_and_replaced_binding_cannot_mutate() -> Result<()> {
    let (mut h, mut bridge, bytes) = harness()?;
    bridge.freeze(&mut h.state_mut().session, true)?;
    for id in [
        "file.new",
        "file.open",
        "file.save",
        "actions.play",
        "paint.stroke",
    ] {
        assert!(h.state_mut().run(id, json!({})).is_err());
    }
    stroke(&mut h, 40.0, 40.0)?;
    assert_eq!(actual_bytes(&h)?, bytes);
    bridge.freeze(&mut h.state_mut().session, false)?;
    let p = bridge.execute(
        &mut h.state_mut().session,
        Command {
            mutation: mutation(head("1", &bytes), "agent"),
            action: Operation::RotateClockwise,
        },
    )?;
    assert_eq!(
        h.state().session.active().ok_or("missing_doc")?.doc.size,
        Size::new(240, 320)
    );
    assert!(h.state().session.managed_frozen);
    assert_eq!(
        bridge
            .execute(
                &mut h.state_mut().session,
                Command {
                    mutation: mutation(head("1", &bytes), "agent"),
                    action: Operation::RotateClockwise
                }
            )?
            .bytes,
        p.bytes
    );
    h.state_mut().session.managed_binding = None;
    assert!(
        bridge
            .accept(
                &mut h.state_mut().session,
                &Accept {
                    scope: scope(),
                    mutation_id: "agent".into(),
                    head: head("2", &p.bytes)
                }
            )
            .is_err()
    );
    Ok(())
}
