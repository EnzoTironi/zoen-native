use super::*;

#[test]
fn forged_known_seed_operation_is_refused_before_a_collision_can_hide_it() {
    let seed = seed();
    let mut doc = seed.open_editor().unwrap();
    let mut packet = doc.full_update();
    let mut changed = false;
    for c in &mut packet.0.changes {
        for op in &mut c.ops {
            if let JsonOpContent::Map(JsonMapOp::Insert { key, value }) = &mut op.content {
                if key == "cost_cents" {
                    *value = LoroValue::I64(900);
                    changed = true;
                }
            }
        }
    }
    assert!(changed);
    let packet = PlanUpdate::from_json(&packet.to_json().unwrap()).unwrap();
    assert_refused_without_change(&mut doc, &packet, PlanError::ReservedSeed);
    doc.import(&seed.open_editor().unwrap().full_update())
        .unwrap();
    assert_eq!(doc.view().unwrap().total, TotalCents::Cents(100));
}

#[test]
fn new_operations_from_the_reserved_peer_cannot_extend_the_frozen_seed() {
    let seed = seed();
    let mut doc = seed.open_editor().unwrap();
    // An actual Loro writer, impersonating the reserved peer after the real seed.
    let attacker = editor_fork(&doc.page).unwrap();
    attacker.doc.set_peer_id(MIGRATION_PEER).unwrap();
    block_map(&attacker, 3)
        .unwrap()
        .insert("cost_cents", 900_i64)
        .unwrap();
    attacker.doc.commit();
    let packet = PlanUpdate(attacker.doc.export_json_updates_without_peer_compression(
        &VersionVector::default(),
        &attacker.version(),
    ));
    assert_refused_without_change(&mut doc, &packet, PlanError::ReservedSeed);
}

#[test]
fn mixed_complete_and_missing_dependency_updates_refuse_atomically_then_retry_exactly() {
    let seed = seed();
    let mut author = seed.open_editor().unwrap();
    change(
        &mut author,
        LinePatch {
            text: Some("Booked".into()),
            ..Default::default()
        },
    );
    let prerequisite = author.full_update();
    let first = author.version();
    change(
        &mut author,
        LinePatch {
            cost_cents: Some(700),
            ..Default::default()
        },
    );
    let mut incomplete = author.updates_since(&first).unwrap();
    let mut independent = seed.open_editor().unwrap();
    change(
        &mut independent,
        LinePatch {
            done: Some(true),
            ..Default::default()
        },
    );
    incomplete
        .0
        .changes
        .extend(independent.full_update().0.changes);
    let incomplete = PlanUpdate::from_json(&incomplete.to_json().unwrap()).unwrap();
    let mut receiver = seed.open_editor().unwrap();
    assert_refused_without_change(&mut receiver, &incomplete, PlanError::MissingDependencies);
    receiver.import(&prerequisite).unwrap();
    receiver.import(&incomplete).unwrap();
    let line = &receiver.view().unwrap().sections[0].lines[0].line;
    assert_eq!(line.text, "Booked");
    assert_eq!(line.cost_cents, 700);
    assert!(line.done);
    let version = receiver.version();
    receiver.import(&incomplete).unwrap();
    assert_eq!(receiver.version(), version);
}

#[test]
fn a_claimed_extension_cannot_change_the_authenticated_ancestry() {
    let seed = seed();
    let mut doc = seed.open_editor().unwrap();
    let before = doc.full_update().to_json().unwrap();
    let mut rewritten = original();
    rewritten.sections[0].lines[0].cost_cents = 900;
    let wrong = CanonicalPlanSeed::replay(&history(rewritten)).unwrap();
    assert_eq!(
        doc.admit_legacy_prefix(&wrong),
        Err(PlanError::IncompatiblePrefix)
    );
    assert_eq!(doc.full_update().to_json().unwrap(), before);
}

#[test]
fn unsupported_kinds_do_not_become_paragraphs_and_drop_financial_metadata() {
    let seed = seed();
    let mut doc = seed.open_editor().unwrap();
    let attacker = editor_fork(&doc.page).unwrap();
    block_map(&attacker, 3)
        .unwrap()
        .insert("kind", "paragraph")
        .unwrap();
    attacker.doc.commit();
    let packet = PlanUpdate(
        attacker
            .doc
            .export_json_updates_without_peer_compression(&doc.page.version(), &attacker.version()),
    );
    assert_refused_without_change(
        &mut doc,
        &packet,
        error("unsupported or invalid plan attribute"),
    );
}

#[test]
fn approval_hash_survives_promotion_and_done_but_not_a_new_cost() {
    let seed = seed();
    let mut doc = seed.open_editor().unwrap();
    let original_hash = line_hash(&original().sections[0].lines[0]).unwrap();
    assert_eq!(
        doc.view().unwrap().sections[0].lines[0].content_hash,
        original_hash
    );
    let block = line_block(&doc);
    doc.apply(PlanCommand::ChangeLine {
        block: block.clone(),
        expected: ExpectedLine::Hash(original_hash.clone()),
        patch: LinePatch {
            done: Some(true),
            ..Default::default()
        },
    })
    .unwrap();
    assert_eq!(
        doc.view().unwrap().sections[0].lines[0].content_hash,
        original_hash
    );
    change(
        &mut doc,
        LinePatch {
            cost_cents: Some(200),
            ..Default::default()
        },
    );
    let before = doc.version();
    assert_eq!(
        doc.apply(PlanCommand::ChangeLine {
            block,
            expected: ExpectedLine::Hash(original_hash),
            patch: LinePatch {
                done: Some(false),
                ..Default::default()
            }
        }),
        Err(PlanError::StaleLine)
    );
    assert_eq!(doc.version(), before);
    assert!(doc.view().unwrap().sections[0].lines[0].line.done);
}

#[test]
fn moving_a_line_preserves_its_container_and_a_concurrent_text_edit() {
    let seed = seed();
    let mut moved = seed.open_editor().unwrap();
    let mut edited = seed.open_editor().unwrap();
    let original_block = line_block(&moved);
    let CommandResult::Created(new_section) = moved
        .apply(PlanCommand::AddSection {
            title: "Transport".into(),
        })
        .unwrap()
    else {
        panic!("section not created");
    };
    moved
        .apply(PlanCommand::MoveAfter {
            block: original_block.clone(),
            after: new_section.clone(),
        })
        .unwrap();
    change(
        &mut edited,
        LinePatch {
            text: Some("Modified while moving".into()),
            ..Default::default()
        },
    );
    let (view, _) = merge_in_both_orders(&seed, &[moved.full_update(), edited.full_update()]);
    assert_eq!(view.sections[0].lines, []);
    assert_eq!(view.sections[1].block_id.as_ref(), Some(&new_section));
    assert_eq!(view.sections[1].lines[0].block_id, original_block);
    assert_eq!(view.sections[1].lines[0].line.text, "Modified while moving");
    assert_eq!(view.total, TotalCents::Cents(100));
}
