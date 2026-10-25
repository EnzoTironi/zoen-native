use super::*;
use crate::pages::session::{DocumentEdit, DocumentView};
use crate::{CoreError, PlanDto};
use documents::plan::{PlanBlockContent, PlanBlockEdit};

fn snapshot(fixture: &Fixture) -> (u32, usize, usize) {
    let version = fixture.engine.item(&fixture.item).unwrap().version;
    let outbox = fixture.engine.store.outbox().unwrap().len();
    let log = fixture
        .engine
        .logs
        .get(&fixture.space)
        .map(|log| log.events().len())
        .unwrap_or(0);
    (version, outbox, log)
}

fn line_id() -> String {
    original().sections[0].lines[0].id.clone()
}

fn open_private_draft(
    fixture: &mut Fixture,
    mutation_id: &str,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    fixture
        .engine
        .with_document(&fixture.item, ItemKind::Plan, |_, session| {
            let read = session.read(&fixture.engine)?;
            let DocumentView::Plan { blocks } = &read.view else {
                panic!("typed Plan required")
            };
            let line = blocks
                .iter()
                .find(|block| matches!(block.content, PlanBlockContent::Line { .. }))
                .unwrap();
            let edit = PlanBlockEdit {
                id: line.id.clone(),
                content: line.content.clone(),
                text: "Private draft bytes".into(),
                spans: vec![],
            };
            session.apply_from(
                &fixture.engine,
                mutation_id,
                &read.edit_context,
                DocumentEdit::Plan {
                    order: &blocks
                        .iter()
                        .map(|block| block.id.clone())
                        .collect::<Vec<_>>(),
                    changed: &[edit],
                },
            )?;
            Ok(())
        })
        .unwrap();

    Ok(())
}

fn assert_plan(got: &PlanDto, expect: &PlanDoc) {
    assert_eq!(got.title, expect.title);
    assert_eq!(got.summary, expect.summary);
    assert_eq!(got.budget_cents, expect.budget_cents);
    assert_eq!(got.sections.len(), expect.sections.len());
    for (section, expected) in got.sections.iter().zip(&expect.sections) {
        assert_eq!(section.title, expected.title);
        assert_eq!(section.lines.len(), expected.lines.len());
        for (line, expected_line) in section.lines.iter().zip(&expected.lines) {
            assert_eq!(line.id, expected_line.id);
            assert_eq!(line.text, expected_line.text);
            assert_eq!(line.cost_cents, expected_line.cost_cents);
            assert_eq!(line.done, expected_line.done);
        }
    }
}

#[test]
fn divergent_hash_on_each_writer_appends_nothing() {
    let mut fixture = Fixture::new().unwrap();
    let line = line_id();
    let before = snapshot(&fixture);
    let stale = "stale-binding";
    assert!(matches!(
        fixture
            .engine
            .edit_plan_line(&fixture.item, &line, "Remote text", 1, stale),
        Err(CoreError::Stale { .. })
    ));
    assert!(matches!(
        fixture.engine.toggle_plan_line(&fixture.item, &line, stale),
        Err(CoreError::Stale { .. })
    ));
    assert!(matches!(
        fixture
            .engine
            .add_plan_line(&fixture.item, 0, "Remote line", 1, stale),
        Err(CoreError::Stale { .. })
    ));
    assert!(matches!(
        fixture.engine.remove_plan_line(&fixture.item, &line, stale),
        Err(CoreError::Stale { .. })
    ));
    assert_eq!(snapshot(&fixture), before);
    assert_plan(
        &fixture.engine.item(&fixture.item).unwrap().plan.unwrap(),
        &original(),
    );
}

#[test]
fn pure_confirmation_keeps_the_binding_and_the_saved_edit_uses_it(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut fixture = Fixture::new().unwrap();
    let item = new_id("confirmed-binding").unwrap();
    let creation = fixture.queue(EventBody::ItemCreated {
        item: item.clone(),
        kind: ItemKind::Plan,
        content: ItemContent::Plan(original()),
        origin: "Signed offline creation".into(),
    });
    let pending = fixture.engine.plan_page(&item).unwrap();
    assert_eq!(pending.source, PlanPageReadSourceDto::LocalPendingLegacy);
    deliver(&mut fixture.engine, &creation, Ingest::Confirmed)?;
    let confirmed = fixture.engine.plan_page(&item).unwrap();
    assert_eq!(confirmed.source, PlanPageReadSourceDto::DocumentSession);
    assert_eq!(confirmed.content_hash, pending.content_hash);
    let saved = fixture
        .engine
        .edit_plan_line(&item, &line_id(), "One cent", 1, &pending.content_hash)
        .unwrap();
    assert_eq!(saved.item.plan.unwrap().sections[0].lines[0].cost_cents, 1);
    assert_ne!(
        fixture.engine.plan_page(&item).unwrap().content_hash,
        pending.content_hash
    );
    Ok(())
}

