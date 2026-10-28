//! Existing typed editor grammar regressions.
use super::*;

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
    apply(&mut author, &[changed]);
    let marked = wire(&author.updates_since(&from).unwrap());
    let mut receiver = seed.open_editor().unwrap();
    receiver.import(&marked).unwrap();
    let middle = author.version();
    let mut changed = edit(&line_block(&author));
    changed.spans.clear();
    apply(&mut author, &[changed]);
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
    apply(&mut author, &[changed]);
    let full = author.updates_since(&from).unwrap();
    let mut slices = Vec::new();
    for change in &full.0.changes {
        for counter in
            change.id.counter..change.id.counter + i32::try_from(change.op_len()).unwrap()
        {
            slices.extend(author.page.doc.export_json_in_id_span(IdSpan::new(
                change.id.peer,
                counter,
                counter + 1,
            )));
        }
    }
    let packet = PlanUpdate(JsonSchema {
        changes: slices,
        ..full.0.clone()
    });
    assert_eq!(
        codec::fingerprint(&full).unwrap(),
        codec::fingerprint(&packet).unwrap()
    );
    let mut receiver = seed.open_editor().unwrap();
    receiver.import(&wire(&packet)).unwrap();
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
