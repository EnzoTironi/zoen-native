use super::*;
use crate::dto::RequestStatus;
use crate::pages::session::{DocumentEdit, DocumentView, PageSession};
use roda_docs::plan::{PlanBlockEdit, PlanCommand};
use rusqlite::types::ValueRef;
use sha2::{Digest, Sha256};

mod admission;

const EXACT: &str = "Reservation: cafe\u{301} 👩🏽‍💻\r\nkeep NEL\u{85}and paragraph\u{2029}bytes";

#[test]
fn legacy_line_addition_refuses_negative_cost_without_publishing_zero() {
    let mut fixture = Fixture::new();
    let before = storage(&fixture);
    for cents in [-1, -1_025, i64::MIN] {
        assert!(matches!(
            fixture
                .engine
                .add_plan_line(&fixture.item, 0, "Credit", cents),
            Err(CoreError::Invalid { .. })
        ));
        assert_eq!(storage(&fixture), before);
        assert_eq!(fixture.engine.item(&fixture.item).unwrap().version, 1);
    }
    for cents in [0, 1_025] {
        let outcome = fixture
            .engine
            .add_plan_line(&fixture.item, 0, "Exact cost", cents)
            .unwrap();
        let plan = outcome.item.plan.unwrap();
        let added = plan.sections[0].lines.last().unwrap();
        assert_eq!(added.text, "Exact cost");
        assert_eq!(added.cost_cents, cents);
        assert_eq!(plan.sections[0].lines[1].cost_cents, -7);
    }
}

#[test]
fn legacy_line_addition_refuses_an_unrepresentable_total_before_writing() {
    let mut fixture = Fixture::new();
    let before = storage(&fixture);
    assert!(matches!(
        fixture
            .engine
            .add_plan_line(&fixture.item, 0, "Too much", i64::MAX),
        Err(CoreError::Invalid { .. })
    ));
    assert_eq!(storage(&fixture), before);
    let outcome = fixture
        .engine
        .add_plan_line(&fixture.item, 0, "Exact upper total", i64::MAX - 193)
        .unwrap();
    assert_eq!(outcome.item.plan.unwrap().total_cents, i64::MAX);
}

fn storage(fixture: &Fixture) -> Vec<(String, Vec<String>)> {
    let connection = fixture.engine.store.conn();
    let tables: Vec<String> = connection
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    tables
        .into_iter()
        .map(|table| {
            let query = format!("SELECT * FROM \"{}\"", table.replace('"', "\"\""));
            let mut statement = connection.prepare(&query).unwrap();
            let columns = statement.column_count();
            let mut cursor = statement.query([]).unwrap();
            let mut hashes = Vec::new();
            while let Some(row) = cursor.next().unwrap() {
                let mut digest = Sha256::new();
                for column in 0..columns {
                    match row.get_ref(column).unwrap() {
                        ValueRef::Null => digest.update([0]),
                        ValueRef::Integer(value) => {
                            digest.update([1]);
                            digest.update(value.to_le_bytes());
                        }
                        ValueRef::Real(value) => {
                            digest.update([2]);
                            digest.update(value.to_bits().to_le_bytes());
                        }
                        ValueRef::Text(bytes) | ValueRef::Blob(bytes) => {
                            digest.update([
                                if matches!(row.get_ref(column).unwrap(), ValueRef::Text(_)) {
                                    3
                                } else {
                                    4
                                },
                            ]);
                            digest.update((bytes.len() as u64).to_le_bytes());
                            digest.update(bytes);
                        }
                    }
                }
                hashes.push(hex::encode(digest.finalize()));
            }
            hashes.sort();
            (table, hashes)
        })
        .collect()
}

fn promote(fixture: &mut Fixture, document: &PlanDocument) {
    let content = page(fixture.fold().manifest().unwrap(), document);
    fixture.ordered(fixture.version(ItemContent::Page(content)));
}

