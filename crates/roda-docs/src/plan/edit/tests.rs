use super::*;
use loro::{IdSpan, JsonSchema, JsonTextOp};

const RAW: &str = "a😀b\r\n\0e\u{301}\r\u{85}\u{2029}tail";

fn original() -> PlanDoc {
    PlanDoc {
        title: "Title\r\nexact".into(),
        summary: "Summary\0\u{85}".into(),
        budget_cents: Some(1_000),
        sections: vec![PlanSection {
            title: "Section\r".into(),
            lines: vec![
                PlanLine {
                    id: "duplicate-legacy-".repeat(100),
                    text: RAW.into(),
                    cost_cents: 100,
                    done: false,
                },
                PlanLine {
                    id: "duplicate-legacy-".repeat(100),
                    text: "Second".into(),
                    cost_cents: 200,
                    done: false,
                },
            ],
        }],
    }
}

fn seed_with(document: PlanDoc) -> CanonicalPlanSeed {
    CanonicalPlanSeed::replay(&[LegacyVersion {
        event_id: "authenticated-plan-creation".into(),
        document,
    }])
    .unwrap()
}

fn seed() -> CanonicalPlanSeed {
    seed_with(original())
}

fn order(doc: &PlanDocument) -> Vec<String> {
    doc.blocks()
        .unwrap()
        .into_iter()
        .map(|block| block.id)
        .collect()
}

fn line_block(doc: &PlanDocument) -> PlanBlock {
    doc.blocks()
        .unwrap()
        .into_iter()
        .find(|block| matches!(block.content, PlanBlockContent::Line { .. }))
        .unwrap()
}

fn edit(block: &PlanBlock) -> PlanBlockEdit {
    PlanBlockEdit {
        id: block.id.clone(),
        content: block.content.clone(),
        text: block.plain_text(),
        spans: crate::spans_of(&block.runs),
    }
}

fn span(text: &str, key: &str) -> Span {
    Span {
        start: 0,
        end: text.encode_utf16().count() as u32,
        key: key.into(),
        value: if key == "a" {
            "https://example.test/exact?q=é".into()
        } else {
            String::new()
        },
    }
}

fn prose(id: &str, kind: Kind, text: &str) -> PlanBlockEdit {
    PlanBlockEdit {
        id: id.into(),
        content: PlanBlockContent::Prose { kind },
        text: text.into(),
        spans: vec![],
    }
}

fn new_line(id: &str, text: &str, cents: i64) -> PlanBlockEdit {
    PlanBlockEdit {
        id: id.into(),
        content: PlanBlockContent::Line {
            legacy_id: format!("ln_{id}"),
            cost_cents: cents,
            done: false,
        },
        text: text.into(),
        spans: vec![],
    }
}

fn apply(doc: &mut PlanDocument, changed: Vec<PlanBlockEdit>) {
    let revision = doc.revision().unwrap();
    doc.apply_from(&revision, &order(doc), &changed).unwrap();
}

fn wire(update: &PlanUpdate) -> PlanUpdate {
    PlanUpdate::from_json(&update.to_json().unwrap()).unwrap()
}

fn merge_both(seed: &CanonicalPlanSeed, branches: &[&PlanDocument]) -> PlanDocument {
    let from = seed.open_editor().unwrap().version();
    let updates: Vec<_> = branches
        .iter()
        .map(|branch| wire(&branch.updates_since(&from).unwrap()))
        .collect();
    let mut forward = seed.open_editor().unwrap();
    let mut reverse = seed.open_editor().unwrap();
    for update in &updates {
        forward.import(update).unwrap();
    }
    for update in updates.iter().rev() {
        reverse.import(update).unwrap();
    }
    assert_eq!(forward.blocks().unwrap(), reverse.blocks().unwrap());
    assert_eq!(
        forward.semantic_hash().unwrap(),
        reverse.semantic_hash().unwrap()
    );
    assert_eq!(forward.view().unwrap(), reverse.view().unwrap());
    forward
}

