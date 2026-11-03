use super::*;

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
