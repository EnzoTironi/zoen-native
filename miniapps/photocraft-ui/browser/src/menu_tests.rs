//! Human menu availability and actual clicks must use the same admitted intent.
use super::*;
use egui_kittest::kittest::Queryable;
use photocraft_ui_egui::menus;
use photocraft_zoen_host::browser::command_policy;
use serde_json::Value;

const PARAMETERLESS: [&str; 7] = [
    "image.imageRotation.flipCanvasHorizontal",
    "image.imageRotation.flipCanvasVertical",
    "image.imageRotation.90cw",
    "image.imageRotation.90ccw",
    "image.imageRotation.180",
    "edit.undo",
    "edit.redo",
];

#[test]
fn actual_human_menu_rotation_admission_click_checkpoint_cold_export() -> Result<()> {
    let (mut h, mut bridge, original) = harness()?;
    stroke(&mut h, 40.0, 60.0)?;
    let command = "image.imageRotation.90cw";
    assert!(
        h.state().session.is_enabled(command),
        "a managed human menu queries a parameterless command with Null"
    );
    let row = menus::menu_items(h.state())
        .into_iter()
        .find(|row| row.id == command)
        .ok_or("rotation_menu_row_missing")?;
    assert!(row.enabled);
    assert_eq!(row.path, ["Image", "Image Rotation"]);
    h.get_by_label("Image").click();
    h.run_steps(3);
    // kittest query_by_label_contains refuses duplicate matches; only the open
    // menu owns this row, whose upstream check/arrow columns decorate its label.
    h.query_by_label_contains("Image Rotation")
        .ok_or("actual_rotation_submenu_not_visible")?
        .hover();
    h.run_steps(3);
    h.query_by_label_contains("90° Clockwise")
        .ok_or("actual_clockwise_item_not_visible")?
        .click();
    h.run_steps(4);
    assert_eq!(
        h.state()
            .session
            .active()
            .ok_or("missing_document")?
            .doc
            .size,
        Size::new(240, 320)
    );
    let changed = actual_bytes(&h)?;
    let proposal = bridge.snapshot(
        &mut h.state_mut().session,
        mutation(head("1", &original), "human-menu"),
    )?;
    assert_eq!(
        proposal.metadata["operation"],
        json!({"kind":"humanCheckpoint"})
    );
    assert_eq!(proposal.bytes, changed);
    assert_eq!(
        bridge
            .export(
                &h.state().session,
                &Export {
                    scope: scope(),
                    head: head("1", &original),
                    format: ExportFormat::Pcraft,
                }
            )?
            .bytes,
        original,
        "pending menu pixels cannot replace the accepted export"
    );
    let accepted = head("2", &changed);
    bridge.accept(
        &mut h.state_mut().session,
        &Accept {
            scope: scope(),
            mutation_id: "human-menu".into(),
            head: accepted.clone(),
        },
    )?;
    clockwise_pixels(&bridge, &h.state().session, &accepted)?;
    assert!(h.state_mut().session.undo());
    assert_eq!(
        h.state()
            .session
            .active()
            .ok_or("missing_document")?
            .doc
            .size,
        Size::new(320, 240)
    );
    cold_export(accepted, &changed)?;
    Ok(())
}

fn clockwise_pixels(bridge: &BrowserBridge, session: &Session, accepted: &Head) -> Result<()> {
    let output = bridge.export(
        session,
        &Export {
            scope: scope(),
            head: accepted.clone(),
            format: ExportFormat::Png,
        },
    )?;
    let pixels = photocraft_codecs::decode(&output.bytes)
        .map_err(|_| failure("png_decode"))?
        .to_rgba8();
    let offset = (40 * 240 + (239 - 60)) * 4;
    assert_eq!(
        &pixels[offset..offset + 4],
        &[0, 0, 0, 255],
        "the native menu rotated the real dab clockwise"
    );
    Ok(())
}
fn cold_export(accepted: Head, changed: &[u8]) -> Result<()> {
    let mut cold = BrowserBridge::default();
    let mut session = Session::new();
    cold.open(
        &mut session,
        Open {
            scope: scope(),
            head: accepted.clone(),
        },
        changed,
    )?;
    assert_eq!(
        cold.export(
            &session,
            &Export {
                scope: scope(),
                head: accepted,
                format: ExportFormat::Pcraft
            }
        )?
        .bytes,
        changed
    );
    Ok(())
}

#[test]
fn parameterless_menu_defaults_do_not_widen_commands_or_mutations() -> Result<()> {
    let (mut h, mut bridge, original) = harness()?;
    for id in PARAMETERLESS {
        for params in [Value::Null, json!({})] {
            assert!(command_policy(&h.state().session, id, &params).is_ok());
        }
        for params in [
            json!({"target":"pixels"}),
            json!({"path":"foreign"}),
            json!([]),
            json!(""),
            json!(false),
            json!(0),
        ] {
            assert!(command_policy(&h.state().session, id, &params).is_err());
            assert!(h.state_mut().session.execute(id, params).is_err());
            assert_eq!(actual_bytes(&h)?, original);
        }
    }
    for id in [
        "file.open",
        "image.rotation.arbitrary",
        "image.imageSize",
        "unknown",
        "paint.stroke",
    ] {
        assert!(command_policy(&h.state().session, id, &Value::Null).is_err());
    }
    bridge.freeze(&mut h.state_mut().session, true)?;
    for id in PARAMETERLESS {
        assert!(!h.state().session.is_enabled(id));
        for params in [Value::Null, json!({})] {
            assert!(command_policy(&h.state().session, id, &params).is_err());
            assert!(h.state_mut().session.execute(id, params).is_err());
        }
    }
    assert_eq!(actual_bytes(&h)?, original);
    Ok(())
}