fn assert_unchanged(doc: &PlanDocument, revision: &PlanRevision, blocks: &[PlanBlock]) {
    assert_eq!(&doc.revision().unwrap(), revision);
    assert_eq!(doc.blocks().unwrap(), blocks);
}

#[test]
fn projection_preserves_exact_raw_text_typed_fields_and_duplicate_long_legacy_ids() {
    let doc = seed().open_editor().unwrap();
    let blocks = doc.blocks().unwrap();
    assert_eq!(doc.view().unwrap().to_plan_doc(), original());
    assert_eq!(blocks[0].plain_text(), original().title);
    assert_eq!(blocks[1].plain_text(), original().summary);
    assert_eq!(blocks[3].runs, vec![Run::plain(RAW)]);
    let PlanBlockContent::Line {
        legacy_id: first,
        cost_cents,
        done,
    } = &blocks[3].content
    else {
        panic!("typed line")
    };
    let PlanBlockContent::Line {
        legacy_id: second, ..
    } = &blocks[4].content
    else {
        panic!("typed line")
    };
    assert_eq!(first, second);
    assert!(first.len() > crate::MAX_BLOCK_ID_BYTES);
    assert_ne!(blocks[3].id, blocks[4].id);
    assert_eq!((*cost_cents, *done), (100, false));
    assert!(matches!(
        blocks[0].content,
        PlanBlockContent::Title {
            budget_cents: Some(1_000)
        }
    ));
}

#[test]
fn style_done_and_money_edits_retain_raw_bytes_and_untouched_origin_negatives() {
    let mut legacy = original();
    legacy.budget_cents = Some(i64::MIN);
    legacy.sections[0].lines[0].cost_cents = -31;
    let seed = seed_with(legacy.clone());
    let mut doc = seed.open_editor().unwrap();
    let mut changed = edit(&line_block(&doc));
    changed.spans = vec![span(RAW, "b"), span(RAW, "a")];
    if let PlanBlockContent::Line { done, .. } = &mut changed.content {
        *done = true;
    }
    apply(&mut doc, vec![changed]);
    assert_eq!(line_block(&doc).plain_text().as_bytes(), RAW.as_bytes());
    assert!(line_block(&doc)
        .runs
        .iter()
        .all(|run| run.attrs.bold && run.attrs.link.is_some()));
    assert_eq!(doc.view().unwrap().budget_cents, Some(i64::MIN));
    assert_eq!(
        doc.view().unwrap().sections[0].lines[0].line.cost_cents,
        -31
    );
    assert_eq!(
        doc.view().unwrap().sections[0].lines[0].line.id,
        legacy.sections[0].lines[0].id
    );
    let mut changed = edit(&line_block(&doc));
    if let PlanBlockContent::Line { cost_cents, .. } = &mut changed.content {
        *cost_cents = 777;
    }
    apply(&mut doc, vec![changed]);
    assert_eq!(line_block(&doc).plain_text(), RAW);
    assert!(line_block(&doc)
        .runs
        .iter()
        .all(|run| run.attrs.bold && run.attrs.link.is_some()));
    let mut receiver = seed.open_editor().unwrap();
    receiver.import(&wire(&doc.full_update())).unwrap();
    assert_eq!(receiver.blocks().unwrap(), doc.blocks().unwrap());
}