fn canonical(fixture: &mut Fixture) -> PlanDoc {
    let mut document = edited(
        fixture.fold().document.as_ref().unwrap(),
        LinePatch {
            text: Some(EXACT.into()),
            cost_cents: Some(350),
            ..Default::default()
        },
    );
    document
        .apply(PlanCommand::SetTitle("Current shared trip".into()))
        .unwrap();
    document
        .apply(PlanCommand::SetSummary("The current shared summary".into()))
        .unwrap();
    let expected = document.view().unwrap().to_plan_doc();
    promote(fixture, &document);
    expected
}

#[test]
fn generic_item_history_preserves_the_canonical_plan_cutover_gate() {
    let mut fixture = Fixture::new();
    let legacy = fixture.engine.item_at(&fixture.item, 1).unwrap();
    assert_eq!(legacy.plan.unwrap().total_cents, 193);
    canonical(&mut fixture);
    let before = storage(&fixture);
    for version in [1, 2] {
        assert!(matches!(
            fixture.engine.item_at(&fixture.item, version),
            Err(CoreError::Invalid { .. })
        ));
        assert_eq!(storage(&fixture), before);
    }
    assert!(
        !fixture
            .engine
            .plan_page(&fixture.item)
            .unwrap()
            .legacy_edit_admitted
    );
}

fn owned_agent(fixture: &mut Fixture) -> String {
    let mut identity = fixture.engine.identities[&fixture.owner].clone();
    identity.kind = IdentityKind::Agent;
    identity.name = "Owned agent".into();
    identity.handle = "owned-agent".into();
    identity.owner = Some(fixture.owner.clone());
    let agent = fixture.engine.create_identity(identity, true).unwrap();
    fixture.ordered(EventBody::MemberAdded {
        identity: agent.clone(),
        role: Role::Member,
    });
    fixture.ordered(EventBody::GrantIssued {
        grant: Grant {
            id: new_id("trust"),
            grantor: fixture.owner.clone(),
            grantee: Some(agent.clone()),
            scope: GrantScope::Space(fixture.space.clone()),
            capability: Capability::Trust(TrustLevel::Act),
            expires_at_ms: None,
        },
    });
    agent
}

#[test]
fn current_readers_use_the_shared_plan_and_retain_refused_legacy_history() {
    let mut fixture = Fixture::new();
    let expected = canonical(&mut fixture);
    let writer = add_identity(&mut fixture.engine, "Later writer");
    fixture.ordered(EventBody::MemberAdded {
        identity: writer.clone(),
        role: Role::Member,
    });
    let late = fixture.ordered_as(
        &writer,
        fixture.version(ItemContent::Plan(PlanDoc {
            title: "Refused old title".into(),
            summary: "Refused old summary".into(),
            ..original()
        })),
    );
    assert_eq!(
        fixture.fold().slots.last().unwrap().content,
        Some(ItemContent::Plan(PlanDoc {
            title: "Refused old title".into(),
            summary: "Refused old summary".into(),
            ..original()
        }))
    );
    let item = fixture.engine.item(&fixture.item).unwrap();
    assert_eq!(item.version, 2);
    assert_eq!(item.versions[0].number, 3);
    assert_eq!(item.title, expected.title);
    let plan = item.plan.unwrap();
    assert_eq!(plan.total_cents, 343);
    assert_eq!(plan.summary, expected.summary);
    assert_eq!(plan.sections[0].lines[0].text.as_bytes(), EXACT.as_bytes());
    assert_eq!(plan.sections[0].lines[1].cost_cents, -7);
    assert_eq!(plan.sections[0].lines[0].id, plan.sections[0].lines[1].id);
    let card = fixture.engine.card(&fixture.item).unwrap();
    assert_eq!(card.title, expected.title);
    assert_eq!(card.summary, expected.summary);
    assert_eq!(card.version, 2);
    assert_eq!(card.total_cents, Some(343));
    assert_eq!(card.line_count, 2);
    let hits = fixture.engine.search("Reservation");
    let hit = hits
        .iter()
        .find(|hit| hit.item_id.as_deref() == Some(&fixture.item))
        .unwrap();
    assert_eq!(hit.title, expected.title);
    assert_eq!(hit.author.id, fixture.owner);
    assert!(hit.snippet.starts_with(EXACT));
    assert!(fixture.engine.search("Refused old title").is_empty());
    let indexed = fixture
        .engine
        .universal_search("Reservation", &["item".into()], 20)
        .unwrap();
    assert_eq!(indexed.hits.len(), 1);
    assert_eq!(indexed.hits[0].title, expected.title);
    assert_eq!(indexed.hits[0].persona.as_ref().unwrap().id, fixture.owner);
    assert!(fixture
        .engine
        .universal_search("Refused", &["item".into()], 20)
        .unwrap()
        .hits
        .is_empty());
    assert_eq!(
        fixture.engine.logs[&fixture.space]
            .events()
            .last()
            .unwrap()
            .content,
        late.content
    );
    fixture.engine.reload().unwrap();
    assert_eq!(
        fixture
            .engine
            .item(&fixture.item)
            .unwrap()
            .plan
            .unwrap()
            .sections[0]
            .lines[0]
            .text
            .as_bytes(),
        EXACT.as_bytes()
    );
    assert!(fixture.engine.page(&fixture.item).is_err()); // Public Plan writers stay closed.
}

