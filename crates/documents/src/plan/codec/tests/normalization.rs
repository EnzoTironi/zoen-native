use super::*;

#[test]
fn canonical_atom_normalization_matches_real_unicode_and_reverse_delete_slices() {
    let seed = seed();
    let author = seed.open_editor().unwrap();
    author.page.doc.set_peer_id(17).unwrap();
    let from = author.version();
    let nodes = read_nodes(&author.page, seed.root()).unwrap();
    let line = nodes.iter().find(|n| n.kind == BlockKind::Line).unwrap();
    line.text.insert(2, "🐟é\u{2028}").unwrap();
    // Forward run, then actual repeated backspaces that Loro merges into a reverse run.
    line.text.delete(0, 2).unwrap();
    line.text.delete(7, 1).unwrap();
    line.text.delete(6, 1).unwrap();
    line.text.delete(5, 1).unwrap();
    author.page.doc.commit();
    let packet = author.updates_since(&from).unwrap();
    assert!(packet.0.changes.iter().flat_map(|c| &c.ops).any(|op| matches!(&op.content, JsonOpContent::Text(JsonTextOp::Delete { len, .. }) if *len < -1)), "fixture must contain an actual reverse run");
    let sliced = split(&author, &packet);
    assert_eq!(atoms(&packet.0).unwrap(), atoms(&sliced.0).unwrap());
    let mut a = seed.open_editor().unwrap();
    let mut b = seed.open_editor().unwrap();
    a.import(&PlanUpdate::from_json(&packet.to_json().unwrap()).unwrap())
        .unwrap();
    b.import(&PlanUpdate::from_json(&sliced.to_json().unwrap()).unwrap())
        .unwrap();
    assert_eq!(a.view().unwrap(), author.view().unwrap());
    assert_eq!(b.view().unwrap(), author.view().unwrap());
    let before = a.version();
    a.import(&sliced).unwrap();
    assert_eq!(a.version(), before);
}

#[test]
fn reserved_prefix_accepts_valid_splits_but_refuses_a_forged_slice() {
    let seed = seed();
    let doc = seed.open_editor().unwrap();
    let packet = split(&doc, &doc.full_update());
    let mut receiver = seed.open_editor().unwrap();
    receiver
        .import(&PlanUpdate::from_json(&packet.to_json().unwrap()).unwrap())
        .unwrap();
    assert_eq!(receiver.view().unwrap(), doc.view().unwrap());
    let vv = receiver.version();
    let view = receiver.view().unwrap();
    let mut forged = packet;
    let change = forged.0.changes.iter_mut().find(|c| c.ops.iter().any(|op| matches!(&op.content, JsonOpContent::Map(JsonMapOp::Insert { key, .. }) if key == "cost_cents"))).unwrap();
    let JsonOpContent::Map(JsonMapOp::Insert { value, .. }) = &mut change.ops[0].content else {
        unreachable!()
    };
    *value = LoroValue::I64(999);
    assert_eq!(
        receiver
            .import(&PlanUpdate::from_json(&forged.to_json().unwrap()).unwrap())
            .unwrap_err(),
        PlanError::ReservedSeed
    );
    unchanged(&receiver, &vv, &view);
}