#[test]
fn text_money_done_budget_and_marks_merge_in_both_orders_from_one_observed_revision() {
    let seed = seed();
    let mut branches: Vec<_> = (0..5).map(|_| seed.open_editor().unwrap()).collect();
    for (index, doc) in branches.iter_mut().enumerate() {
        let mut changed = edit(&line_block(doc));
        match index {
            0 => changed.text = format!("New {RAW}"),
            1 => {
                if let PlanBlockContent::Line { cost_cents, .. } = &mut changed.content {
                    *cost_cents = 777;
                }
            }
            2 => {
                if let PlanBlockContent::Line { done, .. } = &mut changed.content {
                    *done = true;
                }
            }
            3 => {
                changed = edit(&doc.blocks().unwrap()[0]);
                changed.content = PlanBlockContent::Title {
                    budget_cents: Some(888),
                };
            }
            4 => {
                // Keep the mark away from the concurrent insertion boundary: a
                // same-position insert and mark may order either way by peer ID.
                let mut bold = span(RAW, "b");
                bold.start = 1;
                changed.spans = vec![bold];
            }
            _ => unreachable!(),
        }
        apply(doc, vec![changed]);
    }
    let merged = merge_both(&seed, &branches.iter().collect::<Vec<_>>());
    let value = &merged.view().unwrap().sections[0].lines[0].line;
    assert_eq!(
        (&value.text, value.cost_cents, value.done),
        (&format!("New {RAW}"), 777, true)
    );
    assert_eq!(merged.view().unwrap().budget_cents, Some(888));
    let marked: String = line_block(&merged)
        .runs
        .iter()
        .filter(|run| run.attrs.bold)
        .map(|run| run.text.as_str())
        .collect();
    assert_eq!(marked, &RAW[1..]);
}

#[test]
fn an_older_observed_edit_preserves_unseen_text_marks_and_new_blocks() {
    let seed = seed();
    let mut current = seed.open_editor().unwrap();
    let revision = current.revision().unwrap();
    let mut local = edit(&line_block(&current));
    if let PlanBlockContent::Line { cost_cents, .. } = &mut local.content {
        *cost_cents = 999;
    }
    let mut remote = seed.open_editor().unwrap();
    let mut changed = edit(&line_block(&remote));
    changed.text = format!("Remote {RAW}");
    changed.spans = vec![span(&changed.text, "i")];
    let mut remote_order = order(&remote);
    remote_order.push("unseen-prose".into());
    remote
        .apply_from(
            &remote.revision().unwrap(),
            &remote_order,
            &[
                changed,
                prose("unseen-prose", Kind::Paragraph, "Remote content"),
            ],
        )
        .unwrap();
    current.import(&wire(&remote.full_update())).unwrap();
    current
        .apply_from(&revision, &order(&current.at(&revision).unwrap()), &[local])
        .unwrap();
    let block = line_block(&current);
    assert_eq!(block.plain_text(), format!("Remote {RAW}"));
    assert!(block.runs.iter().all(|run| run.attrs.italic));
    assert!(matches!(
        block.content,
        PlanBlockContent::Line {
            cost_cents: 999,
            ..
        }
    ));
    assert!(current
        .blocks()
        .unwrap()
        .iter()
        .any(|block| block.id == "unseen-prose" && block.plain_text() == "Remote content"));
}

#[test]
fn all_supported_prose_kinds_remain_typed_and_only_derived_plan_doc_omits_them() {
    let mut doc = seed().open_editor().unwrap();
    let kinds = vec![
        Kind::Paragraph,
        Kind::Heading {
            level: 2,
            setext: true,
        },
        Kind::Bullet { indent: 1 },
        Kind::Numbered {
            indent: 2,
            number: 3,
        },
        Kind::Task {
            indent: 1,
            checked: true,
        },
        Kind::Quote,
        Kind::Code {
            lang: "rust".into(),
        },
        Kind::Divider,
        Kind::Image {
            url: "https://example.test/image".into(),
            alt: "Alt".into(),
            title: "Title".into(),
        },
        Kind::Raw,
    ];
    let mut desired = order(&doc);
    let changed: Vec<_> = kinds
        .iter()
        .enumerate()
        .map(|(i, kind)| {
            let id = format!("prose-{i}");
            desired.push(id.clone());
            prose(&id, kind.clone(), RAW)
        })
        .collect();
    doc.apply_from(&doc.revision().unwrap(), &desired, &changed)
        .unwrap();
    assert_eq!(doc.view().unwrap().to_plan_doc(), original());
    for (block, kind) in doc.blocks().unwrap()[5..].iter().zip(&kinds) {
        assert_eq!(
            block.content,
            PlanBlockContent::Prose { kind: kind.clone() }
        );
        assert_eq!(block.plain_text(), RAW);
    }
    let mut change = edit(&doc.blocks().unwrap()[6]);
    change.content = PlanBlockContent::Prose { kind: Kind::Quote };
    apply(&mut doc, vec![change]);
    assert_eq!(
        doc.blocks().unwrap()[6].content,
        PlanBlockContent::Prose { kind: Kind::Quote }
    );
    let mut receiver = seed().open_editor().unwrap();
    receiver.import(&wire(&doc.full_update())).unwrap();
    assert_eq!(
        receiver.semantic_hash().unwrap(),
        doc.semantic_hash().unwrap()
    );
}

