use super::*;

fn original() -> PlanDoc {
    PlanDoc {
        title: "Trip".into(),
        summary: "One\nline".into(),
        budget_cents: Some(500),
        sections: vec![PlanSection {
            title: "Hotel".into(),
            lines: vec![PlanLine {
                id: "existing-line".into(),
                text: "Stay".into(),
                cost_cents: 100,
                done: false,
            }],
        }],
    }
}

fn history(plan: PlanDoc) -> Vec<LegacyVersion> {
    vec![LegacyVersion {
        event_id: "authenticated-creation".into(),
        document: plan,
    }]
}

fn seed() -> CanonicalPlanSeed {
    CanonicalPlanSeed::replay(&history(original())).unwrap()
}

fn line_block(doc: &PlanDocument) -> String {
    doc.view().unwrap().sections[0].lines[0].block_id.clone()
}

fn section_block(doc: &PlanDocument) -> String {
    doc.view().unwrap().sections[0].block_id.clone().unwrap()
}

fn change(doc: &mut PlanDocument, patch: LinePatch) {
    let block = line_block(doc);
    doc.apply(PlanCommand::ChangeLine {
        block,
        expected: ExpectedLine::AnyCurrent,
        patch,
    })
    .unwrap();
}

fn merge_in_both_orders(seed: &CanonicalPlanSeed, updates: &[PlanUpdate]) -> (PlanView, PlanView) {
    let mut forward = seed.open_editor().unwrap();
    for update in updates {
        forward.import(update).unwrap();
    }
    let mut reverse = seed.open_editor().unwrap();
    for update in updates.iter().rev() {
        reverse.import(update).unwrap();
    }
    assert_eq!(forward.view().unwrap(), reverse.view().unwrap());
    assert_eq!(forward.version(), reverse.version());
    (forward.view().unwrap(), reverse.view().unwrap())
}

fn assert_refused_without_change(doc: &mut PlanDocument, update: &PlanUpdate, expected: PlanError) {
    let view = doc.view().unwrap();
    let version = doc.version();
    let json = doc.full_update().to_json().unwrap();
    assert_eq!(doc.import(update), Err(expected));
    assert_eq!(doc.view().unwrap(), view);
    assert_eq!(doc.version(), version);
    assert_eq!(doc.full_update().to_json().unwrap(), json);
}

#[test]
fn legacy_ids_extreme_cents_and_exact_utf8_are_not_normalized() {
    let mut plan = original();
    plan.title = "  🚃\r\n\u{2028} ".into();
    plan.summary = "\0two\n\r\u{2028}😀".into();
    plan.budget_cents = Some(i64::MIN);
    plan.sections[0].title = String::new();
    let duplicate_id = "  legacy-id ".repeat(600);
    plan.sections[0].lines = vec![
        PlanLine {
            id: duplicate_id.clone(),
            text: " \r\n😀\u{2028}\0 ".into(),
            cost_cents: i64::MIN,
            done: true,
        },
        PlanLine {
            id: duplicate_id,
            text: String::new(),
            cost_cents: i64::MAX,
            done: false,
        },
        PlanLine {
            id: String::new(),
            text: "empty original ID".into(),
            cost_cents: 0,
            done: false,
        },
    ];
    plan.sections.push(PlanSection {
        title: "Empty".into(),
        lines: Vec::new(),
    });
    let seed = CanonicalPlanSeed::replay(&history(plan.clone())).unwrap();
    let editor = seed.open_editor().unwrap();
    let view = editor.view().unwrap();
    assert_eq!(view.to_plan_doc(), plan);
    assert_eq!(view.total, TotalCents::Cents(-1));
    let ids: Vec<_> = view.sections[0].lines.iter().map(|l| &l.block_id).collect();
    assert_eq!(ids.iter().collect::<HashSet<_>>().len(), 3);
    assert!(ids.iter().all(|id| id.len() <= 128));
    assert_eq!(seed.legacy_at(1).unwrap().document, plan);
    assert_eq!(
        CanonicalPlanSeed::replay(&history(plan))
            .unwrap()
            .open_editor()
            .unwrap()
            .full_update()
            .to_json()
            .unwrap(),
        editor.full_update().to_json().unwrap()
    );
}

