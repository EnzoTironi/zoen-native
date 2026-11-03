use super::*;

#[test]
fn role_and_legacy_identity_reassignment_refuse_the_entire_typed_edit() {
    let mut doc = seed().open_editor().unwrap();
    let revision = doc.revision().unwrap();
    let before = doc.blocks().unwrap();
    for changed_identity in [false, true] {
        let mut invalid = edit(&line_block(&doc));
        if changed_identity {
            if let PlanBlockContent::Line { legacy_id, .. } = &mut invalid.content {
                *legacy_id = "other-line".into();
            }
        } else {
            invalid.content = PlanBlockContent::Prose {
                kind: Kind::Paragraph,
            };
        }
        let mut good = edit(&before[0]);
        good.text = "Must not be applied".into();
        assert!(doc
            .apply_from(&revision, &order(&doc), &[good, invalid])
            .is_err());
        assert_unchanged(&doc, &revision, &before);
    }
}

#[test]
fn anchors_cannot_be_removed_moved_duplicated_or_created() {
    let mut doc = seed().open_editor().unwrap();
    let revision = doc.revision().unwrap();
    let before = doc.blocks().unwrap();
    let normal = order(&doc);
    let mut moved = normal.clone();
    moved.swap(1, 2);
    let mut duplicated = normal.clone();
    duplicated.push(normal[0].clone());
    let mut created = normal.clone();
    created.push("second-summary".into());
    for desired in [normal[1..].to_vec(), moved, duplicated] {
        assert!(doc.apply_from(&revision, &desired, &[]).is_err());
        assert_unchanged(&doc, &revision, &before);
    }
    let edit = PlanBlockEdit {
        id: "second-summary".into(),
        content: PlanBlockContent::Summary,
        text: "Duplicate".into(),
        spans: vec![],
    };
    assert!(doc.apply_from(&revision, &created, &[edit]).is_err());
    assert_unchanged(&doc, &revision, &before);
}

#[test]
fn deleted_historical_id_is_refused_by_typed_api_raw_new_map_and_old_map_reinsertion() {
    let mut doc = seed().open_editor().unwrap();
    let old = line_block(&doc);
    let old_map = find(&read_nodes(&doc.page, doc.seed.root()).unwrap(), &old.id)
        .unwrap()
        .map
        .id();
    let mut desired = order(&doc);
    desired.retain(|id| id != &old.id);
    doc.apply_from(&doc.revision().unwrap(), &desired, &[])
        .unwrap();
    let revision = doc.revision().unwrap();
    let before = doc.blocks().unwrap();
    desired.push(old.id.clone());
    assert!(doc
        .apply_from(&revision, &desired, &[new_line(&old.id, "Reuse", 12)])
        .is_err());
    assert_unchanged(&doc, &revision, &before);
    let attacker = doc.fork().unwrap();
    let from = attacker.version();
    let map = attacker
        .page
        .blocks_list()
        .insert_container(attacker.page.blocks_list().len(), LoroMap::new())
        .unwrap();
    let line = PlanLine {
        id: format!("ln_{}", old.id),
        text: "Reuse".into(),
        cost_cents: 12,
        done: false,
    };
    initialize_block(
        &map,
        &old.id,
        BlockKind::Line,
        &line.text,
        Some(&line),
        None,
    )
    .unwrap();
    attacker.page.doc.commit();
    let packet = wire(&attacker.updates_since(&from).unwrap());
    assert!(doc.import(&packet).is_err());
    assert_unchanged(&doc, &revision, &before);
    let mut reinsertion = packet;
    for op in reinsertion
        .0
        .changes
        .iter_mut()
        .flat_map(|change| &mut change.ops)
    {
        if let JsonOpContent::MovableList(JsonMovableListOp::Insert { value, .. }) = &mut op.content
        {
            value[0] = LoroValue::Container(old_map.clone());
            break;
        }
    }
    assert!(doc.import(&reinsertion).is_err());
    assert_unchanged(&doc, &revision, &before);
}