#[test]
fn concurrent_section_deletion_and_new_line_prose_preserve_a_visible_orphan_and_total() {
    let seed = seed();
    let mut removed = seed.open_editor().unwrap();
    let keep = order(&removed)[..2].to_vec();
    removed
        .apply_from(&removed.revision().unwrap(), &keep, &[])
        .unwrap();
    let mut inserted = seed.open_editor().unwrap();
    let mut desired = order(&inserted);
    desired.extend(["concurrent-prose".into(), "concurrent-line".into()]);
    inserted
        .apply_from(
            &inserted.revision().unwrap(),
            &desired,
            &[
                prose(
                    "concurrent-prose",
                    Kind::Task {
                        indent: 0,
                        checked: false,
                    },
                    "Remember",
                ),
                new_line("concurrent-line", RAW, 55),
            ],
        )
        .unwrap();
    let merged = merge_both(&seed, &[&removed, &inserted]);
    let view = merged.view().unwrap();
    assert_eq!(view.sections.len(), 1);
    assert!(view.sections[0].recovered && view.sections[0].block_id.is_none());
    assert_eq!(view.sections[0].lines.len(), 1);
    assert_eq!(view.sections[0].lines[0].line.text, RAW);
    assert_eq!(view.total, TotalCents::Cents(55));
    assert!(merged
        .blocks()
        .unwrap()
        .iter()
        .any(|block| block.id == "concurrent-prose"));
}

#[test]
fn observed_deletion_does_not_delete_unseen_blocks_already_in_current_document() {
    let seed = seed();
    let mut current = seed.open_editor().unwrap();
    let revision = current.revision().unwrap();
    let mut remote = seed.open_editor().unwrap();
    let mut remote_order = order(&remote);
    remote_order.push("new-remote-line".into());
    remote
        .apply_from(
            &remote.revision().unwrap(),
            &remote_order,
            &[new_line("new-remote-line", "Unseen", 42)],
        )
        .unwrap();
    current.import(&wire(&remote.full_update())).unwrap();
    let only_anchors = order(&current.at(&revision).unwrap())[..2].to_vec();
    current.apply_from(&revision, &only_anchors, &[]).unwrap();
    assert_eq!(current.blocks().unwrap().len(), 3);
    assert_eq!(current.view().unwrap().total, TotalCents::Cents(42));
    assert!(current.view().unwrap().sections[0].recovered);
}

#[test]
fn overflow_is_visible_and_does_not_refuse_a_legitimate_rich_text_branch() {
    let mut legacy = original();
    legacy.sections[0].lines[0].cost_cents = i64::MAX;
    let mut doc = seed_with(legacy).open_editor().unwrap();
    let mut changed = edit(&line_block(&doc));
    changed.spans = vec![span(RAW, "c")];
    apply(&mut doc, vec![changed]);
    assert_eq!(doc.view().unwrap().total, TotalCents::Overflow);
    assert_eq!(line_block(&doc).plain_text(), RAW);
    assert!(line_block(&doc).runs.iter().all(|run| run.attrs.code));
    assert!(matches!(
        line_block(&doc).content,
        PlanBlockContent::Line {
            cost_cents: i64::MAX,
            ..
        }
    ));
}

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