#[test]
fn legacy_writers_refuse_before_mutating_after_cutover_or_while_it_is_pending() {
    for pending in [false, true] {
        let mut fixture = Fixture::new();
        let document = fixture.fold().document.unwrap();
        let content = page(fixture.fold().manifest().unwrap(), &document);
        if pending {
            fixture.queue(fixture.version(ItemContent::Page(content)));
        } else {
            fixture.ordered(fixture.version(ItemContent::Page(content)));
            fixture.ordered(fixture.version(ItemContent::Plan(original())));
        }
        let before = storage(&fixture);
        let line = original().sections[0].lines[0].id.clone();
        assert!(fixture
            .engine
            .edit_plan_line(&fixture.item, &line, "Old edit", 777)
            .is_err());
        assert_eq!(storage(&fixture), before);
        assert!(fixture
            .engine
            .toggle_plan_line(&fixture.item, &line)
            .is_err());
        assert_eq!(storage(&fixture), before);
        assert!(fixture
            .engine
            .add_plan_line(&fixture.item, 0, "Old insertion", 777)
            .is_err());
        assert_eq!(storage(&fixture), before);
        assert!(fixture
            .engine
            .remove_plan_line(&fixture.item, &line)
            .is_err());
        assert_eq!(storage(&fixture), before);
        assert!(fixture
            .engine
            .restore_version(&fixture.item, 1, "Old restore")
            .is_err());
        assert_eq!(storage(&fixture), before);
    }
}

#[test]
fn existing_offline_legacy_previews_keep_their_exact_bytes() {
    let mut fixture = Fixture::new();
    let mut pending = original();
    pending.summary = EXACT.into();
    fixture.queue(fixture.version(ItemContent::Plan(pending.clone())));
    assert_eq!(
        fixture
            .engine
            .item(&fixture.item)
            .unwrap()
            .plan
            .unwrap()
            .summary
            .as_bytes(),
        EXACT.as_bytes()
    );
    let item = new_id("pending-creation");
    fixture.queue(EventBody::ItemCreated {
        item: item.clone(),
        kind: ItemKind::Plan,
        content: ItemContent::Plan(pending),
        origin: "offline".into(),
    });
    assert!(fixture
        .engine
        .plan_timeline(&item)
        .unwrap()
        .document
        .is_none());
    assert_eq!(
        fixture
            .engine
            .item(&item)
            .unwrap()
            .plan
            .unwrap()
            .summary
            .as_bytes(),
        EXACT.as_bytes()
    );
    assert_eq!(fixture.engine.card(&item).unwrap().total_cents, Some(193));
}