#[test]
fn legacy_prefixes_share_the_exact_tape_and_two_first_edits_survive() {
    let v1 = history(original());
    let mut v2 = original();
    v2.title = "Trip updated".into();
    v2.budget_cents = None;
    v2.sections[0].lines[0].cost_cents = 200;
    let versions = [
        v1.clone(),
        vec![LegacyVersion {
            event_id: "authenticated-second".into(),
            document: v2.clone(),
        }],
    ]
    .concat();
    let old = CanonicalPlanSeed::replay(&v1).unwrap();
    let current = CanonicalPlanSeed::replay(&versions).unwrap();
    assert_eq!(old.legacy_frontiers(1), current.legacy_frontiers(1));
    assert_eq!(old.legacy_at(1), current.legacy_at(1));
    let mut a = old.open_editor().unwrap();
    let mut b = current.open_editor().unwrap();
    assert_eq!(b.view().unwrap().to_plan_doc(), v2);
    change(
        &mut a,
        LinePatch {
            text: Some("Reserved stay".into()),
            ..Default::default()
        },
    );
    change(
        &mut b,
        LinePatch {
            done: Some(true),
            ..Default::default()
        },
    );
    let a_first = a.full_update();
    let b_first = b.full_update();
    a.admit_legacy_prefix(&current).unwrap();
    a.import(&b_first).unwrap();
    b.import(&a_first).unwrap();
    assert_eq!(a.view().unwrap(), b.view().unwrap());
    let (merged, _) = merge_in_both_orders(&current, &[a_first, b_first]);
    let line = &merged.sections[0].lines[0].line;
    assert_eq!(line.id, "existing-line");
    assert_eq!(line.text, "Reserved stay");
    assert_eq!(line.cost_cents, 200);
    assert!(line.done);
    assert_eq!(merged.title, "Trip updated");
    assert_eq!(merged.budget_cents, None);
    assert_eq!(merged.total, TotalCents::Cents(200));
    assert_eq!(current.legacy_at(1).unwrap().document, original());
    assert_eq!(current.legacy_at(2).unwrap().document, v2);
}

#[test]
fn text_cost_and_done_merge_independently_even_when_callers_repeat_old_fields() {
    let seed = seed();
    let mut text = seed.open_editor().unwrap();
    let mut cost = seed.open_editor().unwrap();
    let mut done = seed.open_editor().unwrap();
    change(
        &mut text,
        LinePatch {
            text: Some("Longer stay".into()),
            cost_cents: Some(100),
            done: Some(false),
        },
    );
    change(
        &mut cost,
        LinePatch {
            text: Some("Stay".into()),
            cost_cents: Some(777),
            done: Some(false),
        },
    );
    change(
        &mut done,
        LinePatch {
            text: Some("Stay".into()),
            cost_cents: Some(100),
            done: Some(true),
        },
    );
    let updates = [text.full_update(), cost.full_update(), done.full_update()];
    let peers: Vec<_> = updates
        .iter()
        .flat_map(|u| {
            u.0.changes
                .iter()
                .filter(|c| c.id.peer != MIGRATION_PEER)
                .map(|c| c.id.peer)
        })
        .collect();
    assert_eq!(peers.iter().collect::<HashSet<_>>().len(), 3);
    let (view, _) = merge_in_both_orders(&seed, &updates);
    assert_eq!(view.sections[0].lines.len(), 1);
    assert_eq!(
        view.sections[0].lines[0].line,
        PlanLine {
            id: "existing-line".into(),
            text: "Longer stay".into(),
            cost_cents: 777,
            done: true
        }
    );
    assert_eq!(view.total, TotalCents::Cents(777));
    // A real subsequent exact import is an idempotent operation, not another line.
    text.import(&cost.full_update()).unwrap();
    text.import(&done.full_update()).unwrap();
    let before = text.version();
    text.import(&done.full_update()).unwrap();
    assert_eq!(text.version(), before);
    assert_eq!(text.view().unwrap(), view);
}