#[test]
fn real_loro_mark_and_unmark_packets_obey_grammar_and_raw_mutations_refuse() {
    let seed = seed();
    let mut author = seed.open_editor().unwrap();
    let from = author.version();
    let mut changed = edit(&line_block(&author));
    changed.spans = ["b", "i", "s", "c", "hb", "a"]
        .into_iter()
        .map(|key| span(RAW, key))
        .collect();
    apply(&mut author, vec![changed]);
    let marked = wire(&author.updates_since(&from).unwrap());
    let mut receiver = seed.open_editor().unwrap();
    receiver.import(&marked).unwrap();
    let middle = author.version();
    let mut changed = edit(&line_block(&author));
    changed.spans.clear();
    apply(&mut author, vec![changed]);
    let unmarked = wire(&author.updates_since(&middle).unwrap());
    let nulls = unmarked
        .0
        .changes
        .iter()
        .flat_map(|change| &change.ops)
        .filter(|op| {
            matches!(
                op.content,
                JsonOpContent::Text(JsonTextOp::Mark {
                    style_value: LoroValue::Null,
                    ..
                })
            )
        })
        .count();
    assert_eq!(nulls, 6);
    receiver.import(&unmarked).unwrap();
    assert_eq!(line_block(&receiver).runs, vec![Run::plain(RAW)]);
    for mutation in 0..5 {
        let mut receiver = seed.open_editor().unwrap();
        let revision = receiver.revision().unwrap();
        let before = receiver.blocks().unwrap();
        let mut malformed = marked.clone();
        let op = malformed.0.changes.iter_mut().flat_map(|change| &mut change.ops)
            .find(|op| matches!(&op.content, JsonOpContent::Text(JsonTextOp::Mark { style_key, .. }) if style_key == "b")).unwrap();
        if let JsonOpContent::Text(JsonTextOp::Mark {
            style_key,
            style_value,
            info,
            start,
            end,
        }) = &mut op.content
        {
            match mutation {
                0 => *style_key = "hidden-unknown".into(),
                1 => *style_value = LoroValue::I64(1),
                2 => *style_value = LoroValue::String("wrong".into()),
                3 => *info ^= 2,
                4 => *start = *end,
                _ => unreachable!(),
            }
        }
        assert!(receiver.import(&malformed).is_err());
        assert_unchanged(&receiver, &revision, &before);
    }
}

#[test]
fn public_loro_atom_slices_preserve_rich_text_pairs_and_unpaired_anchors_refuse() {
    let seed = seed();
    let mut author = seed.open_editor().unwrap();
    let from = author.version();
    let mut changed = edit(&line_block(&author));
    changed.spans = vec![span(RAW, "b")];
    apply(&mut author, vec![changed]);
    let full = author.updates_since(&from).unwrap();
    let mut slices = Vec::new();
    for change in &full.0.changes {
        for counter in change.id.counter..change.id.counter + change.op_len() as i32 {
            slices.extend(author.page.doc.export_json_in_id_span(IdSpan::new(
                change.id.peer,
                counter,
                counter + 1,
            )));
        }
    }
    let sliced = PlanUpdate(JsonSchema {
        changes: slices,
        ..full.0.clone()
    });
    assert_eq!(
        codec::fingerprint(&full).unwrap(),
        codec::fingerprint(&sliced).unwrap()
    );
    let mut receiver = seed.open_editor().unwrap();
    receiver.import(&wire(&sliced)).unwrap();
    assert_eq!(receiver.blocks().unwrap(), author.blocks().unwrap());
    let mut malformed = full;
    for op in malformed
        .0
        .changes
        .iter_mut()
        .flat_map(|change| &mut change.ops)
    {
        if matches!(op.content, JsonOpContent::Text(JsonTextOp::MarkEnd)) {
            op.content = JsonOpContent::Text(JsonTextOp::Insert {
                pos: 0,
                text: "x".into(),
            });
            break;
        }
    }
    let mut receiver = seed.open_editor().unwrap();
    let revision = receiver.revision().unwrap();
    let before = receiver.blocks().unwrap();
    assert!(receiver.import(&malformed).is_err());
    assert_unchanged(&receiver, &revision, &before);
}