#[test]
fn semantic_field_changes_move_the_binding() -> std::result::Result<(), Box<dyn std::error::Error>>
{
    let mut fixture = Fixture::new().unwrap();
    let mut seen = vec![
        fixture
            .engine
            .plan_page(&fixture.item)
            .unwrap()
            .content_hash,
    ];
    let mut next = |fixture: &mut Fixture, doc: PlanDoc| {
        let body = fixture.version(ItemContent::Plan(doc));
        fixture.ordered(body).unwrap();
        let hash = fixture
            .engine
            .plan_page(&fixture.item)
            .unwrap()
            .content_hash;
        assert!(!seen.contains(&hash), "{hash}");
        seen.push(hash);
    };
    let mut title = original();
    title.title = "Other title".into();
    next(&mut fixture, title);
    let mut budget = original();
    budget.budget_cents = Some(2_000);
    next(&mut fixture, budget);
    let mut summary = original();
    summary.summary = "Other summary".into();
    next(&mut fixture, summary);
    let mut section = original();
    section.sections[0].title = "Travel".into();
    next(&mut fixture, section);
    let mut text = original();
    text.sections[0].lines[0].text = "One cent".into();
    next(&mut fixture, text);
    let mut cents = original();
    cents.sections[0].lines[0].cost_cents = 1;
    next(&mut fixture, cents);
    let mut done = original();
    done.sections[0].lines[0].done = true;
    next(&mut fixture, done);
    let mut order = original();
    order.sections[0].lines.swap(0, 1);
    next(&mut fixture, order);

    fixture
        .engine
        .with_document(&fixture.item, ItemKind::Plan, |_, session| {
            let read = session.read(&fixture.engine)?;
            let DocumentView::Plan { blocks } = &read.view else {
                panic!("typed Plan required")
            };
            let line = blocks
                .iter()
                .find(|block| matches!(block.content, PlanBlockContent::Line { .. }))
                .unwrap();
            let edit = PlanBlockEdit {
                id: line.id.clone(),
                content: line.content.clone(),
                text: "Marked prose stays in the binding".into(),
                spans: vec![],
            };
            session.apply_from(
                &fixture.engine,
                "observed-prose",
                &read.edit_context,
                DocumentEdit::Plan {
                    order: &blocks
                        .iter()
                        .map(|block| block.id.clone())
                        .collect::<Vec<_>>(),
                    changed: &[edit],
                },
            )?;
            Ok(())
        })
        .unwrap();
    let page = fixture.engine.plan_page(&fixture.item).unwrap();
    assert!(page.unsaved);
    assert!(page
        .blocks
        .iter()
        .any(|block| block.text == "Marked prose stays in the binding"));
    assert_eq!(&page.content_hash, seen.last().expect("plan binding"));

    Ok(())
}

#[test]
fn restore_with_a_stale_hash_appends_nothing_and_plain_restore_refuses_plans() {
    let mut fixture = Fixture::new().unwrap();
    let observed = fixture
        .engine
        .plan_page(&fixture.item)
        .unwrap()
        .content_hash;
    fixture
        .engine
        .edit_plan_line(&fixture.item, &line_id(), "Newer line", 4, &observed)
        .unwrap();
    let before = snapshot(&fixture);
    assert!(matches!(
        fixture
            .engine
            .restore_plan_version(&fixture.item, 1, &observed),
        Err(CoreError::Stale { .. })
    ));
    assert!(matches!(
        fixture
            .engine
            .restore_version(&fixture.item, 1, "plain restore"),
        Err(CoreError::Invalid { .. })
    ));
    assert_eq!(snapshot(&fixture), before);
    assert_eq!(
        fixture
            .engine
            .item(&fixture.item)
            .unwrap()
            .plan
            .unwrap()
            .sections[0]
            .lines[0]
            .text,
        "Newer line"
    );
}

#[test]
fn undo_after_an_intermediate_edit_appends_nothing() {
    let mut fixture = Fixture::new().unwrap();
    let line = line_id();
    let first_hash = fixture
        .engine
        .plan_page(&fixture.item)
        .unwrap()
        .content_hash;
    let first = fixture
        .engine
        .edit_plan_line(&fixture.item, &line, "First", 2, &first_hash)
        .unwrap();
    let second_hash = fixture
        .engine
        .plan_page(&fixture.item)
        .unwrap()
        .content_hash;
    fixture
        .engine
        .edit_plan_line(&fixture.item, &line, "Second", 3, &second_hash)
        .unwrap();
    let before = snapshot(&fixture);
    assert!(matches!(
        fixture.engine.undo(&first.undo),
        Err(CoreError::Stale { .. })
    ));
    assert_eq!(snapshot(&fixture), before);
    assert_eq!(
        fixture
            .engine
            .item(&fixture.item)
            .unwrap()
            .plan
            .unwrap()
            .sections[0]
            .lines[0]
            .text,
        "Second"
    );
}

#[test]
fn undo_without_an_intermediate_edit_restores_the_previous_line() {
    let mut fixture = Fixture::new().unwrap();
    let previous = original().sections[0].lines[0].text.clone();
    let observed = fixture
        .engine
        .plan_page(&fixture.item)
        .unwrap()
        .content_hash;
    let edited = fixture
        .engine
        .edit_plan_line(&fixture.item, &line_id(), "Temporary", 9, &observed)
        .unwrap();
    let undone = fixture.engine.undo(&edited.undo).unwrap();
    assert_eq!(undone.plan.unwrap().sections[0].lines[0].text, previous);
    assert_eq!(undone.version, edited.item.version + 1);
    assert!(undone.versions[0].is_undo);
}