#[test]
fn private_plan_drafts_never_enter_cards_search_or_approval_reads() {
    let mut fixture = Fixture::new();
    canonical(&mut fixture);
    let mut session = PageSession::open(&fixture.engine, &fixture.item).unwrap();
    let read = session.read(&fixture.engine).unwrap();
    let DocumentView::Plan { blocks } = &read.view else {
        panic!("Plan")
    };
    let block = blocks
        .iter()
        .find(|block| block.plain_text() == EXACT)
        .unwrap();
    let edit = PlanBlockEdit {
        id: block.id.clone(),
        content: block.content.clone(),
        text: "Private unsaved reservation".into(),
        spans: roda_docs::spans_of(&block.runs),
    };
    let order: Vec<_> = blocks.iter().map(|block| block.id.clone()).collect();
    session
        .apply_from(
            &fixture.engine,
            "private-draft",
            &read.edit_context,
            DocumentEdit::Plan {
                order: &order,
                changed: &[edit],
            },
        )
        .unwrap();
    fixture
        .engine
        .pages
        .sessions
        .lock()
        .unwrap()
        .insert(fixture.item.clone(), session);
    let before = storage(&fixture);
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
        EXACT
    );
    assert_eq!(
        fixture.engine.card(&fixture.item).unwrap().total_cents,
        Some(343)
    );
    assert!(fixture.engine.search("Private unsaved").is_empty());
    assert_eq!(storage(&fixture), before);
    let mut session = fixture
        .engine
        .pages
        .sessions
        .lock()
        .unwrap()
        .remove(&fixture.item)
        .unwrap();
    let read = session.read(&fixture.engine).unwrap();
    assert!(read.unsaved);
    let DocumentView::Plan { blocks } = read.view else {
        panic!("Plan")
    };
    assert!(blocks
        .iter()
        .any(|block| block.plain_text() == "Private unsaved reservation"));
}

#[test]
fn approval_reads_keep_first_duplicate_id_and_exact_text_cents_semantics() {
    let mut fixture = Fixture::new();
    let agent = owned_agent(&mut fixture);
    let line = original().sections[0].lines[0].id.clone();
    let request = fixture
        .engine
        .open_request(
            &fixture.space,
            &agent,
            "Book",
            "Exact content",
            "Us",
            ActionClass::Money { cents: 200 },
            Some((fixture.item.clone(), line.clone())),
        )
        .unwrap()
        .unwrap();
    let mut document = edited(
        fixture.fold().document.as_ref().unwrap(),
        LinePatch {
            done: Some(true),
            ..Default::default()
        },
    );
    promote(&mut fixture, &document);
    assert_eq!(
        fixture
            .engine
            .requests()
            .iter()
            .find(|row| row.id == request)
            .unwrap()
            .status,
        RequestStatus::Pending
    );
    let before = storage(&fixture);
    assert!(fixture.engine.resolve_request(&request, true).is_err());
    assert_eq!(storage(&fixture), before);
    assert!(fixture
        .engine
        .open_request(
            &fixture.space,
            &agent,
            "Old link",
            "Old link",
            "Us",
            ActionClass::Money { cents: 200 },
            Some((fixture.item.clone(), line))
        )
        .is_err());
    assert_eq!(storage(&fixture), before);
    document = edited(
        &document,
        LinePatch {
            text: Some("Changed approved text".into()),
            ..Default::default()
        },
    );
    promote(&mut fixture, &document);
    assert_eq!(
        fixture
            .engine
            .requests()
            .iter()
            .find(|row| row.id == request)
            .unwrap()
            .status,
        RequestStatus::Stale
    );
    assert!(matches!(
        fixture.engine.resolve_request(&request, true),
        Err(CoreError::Stale { .. })
    ));
    fixture.engine.resolve_request(&request, false).unwrap();
    assert_eq!(
        fixture
            .engine
            .requests()
            .iter()
            .find(|row| row.id == request)
            .unwrap()
            .status,
        RequestStatus::Denied
    );
}