#[test]
fn known_earlier_revision_reads_and_edits_exact_history_but_cross_origin_and_future_refuse() {
    let seed = seed();
    let mut doc = seed.open_editor().unwrap();
    let old_revision = doc.revision().unwrap();
    let old_blocks = doc.blocks().unwrap();
    let mut changed = edit(&line_block(&doc));
    changed.text = "New text".into();
    apply(&mut doc, vec![changed]);
    assert_eq!(doc.at(&old_revision).unwrap().blocks().unwrap(), old_blocks);
    let current = doc.revision().unwrap();
    let before = doc.blocks().unwrap();
    let other_seed = CanonicalPlanSeed::replay(&[LegacyVersion {
        event_id: "other-origin".into(),
        document: original(),
    }])
    .unwrap();
    let other = other_seed.open_editor().unwrap().revision().unwrap();
    assert_eq!(doc.at(&other).err(), Some(PlanError::InvalidRevision));
    assert_eq!(
        doc.apply_from(&other, &order(&doc), &[]).unwrap_err(),
        PlanError::InvalidRevision
    );
    let older = seed.open_editor().unwrap();
    assert_eq!(older.at(&current).err(), Some(PlanError::InvalidRevision));
    let mut missing = current.clone();
    missing.frontiers = Frontiers::from(loro::ID::new(8877, 0));
    assert_eq!(doc.at(&missing).err(), Some(PlanError::InvalidRevision));
    assert_unchanged(&doc, &current, &before);
}

#[test]
fn equal_visible_state_and_vv_do_not_authorize_a_different_operation_history() {
    let seed = seed();
    let a = seed.open_editor().unwrap();
    let b = seed.open_editor().unwrap();
    for (doc, temporary) in [(&a, 101_i64), (&b, 102_i64)] {
        doc.page.doc.set_peer_id(77).unwrap();
        let nodes = read_nodes(&doc.page, doc.seed.root()).unwrap();
        let node = nodes
            .iter()
            .find(|node| node.kind == BlockKind::Line)
            .unwrap();
        node.map.insert("cost_cents", temporary).unwrap();
        node.map.insert("cost_cents", 100_i64).unwrap();
        doc.page.doc.commit();
    }
    assert_eq!(a.version(), b.version());
    assert_eq!(a.semantic_hash().unwrap(), b.semantic_hash().unwrap());
    let a_revision = a.revision().unwrap();
    assert!(a.at(&a_revision).is_ok());
    assert_eq!(b.at(&a_revision).err(), Some(PlanError::InvalidRevision));
}

#[test]
fn rich_prose_deltas_retry_identically_after_missing_dependencies_without_duplicate_changes() {
    let seed = seed();
    let mut author = seed.open_editor().unwrap();
    let from = author.version();
    let mut desired = order(&author);
    desired.push("new-prose".into());
    author
        .apply_from(
            &author.revision().unwrap(),
            &desired,
            &[prose("new-prose", Kind::Paragraph, RAW)],
        )
        .unwrap();
    let first = wire(&author.updates_since(&from).unwrap());
    let middle = author.version();
    let mut changed = edit(author.blocks().unwrap().last().unwrap());
    changed.spans = vec![span(RAW, "b")];
    changed.content = PlanBlockContent::Prose {
        kind: Kind::Heading {
            level: 3,
            setext: false,
        },
    };
    apply(&mut author, vec![changed]);
    let second_bytes = author.updates_since(&middle).unwrap().to_json().unwrap();
    let mut receiver = seed.open_editor().unwrap();
    let revision = receiver.revision().unwrap();
    let before = receiver.blocks().unwrap();
    assert_eq!(
        receiver
            .import(&PlanUpdate::from_json(&second_bytes).unwrap())
            .unwrap_err(),
        PlanError::MissingDependencies
    );
    assert_unchanged(&receiver, &revision, &before);
    receiver.import(&first).unwrap();
    receiver
        .import(&PlanUpdate::from_json(&second_bytes).unwrap())
        .unwrap();
    assert_eq!(receiver.blocks().unwrap(), author.blocks().unwrap());
    let final_revision = receiver.revision().unwrap();
    receiver.import(&first).unwrap();
    receiver
        .import(&PlanUpdate::from_json(&second_bytes).unwrap())
        .unwrap();
    assert_eq!(receiver.revision().unwrap(), final_revision);
}

