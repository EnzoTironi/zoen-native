use super::*;

#[test]
fn ordinary_lamport_is_immutable_and_real_slices_keep_the_same_causal_value() {
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
    let good = author.updates_since(&from).unwrap();
    let sliced = split(&author, &good);
    assert_eq!(atoms(&good.0).unwrap(), atoms(&sliced.0).unwrap());
    let mut receiver = seed.open_editor().unwrap();
    receiver.import(&good).unwrap();
    let vv = receiver.version();
    let view = receiver.view().unwrap();
    let mut forged = sliced.clone();
    forged.0.changes[0].lamport += 1;
    assert_eq!(
        receiver.import(&PlanUpdate::from_json(&forged.to_json().unwrap()).unwrap()),
        Err(PlanError::OperationConflict)
    );
    unchanged(&receiver, &vv, &view);
    receiver.import(&sliced).unwrap();
    unchanged(&receiver, &vv, &view);

    // Even a fresh receiver refuses a forged declared Lamport instead of
    // accepting bytes that Loro would recalculate and then reject on retry.
    let mut fresh = seed.open_editor().unwrap();
    let vv = fresh.version();
    let view = fresh.view().unwrap();
    let mut forged = good.clone();
    forged.0.changes[0].lamport += 1;
    assert_eq!(
        fresh.import(&PlanUpdate::from_json(&forged.to_json().unwrap()).unwrap()),
        Err(error("plan Lamport does not match causal dependencies"))
    );
    unchanged(&fresh, &vv, &view);
    fresh.import(&good).unwrap();
    assert_eq!(fresh.view().unwrap(), author.view().unwrap());

    // Ordinary commit grouping metadata is not operation identity.
    let mut regrouped = sliced;
    for change in &mut regrouped.0.changes {
        change.timestamp = 99;
        change.msg = Some("regrouped ordinary commit".into());
    }
    receiver.import(&regrouped).unwrap();
    assert_eq!(receiver.view().unwrap(), author.view().unwrap());
}

#[test]
fn missing_dependency_packet_retries_with_identical_bytes_after_predecessor() {
    let seed = seed();
    let author = seed.open_editor().unwrap();
    author.page.doc.set_peer_id(77).unwrap();
    let from = author.version();
    let nodes = read_nodes(&author.page, seed.root()).unwrap();
    let node = nodes.iter().find(|n| n.kind == BlockKind::Line).unwrap();
    node.text.insert(0, "arrived ").unwrap();
    author.page.doc.commit();
    let first = author.updates_since(&from).unwrap();
    let middle = author.version();
    node.map.insert("cost_cents", 444_i64).unwrap();
    author.page.doc.commit();
    let second = author.updates_since(&middle).unwrap().to_json().unwrap();
    let mut receiver = seed.open_editor().unwrap();
    let vv = receiver.version();
    let view = receiver.view().unwrap();
    assert_eq!(
        receiver
            .import(&PlanUpdate::from_json(&second).unwrap())
            .unwrap_err(),
        PlanError::MissingDependencies
    );
    unchanged(&receiver, &vv, &view);
    receiver
        .import(&PlanUpdate::from_json(&first.to_json().unwrap()).unwrap())
        .unwrap();
    receiver
        .import(&PlanUpdate::from_json(&second).unwrap())
        .unwrap();
    assert_eq!(receiver.view().unwrap(), author.view().unwrap());
    let vv = receiver.version();
    receiver
        .import(&PlanUpdate::from_json(&second).unwrap())
        .unwrap();
    assert_eq!(receiver.version(), vv);
}
