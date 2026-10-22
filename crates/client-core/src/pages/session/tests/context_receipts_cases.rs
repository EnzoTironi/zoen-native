//! Complete context receipts scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn typed_plan_draft_and_exact_receipt_survive_file_reopen() {
    let db = Database::new();
    let mut f = Fixture::at(&db.path());
    let mut s = f.session();
    let read = s.read(&f.engine).unwrap();
    let mut edit = text_edit(&read, RAW);
    edit.spans = vec![Span {
        start: 0,
        end: 3,
        key: "b".into(),
        value: String::new(),
    }];
    if let PlanBlockContent::Line {
        cost_cents, done, ..
    } = &mut edit.content
    {
        *cost_cents = 987;
        *done = true;
    }
    let mut title = changed(&blocks(&read)[0]);
    title.content = PlanBlockContent::Title {
        budget_cents: Some(7_777),
    };
    let prose = PlanBlockEdit {
        id: "new-prose".into(),
        content: PlanBlockContent::Prose { kind: Kind::Quote },
        text: "unique private prose text".into(),
        spans: vec![],
    };
    let mut order = order_read(&read);
    order.push(prose.id.clone());
    let edits = vec![edit, title, prose];
    let accepted = s
        .apply_from(
            &f.engine,
            "durable-plan",
            &read.edit_context,
            DocumentEdit::Plan {
                order: &order,
                changed: &edits,
            },
        )
        .unwrap();
    assert!(accepted.applied_edit_context.starts_with("v2:"));
    let projection = accepted.current.view;
    let cipher = f.cipher().unwrap();
    assert!(!cipher.contains("unique private prose text"));
    assert!(!cipher.contains(RAW));
    let legacy = plan_doc(&s.live).unwrap().view().unwrap().to_plan_doc();
    assert_eq!(legacy.sections[0].lines[0].text, RAW);
    assert_eq!(legacy.sections[0].lines[0].cost_cents, 987);
    assert!(legacy.sections[0].lines[0].done);
    assert_eq!(
        legacy.sections[0].lines[0].id,
        legacy.sections[0].lines[1].id
    );
    drop(s);
    f = f.reopen();
    s = f.session();
    assert_eq!(s.read(&f.engine).unwrap().view, projection);
    let replay = s
        .apply_from(
            &f.engine,
            "durable-plan",
            &read.edit_context,
            DocumentEdit::Plan {
                order: &order,
                changed: &edits,
            },
        )
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.applied_edit_context, accepted.applied_edit_context);
    assert_eq!(replay.applied_content_hash, accepted.applied_content_hash);
    assert_eq!(f.cipher().unwrap(), cipher);
    drop(s);
    drop(f);
}

pub(super) fn contexts_bind_actor_space_item_kind_and_creation() {
    let mut f = Fixture::new();
    let mut s = f.session();
    let read = s.read(&f.engine).unwrap();
    apply(
        &f,
        &mut s,
        &read,
        "scope-original",
        &[text_edit(&read, "scoped draft")],
    )
    .unwrap();
    let before = exact(&f, &s);
    let tag = journal(&s).tag.clone();
    let receipt = journal(&s).receipt("scope-original").unwrap().clone();
    for forged in [
        PlanOrigin {
            space: "other-space".into(),
            ..tag.clone()
        },
        PlanOrigin {
            item: "other-item".into(),
            ..tag.clone()
        },
        PlanOrigin {
            creation_event: "other-creation".into(),
            ..tag.clone()
        },
    ] {
        let mut forged_receipt = receipt.clone();
        forged_receipt.tag = forged;
        assert!(verify_receipt(
            &f.engine,
            f.engine.item_state(&f.item).unwrap(),
            &tag,
            plan_doc(&s.live).unwrap(),
            &forged_receipt
        )
        .is_err());
    }
    no_change(&f, &s, &before);
    let other_id = new_id("other-plan");
    f.ordered(EventBody::ItemCreated {
        item: other_id.clone(),
        kind: ItemKind::Plan,
        content: ItemContent::Plan(original()),
        origin: "other".into(),
    });
    let mut other = PageSession::open(&f.engine, &other_id).unwrap();
    let other_read = other.read(&f.engine).unwrap();
    assert!(other
        .apply_from(
            &f.engine,
            "foreign-item",
            &read.edit_context,
            DocumentEdit::Plan {
                order: &order_read(&other_read),
                changed: &[]
            }
        )
        .is_err());
    assert_eq!(f.cipher(), before.0);
    let actor = identity(&mut f.engine, "Another");
    f.ordered(EventBody::MemberAdded {
        identity: actor.clone(),
        role: Role::Member,
    });
    f.engine.me = Some(actor.clone());
    f.engine.store.set_meta("me", &actor).unwrap();
    assert!(verify_receipt(
        &f.engine,
        f.engine.item_state(&f.item).unwrap(),
        &tag,
        plan_doc(&s.live).unwrap(),
        &receipt
    )
    .is_err());
    let mut foreign = f.session();
    let foreign_read = foreign.read(&f.engine).unwrap();
    assert!(foreign
        .apply_from(
            &f.engine,
            "foreign-actor",
            &read.edit_context,
            DocumentEdit::Plan {
                order: &order_read(&foreign_read),
                changed: &[]
            }
        )
        .is_err());
    f.engine.me = Some(f.owner.clone());
    f.engine.store.set_meta("me", &f.owner).unwrap();
    let it = f.engine.item_state(&f.item).unwrap();
    for forged in [
        PlanOrigin {
            space: "other-space".into(),
            ..tag.clone()
        },
        PlanOrigin {
            creation_event: "another signed creation".into(),
            ..tag.clone()
        },
    ] {
        assert!(decode_context(&f.engine, it, &forged, &read.edit_context).is_err());
    }
    assert!(f
        .engine
        .decode_page_edit_context(it, &read.edit_context)
        .is_err());
    // Same actor/item spelling with an independent valid signed creation is refused.
    let mut recreated = Fixture::with_ids(":memory:", f.space.clone(), f.item.clone());
    let secret = f.engine.signers[&f.owner].secret();
    recreated
        .engine
        .store
        .put_identity(&f.engine.identities[&f.owner], Some(&secret))
        .unwrap();
    recreated.engine.me = Some(f.owner.clone());
    recreated.engine.store.set_meta("me", &f.owner).unwrap();
    recreated.engine.reload().unwrap();
    // Authenticate the owner on the independent timeline before exercising its context.
    recreated.ordered(EventBody::MemberAdded {
        identity: f.owner.clone(),
        role: Role::Member,
    });
    let mut foreign = recreated.session();
    let view = foreign.read(&recreated.engine).unwrap();
    assert!(foreign
        .apply_from(
            &recreated.engine,
            "foreign-creation",
            &read.edit_context,
            DocumentEdit::Plan {
                order: &order_read(&view),
                changed: &[]
            }
        )
        .is_err());
    assert!(recreated.cipher().is_none());
}