#[test]
fn poll_plan_effect_refuses_before_closing_or_posting_into_a_canonical_plan() {
    let mut fixture = Fixture::new();
    let agent = owned_agent(&mut fixture);
    let poll = fixture
        .engine
        .agent_create_app(
            &fixture.space,
            &agent,
            "start_poll",
            r#"{"question":"Stay?","options":["Inn","Camping"]}"#,
            "test",
            "Choose a stay",
        )
        .unwrap()
        .item
        .unwrap();
    fixture
        .engine
        .app_call_tool(&poll.id, "poll_vote", r#"{"option":"o1"}"#, false)
        .unwrap();
    canonical(&mut fixture);
    let before = storage(&fixture);
    assert!(fixture
        .engine
        .app_call_tool(&poll.id, "poll_close", "{}", false)
        .is_err());
    assert_eq!(storage(&fixture), before);
}

#[test]
fn total_overflow_is_unavailable_and_minimum_legacy_cents_stay_readable() {
    let mut fixture = Fixture::new();
    let mut original = original();
    original.sections[0].lines[0].cost_cents = i64::MAX;
    original.sections[0].lines[1].cost_cents = 1;
    fixture.ordered(fixture.version(ItemContent::Plan(original.clone())));
    let before = storage(&fixture);
    let card = fixture.engine.card(&fixture.item).unwrap();
    assert_eq!(card.total_cents, None);
    assert_eq!(card.line_count, 2);
    assert!(fixture.engine.item(&fixture.item).is_err());
    assert_eq!(storage(&fixture), before);
    let document = fixture.fold().document.unwrap();
    promote(&mut fixture, &document);
    assert_eq!(
        fixture.engine.card(&fixture.item).unwrap().total_cents,
        None
    );
    assert!(fixture.engine.item(&fixture.item).is_err());
    let mut legacy = Fixture::new();
    original.sections[0].lines[0].cost_cents = i64::MIN;
    original.sections[0].lines[1].cost_cents = 0;
    legacy.ordered(legacy.version(ItemContent::Plan(original)));
    assert_eq!(
        legacy
            .engine
            .item(&legacy.item)
            .unwrap()
            .plan
            .unwrap()
            .total_cents,
        i64::MIN
    );
    assert_eq!(
        legacy.engine.card(&legacy.item).unwrap().total_cents,
        Some(i64::MIN)
    );
    assert_eq!(legacy.engine.search("Hotel").len(), 1);
    assert_eq!(
        crate::i18n::money_in(crate::i18n::Lang::En, i64::MIN),
        "−$92,233,720,368,547,758.08"
    );
}

#[test]
fn credits_cancel_intermediate_overflow_independently_of_row_order() {
    for (costs, total) in [
        ([i64::MAX, 1, -1], i64::MAX),
        ([-1, 1, i64::MAX], i64::MAX),
        ([i64::MIN, -1, 1], i64::MIN),
        ([1, -1, i64::MIN], i64::MIN),
    ] {
        let mut fixture = Fixture::new();
        let mut legacy = original();
        let template = legacy.sections[0].lines[0].clone();
        legacy.sections[0].lines = costs
            .iter()
            .enumerate()
            .map(|(index, cost)| PlanLine {
                id: format!("cancel-credit-{index}"),
                text: format!("Exact credit row {index}"),
                cost_cents: *cost,
                ..template.clone()
            })
            .collect();
        fixture.ordered(fixture.version(ItemContent::Plan(legacy)));
        let document = fixture.fold().document.unwrap();
        assert_eq!(
            document.view().unwrap().total,
            roda_docs::plan::TotalCents::Cents(total)
        );
        let before = storage(&fixture);
        let card = fixture.engine.card(&fixture.item).unwrap();
        assert_eq!(card.total_cents, Some(total));
        assert_eq!(card.line_count, 3);
        let item = fixture.engine.item(&fixture.item).unwrap();
        assert_eq!(item.plan.as_ref().unwrap().total_cents, total);
        assert_eq!(
            item.plan.unwrap().sections[0]
                .lines
                .iter()
                .map(|line| line.cost_cents)
                .collect::<Vec<_>>(),
            costs
        );
        assert_eq!(storage(&fixture), before);
        promote(&mut fixture, &document);
        assert_eq!(
            fixture.engine.card(&fixture.item).unwrap().total_cents,
            Some(total)
        );
        assert_eq!(
            fixture
                .engine
                .item(&fixture.item)
                .unwrap()
                .plan
                .unwrap()
                .total_cents,
            total
        );
        fixture.engine.reload().unwrap();
        assert_eq!(
            fixture.engine.card(&fixture.item).unwrap().total_cents,
            Some(total)
        );
    }
}