#[test]
fn typed_and_raw_new_negatives_refuse_while_authenticated_origin_values_survive() {
    let seed = seed();
    let mut doc = seed.open_editor().unwrap();
    let revision = doc.revision().unwrap();
    let before = doc.blocks().unwrap();
    let mut title = edit(&before[0]);
    title.content = PlanBlockContent::Title {
        budget_cents: Some(-1),
    };
    let mut line = edit(&line_block(&doc));
    if let PlanBlockContent::Line { cost_cents, .. } = &mut line.content {
        *cost_cents = -1;
    }
    for changed in [title, line] {
        assert!(doc.apply_from(&revision, &order(&doc), &[changed]).is_err());
        assert_unchanged(&doc, &revision, &before);
    }
    for field in ["budget_cents", "cost_cents"] {
        let author = doc.fork().unwrap();
        let from = author.version();
        let nodes = read_nodes(&author.page, author.seed.root()).unwrap();
        let node = if field == "budget_cents" {
            &nodes[0]
        } else {
            nodes
                .iter()
                .find(|node| node.kind == BlockKind::Line)
                .unwrap()
        };
        node.map.insert(field, -1_i64).unwrap();
        author.page.doc.commit();
        assert!(doc.import(&author.updates_since(&from).unwrap()).is_err());
        assert_unchanged(&doc, &revision, &before);
    }
}

#[test]
fn approvals_keep_exact_text_cents_comparison_through_style_done_and_prose_edits() {
    let mut doc = seed().open_editor().unwrap();
    let line = line_block(&doc);
    let original_hash = doc.view().unwrap().sections[0].lines[0]
        .content_hash
        .clone();
    let mut changed = edit(&line);
    changed.spans = vec![span(RAW, "b")];
    if let PlanBlockContent::Line { done, .. } = &mut changed.content {
        *done = true;
    }
    let mut desired = order(&doc);
    desired.push("approval-prose".into());
    doc.apply_from(
        &doc.revision().unwrap(),
        &desired,
        &[changed, prose("approval-prose", Kind::Quote, "Note")],
    )
    .unwrap();
    assert_eq!(
        doc.view().unwrap().sections[0].lines[0].content_hash,
        original_hash
    );
    doc.apply(PlanCommand::ChangeLine {
        block: line.id.clone(),
        expected: ExpectedLine::Hash(original_hash.clone()),
        patch: LinePatch {
            cost_cents: Some(200),
            ..Default::default()
        },
    })
    .unwrap();
    let revision = doc.revision().unwrap();
    let before = doc.blocks().unwrap();
    assert_eq!(
        doc.apply(PlanCommand::ChangeLine {
            block: line.id,
            expected: ExpectedLine::Hash(original_hash),
            patch: LinePatch {
                text: Some("Refused approval".into()),
                ..Default::default()
            },
        })
        .unwrap_err(),
        PlanError::StaleLine
    );
    assert_unchanged(&doc, &revision, &before);
}

#[test]
fn empty_edits_and_equivalent_span_segmentation_do_not_create_operations_or_drop_metadata() {
    let mut doc = seed().open_editor().unwrap();
    let mut changed = edit(&line_block(&doc));
    changed.spans = vec![span(RAW, "b")];
    apply(&mut doc, vec![changed]);
    let revision = doc.revision().unwrap();
    let before = doc.blocks().unwrap();
    assert_eq!(
        doc.apply_from(&revision, &order(&doc), &[]).unwrap(),
        CommandResult::Unchanged
    );
    let mut same = edit(&line_block(&doc));
    same.spans = vec![
        Span {
            start: 0,
            end: 1,
            key: "b".into(),
            value: String::new(),
        },
        Span {
            start: 1,
            end: RAW.encode_utf16().count() as u32,
            key: "b".into(),
            value: String::new(),
        },
    ];
    assert_eq!(
        doc.apply_from(&revision, &order(&doc), &[same]).unwrap(),
        CommandResult::Unchanged
    );
    assert_unchanged(&doc, &revision, &before);
}