pub(super) fn same_mutation_replays_exact_accepted_context_but_returns_current_view() {
    let mut f = Fixture::new();
    let mut s = f.session();
    let read = s.read(&f.engine).unwrap();
    let edit = text_edit(&read, "own accepted text");
    let accepted = apply(&f, &mut s, &read, "same-id", std::slice::from_ref(&edit)).unwrap();
    let mut remote = changed(&blocks(&read)[1]);
    remote.text = "remote unseen summary".into();
    f.remote(remote);
    let current = s.read(&f.engine).unwrap();
    let before = exact(&f, &s);
    let retry = apply(&f, &mut s, &read, "same-id", &[edit]).unwrap();
    assert!(retry.replayed);
    assert_eq!(retry.mutation_id, "same-id");
    assert_eq!(retry.request_hash, accepted.request_hash);
    assert_eq!(retry.applied_edit_context, accepted.applied_edit_context);
    assert_eq!(retry.applied_content_hash, accepted.applied_content_hash);
    assert_ne!(retry.current.content_hash, retry.applied_content_hash);
    assert_eq!(retry.current.view, current.view);
    no_change(&f, &s, &before);
}

pub(super) fn changed_request_with_same_mutation_refuses_atomically() {
    let f = Fixture::new();
    let mut s = f.session();
    let read = s.read(&f.engine).unwrap();
    let edit = text_edit(&read, "one request");
    apply(&f, &mut s, &read, "same-id", std::slice::from_ref(&edit)).unwrap();
    let before = exact(&f, &s);
    let mut text = edit.clone();
    text.text.push('!');
    let mut spans = edit.clone();
    spans.spans = vec![Span {
        start: 0,
        end: 1,
        key: "i".into(),
        value: String::new(),
    }];
    let mut cents = edit.clone();
    if let PlanBlockContent::Line { cost_cents, .. } = &mut cents.content {
        *cost_cents = 201;
    }
    for change in [text, spans, cents] {
        assert!(apply(&f, &mut s, &read, "same-id", &[change]).is_err());
        no_change(&f, &s, &before);
    }
    let mut order = order_read(&read);
    order.swap(3, 4);
    assert!(s
        .apply_from(
            &f.engine,
            "same-id",
            &read.edit_context,
            DocumentEdit::Plan {
                order: &order,
                changed: std::slice::from_ref(&edit)
            }
        )
        .is_err());
    let current = s.read(&f.engine).unwrap();
    assert!(s
        .apply_from(
            &f.engine,
            "same-id",
            &current.edit_context,
            DocumentEdit::Plan {
                order: &order_read(&read),
                changed: &[edit]
            }
        )
        .is_err());
    no_change(&f, &s, &before);
}