#[test]
fn unknown_prose_ignored_fields_and_wrong_role_attributes_refuse_even_when_hidden() {
    let seed = seed();
    let mut receiver = seed.open_editor().unwrap();
    let revision = receiver.revision().unwrap();
    let before = receiver.blocks().unwrap();
    for encoded in [
        r#"{"k":"unknown"}"#,
        r#"{"k":"heading","level":0}"#,
        r#"{"k":"paragraph","cost_cents":1}"#,
        r#"{"k":"numbered","indent":0,"number":0}"#,
        r#"{"k":"paragraph","k":"quote"}"#,
    ] {
        assert!(parse_prose_kind(encoded).is_err());
        let author = receiver.fork().unwrap();
        let from = author.version();
        let map = author
            .page
            .blocks_list()
            .insert_container(author.page.blocks_list().len(), LoroMap::new())
            .unwrap();
        initialize_block(
            &map,
            "invalid-prose",
            BlockKind::Prose,
            "Visible",
            None,
            None,
        )
        .unwrap();
        map.insert("prose_kind", encoded).unwrap();
        map.insert("prose_kind", r#"{"k":"paragraph"}"#).unwrap();
        author.page.doc.commit();
        assert!(receiver
            .import(&author.updates_since(&from).unwrap())
            .is_err());
        assert_unchanged(&receiver, &revision, &before);
    }
    let author = receiver.fork().unwrap();
    let from = author.version();
    let nodes = read_nodes(&author.page, author.seed.root()).unwrap();
    let node = nodes
        .iter()
        .find(|node| node.kind == BlockKind::Line)
        .unwrap();
    node.map
        .insert("prose_kind", r#"{"k":"paragraph"}"#)
        .unwrap();
    let position = block_position(&author.page, &node.id).unwrap().unwrap();
    author.page.blocks_list().delete(position, 1).unwrap();
    author.page.doc.commit();
    assert!(receiver
        .import(&author.updates_since(&from).unwrap())
        .is_err());
    assert_unchanged(&receiver, &revision, &before);
}

#[test]
fn typed_invalid_spans_and_prose_metadata_refuse_atomically() {
    let mut doc = seed().open_editor().unwrap();
    let revision = doc.revision().unwrap();
    let before = doc.blocks().unwrap();
    let invalid = [
        vec![Span {
            start: 2,
            end: 3,
            key: "b".into(),
            value: String::new(),
        }],
        vec![Span {
            start: 3,
            end: 2,
            key: "b".into(),
            value: String::new(),
        }],
        vec![Span {
            start: 0,
            end: 1,
            key: "unknown".into(),
            value: String::new(),
        }],
        vec![Span {
            start: 0,
            end: 1,
            key: "b".into(),
            value: "wrong".into(),
        }],
        vec![
            Span {
                start: 0,
                end: 4,
                key: "a".into(),
                value: "one".into(),
            },
            Span {
                start: 1,
                end: 4,
                key: "a".into(),
                value: "two".into(),
            },
        ],
    ];
    for spans in invalid {
        let mut changed = edit(&line_block(&doc));
        changed.spans = spans;
        assert!(doc.apply_from(&revision, &order(&doc), &[changed]).is_err());
        assert_unchanged(&doc, &revision, &before);
    }
    let mut desired = order(&doc);
    desired.push("bad-heading".into());
    assert!(doc
        .apply_from(
            &revision,
            &desired,
            &[prose(
                "bad-heading",
                Kind::Heading {
                    level: 7,
                    setext: false
                },
                RAW
            )]
        )
        .is_err());
    assert_unchanged(&doc, &revision, &before);
}