#[test]
fn deleting_the_last_section_retains_an_unseen_insert_in_a_visible_recovery_group() {
    let seed = seed();
    let mut delete = seed.open_editor().unwrap();
    let mut insert = seed.open_editor().unwrap();
    delete
        .apply(PlanCommand::RemoveSection {
            block: section_block(&delete),
        })
        .unwrap();
    assert!(delete.view().unwrap().sections.is_empty());
    let CommandResult::Created(inserted) = insert
        .apply(PlanCommand::InsertLine {
            section: section_block(&insert),
            text: "A concurrent train".into(),
            cost_cents: 333,
        })
        .unwrap()
    else {
        panic!("line not inserted");
    };
    let (view, _) = merge_in_both_orders(&seed, &[delete.full_update(), insert.full_update()]);
    assert_eq!(view.sections.len(), 1);
    let recovered = &view.sections[0];
    assert!(recovered.recovered);
    assert_eq!(recovered.block_id, None);
    assert_eq!(recovered.lines.len(), 1);
    assert_eq!(recovered.lines[0].block_id, inserted);
    assert_eq!(recovered.lines[0].line.text, "A concurrent train");
    assert_eq!(recovered.lines[0].line.cost_cents, 333);
    assert_eq!(view.total, TotalCents::Cents(333));
    delete.import(&insert.full_update()).unwrap();
    insert.import(&delete.full_update()).unwrap();
    assert_eq!(delete.view().unwrap(), view);
    assert_eq!(insert.view().unwrap(), view);
}

#[test]
fn a_legitimate_merged_total_can_overflow_without_losing_either_branch() {
    let mut plan = original();
    plan.sections[0].lines[0].cost_cents = 0;
    plan.sections[0].lines.push(PlanLine {
        id: "second".into(),
        text: "Second".into(),
        cost_cents: 0,
        done: false,
    });
    let seed = CanonicalPlanSeed::replay(&history(plan)).unwrap();
    let mut a = seed.open_editor().unwrap();
    let mut b = seed.open_editor().unwrap();
    let lines = a.view().unwrap().sections[0].lines.clone();
    a.apply(PlanCommand::ChangeLine {
        block: lines[0].block_id.clone(),
        expected: ExpectedLine::AnyCurrent,
        patch: LinePatch {
            cost_cents: Some(i64::MAX),
            ..Default::default()
        },
    })
    .unwrap();
    b.apply(PlanCommand::ChangeLine {
        block: lines[1].block_id.clone(),
        expected: ExpectedLine::AnyCurrent,
        patch: LinePatch {
            cost_cents: Some(1),
            ..Default::default()
        },
    })
    .unwrap();
    assert_eq!(a.view().unwrap().total, TotalCents::Cents(i64::MAX));
    assert_eq!(b.view().unwrap().total, TotalCents::Cents(1));
    let (view, _) = merge_in_both_orders(&seed, &[a.full_update(), b.full_update()]);
    assert_eq!(view.total, TotalCents::Overflow);
    assert_eq!(view.sections[0].lines.len(), 2);
    assert_eq!(view.sections[0].lines[0].line.cost_cents, i64::MAX);
    assert_eq!(view.sections[0].lines[1].line.cost_cents, 1);
    // Exact cancellation is representable even when an intermediate i64 sum is not.
    let mut cancellation = view.to_plan_doc();
    cancellation.sections[0].lines.push(PlanLine {
        id: "legacy-negative".into(),
        text: "Credit".into(),
        cost_cents: -1,
        done: false,
    });
    assert_eq!(
        CanonicalPlanSeed::replay(&history(cancellation))
            .unwrap()
            .open_editor()
            .unwrap()
            .view()
            .unwrap()
            .total,
        TotalCents::Cents(i64::MAX)
    );
}

mod integrity;
