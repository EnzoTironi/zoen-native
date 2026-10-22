//! Complete journal recovery scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn receipt_eviction_fences_forgotten_retry_across_reopen() {
    let db = Database::new();
    let mut f = Fixture::at(&db.path());
    let mut s = f.session();
    let read = s.read(&f.engine).unwrap();
    let mut retained = None;
    for n in 0..513 {
        let current = s.read(&f.engine).unwrap();
        retained = Some((
            current.edit_context.clone(),
            apply(&f, &mut s, &current, &format!("receipt-{n}"), &[]).unwrap(),
        ));
    }
    assert_eq!(journal(&s).receipts.len(), 512);
    assert!(journal(&s).minimum_generation > 0);
    let (context, accepted) = retained.unwrap();
    drop(s);
    f = f.reopen();
    s = f.session();
    let before = exact(&f, &s);
    assert!(matches!(
        apply(&f, &mut s, &read, "receipt-0", &[]),
        Err(CoreError::Stale { .. })
    ));
    let retry = s
        .apply_from(
            &f.engine,
            "receipt-512",
            &context,
            DocumentEdit::Plan {
                order: &order_read(&read),
                changed: &[],
            },
        )
        .unwrap();
    assert!(retry.replayed);
    assert_eq!(retry.applied_edit_context, accepted.applied_edit_context);
    no_change(&f, &s, &before);
    drop(s);
    drop(f);
}

pub(super) fn journal_insert_or_update_failure_keeps_previous_draft() {
    let f = Fixture::new();
    let mut s = f.session();
    let read = s.read(&f.engine).unwrap();
    let edit = text_edit(&read, "first durable");
    trigger(
        &f,
        "fail_insert",
        "INSERT",
        "meta",
        "WHEN NEW.key LIKE 'page.draft.%'",
    );
    let before = exact(&f, &s);
    assert!(apply(&f, &mut s, &read, "first", std::slice::from_ref(&edit)).is_err());
    no_change(&f, &s, &before);
    drop_trigger(&f, "fail_insert");
    apply(&f, &mut s, &read, "first", &[edit]).unwrap();
    let current = s.read(&f.engine).unwrap();
    let edit = text_edit(&current, "second durable");
    trigger(
        &f,
        "fail_update",
        "UPDATE",
        "meta",
        "WHEN NEW.key LIKE 'page.draft.%'",
    );
    let before = exact(&f, &s);
    assert!(apply(&f, &mut s, &current, "second", std::slice::from_ref(&edit)).is_err());
    no_change(&f, &s, &before);
    drop_trigger(&f, "fail_update");
    assert!(
        !apply(&f, &mut s, &current, "second", &[edit])
            .unwrap()
            .replayed
    );
}

pub(super) fn oversized_context_or_journal_fails_before_memory_replacement() {
    let f = Fixture::new();
    let mut s = f.session();
    let read = s.read(&f.engine).unwrap();
    apply(&f, &mut s, &read, "saved", &[text_edit(&read, "existing")]).unwrap();
    let before = exact(&f, &s);
    let oversized = "x".repeat(MAX_CONTEXT_BYTES + 1);
    assert!(s
        .apply_from(
            &f.engine,
            "huge-context",
            &oversized,
            DocumentEdit::Plan {
                order: &order_read(&read),
                changed: &[]
            }
        )
        .is_err());
    let mut huge = text_edit(&read, "");
    huge.text = "x".repeat(MAX_PLAN_PACKET_BYTES + 1);
    assert!(apply(&f, &mut s, &read, "huge-arguments", &[huge]).is_err());
    no_change(&f, &s, &before);
    let tag = journal(&s).tag.clone();
    let it = f.engine.item_state(&f.item).unwrap();
    assert!(context(
        &f.engine,
        it,
        &tag,
        1,
        &vec![0; PlanRevision::MAX_ENCODED_BYTES + 1]
    )
    .is_err());
    no_change(&f, &s, &before);
    let mut full = plan_doc(&s.live).unwrap().fork().unwrap();
    let rev = full.revision().unwrap();
    let mut edit = text_edit(&read, "");
    edit.text = "x".repeat(65_537);
    assert!(full.apply_from(&rev, &order_doc(&full), &[edit]).is_err());
    assert_eq!(full.full_update().to_json().unwrap(), before.1);
    // Real large scoped contexts reach the aggregate cap through normal acceptance.
    // No malformed receipt or oversized single field is injected into this journal.
    let large = Fixture::with_ids(
        ":memory:",
        format!("space-{}", "s".repeat(4_096)),
        format!("item-{}", "i".repeat(4_096)),
    );
    let mut session = large.session();
    let mut hit_cap = false;
    for n in 0..512 {
        let read = session.read(&large.engine).unwrap();
        let exact_before = exact(&large, &session);
        match apply(&large, &mut session, &read, &format!("bounded-{n}"), &[]) {
            Ok(_) => {}
            Err(error) => {
                assert!(error.to_string().contains("bound"));
                no_change(&large, &session, &exact_before);
                hit_cap = true;
                break;
            }
        }
    }
    assert!(
        hit_cap,
        "aggregate journal limit must be observable rather than clipped"
    );
}

pub(super) fn lost_response_after_durable_accept_recovers_the_same_receipt() {
    let db = Database::new();
    let mut f = Fixture::at(&db.path());
    let mut s = f.session();
    let read = s.read(&f.engine).unwrap();
    let edit = text_edit(&read, "persisted before lost response");
    s.lose_apply_response = true;
    assert!(apply(&f, &mut s, &read, "lost-reply", std::slice::from_ref(&edit)).is_err());
    let receipt = journal(&s).receipt("lost-reply").unwrap().clone();
    let cipher = f.cipher();
    drop(s);
    f = f.reopen();
    s = f.session();
    let history = plan_doc(&s.live).unwrap().full_update().to_json().unwrap();
    let retry = apply(&f, &mut s, &read, "lost-reply", &[edit]).unwrap();
    assert!(retry.replayed);
    assert_eq!(retry.applied_edit_context, receipt.applied_edit_context);
    assert_eq!(retry.applied_content_hash, receipt.content_hash);
    assert_eq!(f.cipher(), cipher);
    assert_eq!(
        plan_doc(&s.live).unwrap().full_update().to_json().unwrap(),
        history
    );
    drop(s);
    drop(f);
}