#[test]
fn oversized_typed_edits_refuse_without_mutating_the_prior_revision() {
    let mut doc = seed().open_editor().unwrap();
    let revision = doc.revision().unwrap();
    let before = doc.blocks().unwrap();
    let mut changed = edit(&line_block(&doc));
    changed.text = "x".repeat(MAX_PLAN_PACKET_BYTES + 1);
    assert!(matches!(
        doc.apply_from(&revision, &order(&doc), &[changed]),
        Err(PlanError::Limit(_))
    ));
    assert_unchanged(&doc, &revision, &before);
}

#[test]
fn plan_revision_token_roundtrip_retains_exact_history() {
    let mut doc = seed().open_editor().unwrap();
    let before = doc.revision().unwrap();
    let mut changed = edit(&line_block(&doc));
    changed.text = "token 😀\r\n\0history".into();
    changed.spans = vec![span(&changed.text, "b")];
    doc.apply_from(&before, &order(&doc), &[changed]).unwrap();
    let current = doc.revision().unwrap();
    let history = doc.full_update().to_json().unwrap();
    for revision in [before, current] {
        let encoded = revision.encode().unwrap();
        let verified = doc.verify_revision(&encoded).unwrap();
        assert_eq!(verified, revision);
        assert_eq!(verified.encode().unwrap(), encoded);
        assert_eq!(doc.at(&verified).unwrap().revision().unwrap(), revision);
    }
    assert_eq!(doc.full_update().to_json().unwrap(), history);
}

#[test]
fn plan_revision_token_rejects_unknown_fields_and_every_forged_witness_atomically() {
    let doc = seed().open_editor().unwrap();
    let revision = doc.revision().unwrap();
    let history = doc.full_update().to_json().unwrap();
    let encoded = revision.encode().unwrap();
    let value: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
    let mut invalid = Vec::new();
    for field in ["schema", "root", "frontiers_hex", "operations_hex"] {
        let mut missing = value.clone();
        missing.as_object_mut().unwrap().remove(field);
        invalid.push(serde_json::to_vec(&missing).unwrap());
        let mut forged = value.clone();
        forged[field] = serde_json::Value::String("forged".into());
        invalid.push(serde_json::to_vec(&forged).unwrap());
    }
    let mut digest = value.clone();
    digest["operations_hex"] = serde_json::Value::String("00".repeat(32));
    invalid.push(serde_json::to_vec(&digest).unwrap());
    let mut unknown = value.clone();
    unknown["version"] = serde_json::Value::String("caller-chosen-vv".into());
    invalid.push(serde_json::to_vec(&unknown).unwrap());
    let duplicate = String::from_utf8(encoded).unwrap();
    invalid.push(format!("{{\"schema\":\"duplicate\",{}", &duplicate[1..]).into_bytes());
    invalid.push(vec![b' '; PlanRevision::MAX_ENCODED_BYTES + 1]);
    for packet in invalid {
        assert!(doc.verify_revision(&packet).is_err());
        assert_eq!(doc.revision().unwrap(), revision);
        assert_eq!(doc.full_update().to_json().unwrap(), history);
    }
}

#[test]
fn plan_revision_token_refuses_cross_history_and_retries_after_dependencies() {
    let mut source = seed().open_editor().unwrap();
    let mut receiver = source.fork().unwrap();
    let base = source.version();
    let mut changed = edit(&line_block(&source));
    changed.text = "unknown causal operations".into();
    apply(&mut source, vec![changed]);
    let token = source.revision().unwrap().encode().unwrap();
    let exact = receiver.full_update().to_json().unwrap();
    assert!(receiver.verify_revision(&token).is_err());
    assert_eq!(receiver.full_update().to_json().unwrap(), exact);
    let other_seed = CanonicalPlanSeed::replay(&[LegacyVersion {
        event_id: "another-authenticated-creation".into(),
        document: original(),
    }])
    .unwrap();
    let other = other_seed.open_editor().unwrap();
    assert!(other.verify_revision(&token).is_err());
    receiver
        .import(&source.updates_since(&base).unwrap())
        .unwrap();
    assert_eq!(
        receiver.verify_revision(&token).unwrap(),
        source.revision().unwrap()
    );
    assert_eq!(receiver.blocks().unwrap(), source.blocks().unwrap());
}
