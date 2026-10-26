use super::*;

#[test]
fn legacy_negatives_survive_but_new_raw_operations_cannot_bypass_commands() {
    let mut legacy = seed().legacy_at(1).unwrap().document.clone();
    legacy.budget_cents = Some(i64::MIN);
    legacy.sections[0].lines[0].cost_cents = -31;
    let seed = CanonicalPlanSeed::replay(&[LegacyVersion {
        event_id: "negative-creation".into(),
        document: legacy.clone(),
    }])
    .unwrap();
    let mut receiver = seed.open_editor().unwrap();
    let split_seed = split(&receiver, &receiver.full_update());
    receiver
        .import(&PlanUpdate::from_json(&split_seed.to_json().unwrap()).unwrap())
        .unwrap();
    assert_eq!(receiver.view().unwrap().to_plan_doc(), legacy);

    let mut author = seed.open_editor().unwrap();
    let from = author.version();
    author
        .apply(PlanCommand::ChangeLine {
            block: line_id(&author),
            expected: ExpectedLine::AnyCurrent,
            patch: LinePatch {
                text: Some("historical negative cost retained".into()),
                ..Default::default()
            },
        })
        .unwrap();
    receiver
        .import(&author.updates_since(&from).unwrap())
        .unwrap();
    assert_eq!(receiver.view().unwrap().budget_cents, Some(i64::MIN));
    assert_eq!(
        receiver.view().unwrap().sections[0].lines[0]
            .line
            .cost_cents,
        -31
    );

    for field in ["cost_cents", "budget_cents"] {
        let mut attacker = receiver.fork().unwrap();
        let vv = receiver.version();
        let view = receiver.view().unwrap();
        let command = if field == "cost_cents" {
            PlanCommand::ChangeLine {
                block: line_id(&attacker),
                expected: ExpectedLine::AnyCurrent,
                patch: LinePatch {
                    cost_cents: Some(-1),
                    ..Default::default()
                },
            }
        } else {
            PlanCommand::SetBudget(Some(-1))
        };
        assert!(attacker.apply(command).is_err());
        unchanged(&attacker, &vv, &view);

        // Generate real Loro operations outside PlanCommand, as a remote
        // peer could do. The packet grammar must enforce the same rule.
        let nodes = read_nodes(&attacker.page, seed.root()).unwrap();
        let node = if field == "cost_cents" {
            nodes.iter().find(|n| n.kind == BlockKind::Line).unwrap()
        } else {
            nodes.iter().find(|n| n.kind == BlockKind::Title).unwrap()
        };
        node.map.insert(field, -1_i64).unwrap();
        attacker.page.doc.commit();
        let update = attacker.updates_since(&vv).unwrap();
        assert!(PlanUpdate::from_json(&raw_packet(&update.0)).is_err());
        assert!(receiver.import(&update).is_err());
        unchanged(&receiver, &vv, &view);
    }
}

#[test]
fn removed_block_identity_cannot_be_reassigned_through_an_invisible_map() {
    let seed = seed();
    let mut doc = seed.open_editor().unwrap();
    let original = read_nodes(&doc.page, seed.root())
        .unwrap()
        .into_iter()
        .find(|n| n.kind == BlockKind::Line)
        .unwrap();
    // The remote branch still has this map alive while the receiver removes
    // it. Mutating after deletion is refused by Loro before admission.
    let attacker = doc.fork().unwrap();
    let from = attacker.version();
    attacker
        .page
        .doc
        .get_map(original.map.id())
        .insert("id", "new-line")
        .unwrap();
    attacker.page.doc.commit();
    let update = attacker.updates_since(&from).unwrap();
    doc.apply(PlanCommand::RemoveLine {
        block: original.id.clone(),
        expected: ExpectedLine::AnyCurrent,
    })
    .unwrap();
    let vv = doc.version();
    let view = doc.view().unwrap();
    assert!(doc
        .import(&PlanUpdate::from_json(&update.to_json().unwrap()).unwrap())
        .is_err());
    unchanged(&doc, &vv, &view);
}
