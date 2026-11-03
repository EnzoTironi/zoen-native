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

mod history_validation;
mod rich_packets;
