use super::*;

#[test]
fn versioned_roundtrip_is_explicit_and_never_falls_back_to_binary() {
    let seed = seed();
    let doc = seed.open_editor().unwrap();
    let update = doc.full_update();
    let bytes = update.to_json().unwrap();
    let raw: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(raw["format"], FORMAT);
    assert_eq!(raw["version"], VERSION);
    assert_eq!(
        PlanUpdate::from_json(&bytes).unwrap().to_json().unwrap(),
        bytes
    );
    let duplicate = String::from_utf8(bytes).unwrap().replacen(
        "\"version\":1",
        "\"version\":2,\"version\":1",
        1,
    );
    assert!(PlanUpdate::from_json(duplicate.as_bytes()).is_err());
    assert!(PlanUpdate::from_json(&serde_json::to_vec(&update.0).unwrap()).is_err());
    assert!(PlanUpdate::from_json(&doc.page.doc.export(ExportMode::Snapshot).unwrap()).is_err());
    let mut unsupported = raw.clone();
    unsupported["version"] = Value::from(2);
    assert_eq!(
        PlanUpdate::from_json(&serde_json::to_vec(&unsupported).unwrap()).unwrap_err(),
        PlanError::UnsupportedCodec
    );
    let mut ignored = raw;
    ignored["operations"]["changes"][0]["ops"][0]["ignored_field"] = Value::Bool(true);
    assert!(PlanUpdate::from_json(&serde_json::to_vec(&ignored).unwrap()).is_err());
}

#[test]
fn bounded_codec_refuses_expansion_and_counter_arithmetic_before_import() {
    let seed = seed();
    let doc = seed.open_editor().unwrap();
    assert_eq!(
        PlanUpdate::from_json(&vec![b' '; MAX_PLAN_PACKET_BYTES + 1]).unwrap_err(),
        PlanError::Limit("plan packet bytes")
    );
    let deep = format!(
        "{}0{}",
        "[".repeat(MAX_JSON_DEPTH + 1),
        "]".repeat(MAX_JSON_DEPTH + 1)
    );
    assert_eq!(
        PlanUpdate::from_json(deep.as_bytes()).unwrap_err(),
        PlanError::Limit("plan JSON depth")
    );
    let broad = format!("[{}0]", "0,".repeat(MAX_JSON_NODES));
    assert!(broad.len() < MAX_PLAN_PACKET_BYTES);
    assert_eq!(
        PlanUpdate::from_json(broad.as_bytes()).unwrap_err(),
        PlanError::Limit("plan JSON nodes")
    );
    let nodes = read_nodes(&doc.page, seed.root()).unwrap();
    let node = nodes.iter().find(|n| n.kind == BlockKind::Line).unwrap();
    let mut raw = doc.full_update().0;
    raw.changes = vec![JsonChange {
        id: ID::new(88, 0),
        timestamp: 0,
        deps: vec![],
        lamport: 0,
        msg: None,
        ops: vec![JsonOp {
            counter: 0,
            container: node.text.id(),
            content: JsonOpContent::Text(JsonTextOp::Delete {
                pos: 0,
                len: i32::try_from(MAX_PLAN_ATOMS).unwrap() + 1,
                start_id: ID::new(0, 0),
            }),
        }],
    }];
    assert_eq!(
        PlanUpdate::from_json(&raw_packet(&raw)).unwrap_err(),
        PlanError::Limit("plan operation atoms")
    );
    raw.changes[0].id.counter = i32::MAX;
    raw.changes[0].ops[0].counter = i32::MAX;
    raw.changes[0].ops[0].content = JsonOpContent::Text(JsonTextOp::Insert {
        pos: 0,
        text: "x".into(),
    });
    assert!(PlanUpdate::from_json(&raw_packet(&raw)).is_err());
    assert_eq!(
        doc.view().unwrap().sections[0].lines[0].line.text,
        "abcdefghi"
    );
}