#[test]
fn remote_event_between_read_and_write_is_refused() {
    let mut fixture = Fixture::new().unwrap();
    let member = add_identity(&mut fixture.engine, "Remote member");
    fixture
        .ordered(EventBody::MemberAdded {
            identity: member.clone(),
            role: Role::Member,
        })
        .unwrap();
    let observed = fixture
        .engine
        .plan_page(&fixture.item)
        .unwrap()
        .content_hash;
    let mut remote = original();
    remote.summary = "Remote summary".into();
    fixture
        .ordered_as(&member, fixture.version(ItemContent::Plan(remote)))
        .unwrap();
    let before = snapshot(&fixture);
    assert!(matches!(
        fixture.engine.edit_plan_line(
            &fixture.item,
            &line_id(),
            "Local after remote",
            1,
            &observed
        ),
        Err(CoreError::Stale { .. })
    ));
    assert_eq!(snapshot(&fixture), before);
    assert_eq!(
        fixture.engine.plan_page(&fixture.item).unwrap().blocks[1].text,
        "Remote summary"
    );
}

#[test]
fn unsaved_draft_refuses_a_plan_doc_change_on_every_writer(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut fixture = Fixture::new().unwrap();
    open_private_draft(&mut fixture, "observed-private-draft")?;
    let opened = fixture.engine.plan_page(&fixture.item).unwrap();
    assert!(opened.unsaved);
    assert!(opened
        .blocks
        .iter()
        .any(|block| block.text == "Private draft bytes"));
    let line = line_id();
    let edited = fixture
        .engine
        .edit_plan_line(
            &fixture.item,
            &line,
            "First writer",
            3,
            &opened.content_hash,
        )
        .unwrap();
    let bound = fixture.engine.plan_page(&fixture.item).unwrap();
    assert!(bound.unsaved, "an unsaved private draft stays present");
    assert!(bound
        .blocks
        .iter()
        .any(|block| block.text == "Private draft bytes"));
    let hash = bound.content_hash;
    fixture
        .engine
        .edit_plan_line(&fixture.item, &line, "Second writer", 4, &hash)
        .unwrap();
    let during = fixture.engine.plan_page(&fixture.item).unwrap();
    assert!(
        during.unsaved,
        "the private draft remains after the PlanDoc change"
    );
    assert!(
        during
            .blocks
            .iter()
            .any(|block| block.text == "Private draft bytes"),
        "the live draft is not replaced by the PlanDoc"
    );
    assert_eq!(
        fixture
            .engine
            .item(&fixture.item)
            .unwrap()
            .plan
            .unwrap()
            .sections[0]
            .lines[0]
            .text,
        "Second writer"
    );
    let before = snapshot(&fixture);
    assert!(
        matches!(
            fixture.engine.edit_plan_line(
                &fixture.item,
                &line,
                "Local after the other writer",
                5,
                &hash
            ),
            Err(CoreError::Stale { .. })
        ),
        "edit_plan_line"
    );
    assert!(
        matches!(
            fixture.engine.toggle_plan_line(&fixture.item, &line, &hash),
            Err(CoreError::Stale { .. })
        ),
        "toggle_plan_line"
    );
    assert!(
        matches!(
            fixture
                .engine
                .add_plan_line(&fixture.item, 0, "Local line", 1, &hash),
            Err(CoreError::Stale { .. })
        ),
        "add_plan_line"
    );
    assert!(
        matches!(
            fixture.engine.remove_plan_line(&fixture.item, &line, &hash),
            Err(CoreError::Stale { .. })
        ),
        "remove_plan_line"
    );
    assert!(
        matches!(
            fixture.engine.undo(&edited.undo),
            Err(CoreError::Stale { .. })
        ),
        "undo"
    );
    assert!(
        matches!(
            fixture.engine.restore_plan_version(&fixture.item, 1, &hash),
            Err(CoreError::Stale { .. })
        ),
        "restore_plan_version"
    );
    assert_eq!(snapshot(&fixture), before);
    assert_eq!(
        fixture
            .engine
            .item(&fixture.item)
            .unwrap()
            .plan
            .unwrap()
            .sections[0]
            .lines[0]
            .text,
        "Second writer"
    );
    Ok(())
}

#[test]
fn restoring_a_missing_version_is_a_clean_not_found_and_appends_nothing() {
    let mut fixture = Fixture::new().unwrap();
    let observed = fixture
        .engine
        .plan_page(&fixture.item)
        .unwrap()
        .content_hash;
    let before = snapshot(&fixture);
    assert!(matches!(
        fixture
            .engine
            .restore_plan_version(&fixture.item, 999, &observed),
        Err(CoreError::NotFound { .. })
    ));
    assert_eq!(snapshot(&fixture), before);
}
