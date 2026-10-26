use super::*;

#[test]
fn known_ordinary_collision_cannot_hide_behind_an_exact_replayed_id() {
    let seed = seed();
    let mut author = seed.open_editor().unwrap();
    let mut receiver = seed.open_editor().unwrap();
    let from = author.version();
    change_cost(&mut author, 700);
    let good = author.updates_since(&from).unwrap();
    receiver
        .import(&PlanUpdate::from_json(&good.to_json().unwrap()).unwrap())
        .unwrap();
    let vv = receiver.version();
    let view = receiver.view().unwrap();
    let mut bad = good.clone();
    let JsonOpContent::Map(JsonMapOp::Insert { value, .. }) = &mut bad.0.changes[0].ops[0].content
    else {
        panic!("real cost register")
    };
    *value = LoroValue::I64(701);
    let decoded = PlanUpdate::from_json(&bad.to_json().unwrap()).unwrap();
    assert_eq!(
        receiver.import(&decoded).unwrap_err(),
        PlanError::OperationConflict
    );
    unchanged(&receiver, &vv, &view);
    receiver.import(&good).unwrap();
    unchanged(&receiver, &vv, &view);
}

#[test]
fn conflicting_overlaps_inside_one_packet_are_refused_before_first_import() {
    let seed = seed();
    let mut author = seed.open_editor().unwrap();
    let from = author.version();
    change_cost(&mut author, 700);
    let mut packet = author.updates_since(&from).unwrap();
    let mut duplicate = packet.0.changes[0].clone();
    let JsonOpContent::Map(JsonMapOp::Insert { value, .. }) = &mut duplicate.ops[0].content else {
        panic!("real cost register")
    };
    *value = LoroValue::I64(701);
    packet.0.changes.push(duplicate);
    assert_eq!(
        PlanUpdate::from_json(&raw_packet(&packet.0)).unwrap_err(),
        PlanError::OperationConflict
    );
    let mut receiver = seed.open_editor().unwrap();
    let vv = receiver.version();
    let view = receiver.view().unwrap();
    assert_eq!(
        receiver.import(&packet).unwrap_err(),
        PlanError::OperationConflict
    );
    unchanged(&receiver, &vv, &view);
    packet.0.changes.pop();
    packet.0.changes.push(packet.0.changes[0].clone());
    let retry = PlanUpdate::from_json(&raw_packet(&packet.0)).unwrap();
    receiver.import(&retry).unwrap();
    let vv = receiver.version();
    let view = receiver.view().unwrap();
    receiver.import(&retry).unwrap();
    unchanged(&receiver, &vv, &view);
    assert_eq!(view.sections[0].lines[0].line.cost_cents, 700);
}

#[test]
fn a_valid_overlapping_span_replays_and_a_forged_overlap_refuses_atomically() {
    let seed = seed();
    let mut author = seed.open_editor().unwrap();
    let from = author.version();
    author
        .apply(PlanCommand::ChangeLine {
            block: line_id(&author),
            expected: ExpectedLine::AnyCurrent,
            patch: LinePatch {
                text: Some("🐟road🚀".into()),
                ..Default::default()
            },
        })
        .unwrap();
    let full = author.updates_since(&from).unwrap();
    let sliced = split(&author, &full);
    let mut receiver = seed.open_editor().unwrap();
    let cut = sliced.0.changes.len() / 2;
    let partial = PlanUpdate(JsonSchema {
        changes: sliced.0.changes[..cut].to_vec(),
        ..sliced.0.clone()
    });
    receiver
        .import(&PlanUpdate::from_json(&partial.to_json().unwrap()).unwrap())
        .unwrap();
    let vv = receiver.version();
    let view = receiver.view().unwrap();
    let mut forged = full.clone();
    let peer = forged.0.changes[0].id.peer;
    let known_end = *receiver.page.version().get(&peer).unwrap();
    let op = forged
        .0
        .changes
        .iter_mut()
        .flat_map(|c| &mut c.ops)
        .find(|op| op.counter < known_end)
        .unwrap();
    if let JsonOpContent::Text(JsonTextOp::Delete { pos, .. }) = &mut op.content {
        *pos += 1;
    } else {
        panic!("fixture's first text operation is a deletion")
    }
    assert_eq!(
        receiver
            .import(&PlanUpdate::from_json(&forged.to_json().unwrap()).unwrap())
            .unwrap_err(),
        PlanError::OperationConflict
    );
    unchanged(&receiver, &vv, &view);
    receiver
        .import(&PlanUpdate::from_json(&full.to_json().unwrap()).unwrap())
        .unwrap();
    assert_eq!(receiver.view().unwrap(), author.view().unwrap());
}
