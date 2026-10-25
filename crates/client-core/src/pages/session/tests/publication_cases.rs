//! Complete publication scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn outbox_insert_failure_keeps_draft_and_receipts(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::new()?;
    let mut s = f.session();
    let read = s.read(&f.engine).unwrap();
    apply(
        &f,
        &mut s,
        &read,
        "draft",
        &[text_edit(&read, "queued later")],
    )
    .unwrap();
    trigger(&f, "fail_outbox", "INSERT", "outbox", "");
    let before = exact(&f, &s);
    assert!(s.commit(&mut f.engine, "will fail").is_err());
    no_change(&f, &s, &before);
    assert!(journal(&s).commit.is_none());
    drop_trigger(&f, "fail_outbox");
    let reference = queued(s.commit(&mut f.engine, "retry").unwrap());
    assert!(matches!(
        f.pending(&reference).body,
        EventBody::ItemVersioned {
            content: ItemContent::Page(_),
            ..
        }
    ));

    Ok(())
}

pub(super) fn journal_failure_after_outbox_insert_rolls_back_event_and_projection(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::new()?;
    let mut s = f.session();
    let read = s.read(&f.engine).unwrap();
    let mut edit = text_edit(&read, "");
    edit.text = "b".repeat(55_000);
    apply(&f, &mut s, &read, "blob-draft", &[edit]).unwrap();
    let before = exact(&f, &s);
    let versions = f.engine.item_state(&f.item).unwrap().versions.len();
    let uploads = f.engine.store.uploads_pending().unwrap();
    // This fault fires only once the real outbox INSERT exists inside the savepoint.
    trigger(
        &f,
        "fail_journal",
        "UPDATE",
        "meta",
        "WHEN NEW.key LIKE 'page.draft.%' AND EXISTS (SELECT 1 FROM outbox WHERE failed = 0)",
    );
    assert!(s.commit(&mut f.engine, "rollback").is_err());
    no_change(&f, &s, &before);
    assert_eq!(
        f.engine.item_state(&f.item).unwrap().versions.len(),
        versions
    );
    assert_eq!(f.engine.store.uploads_pending().unwrap(), uploads);
    let media: u64 = f
        .engine
        .store
        .conn()
        .query_row("SELECT count(*) FROM media", [], |r| r.get(0))
        .unwrap();
    assert_eq!(media, 0);
    drop_trigger(&f, "fail_journal");
    let reference = queued(s.commit(&mut f.engine, "success").unwrap());
    assert!(matches!(
        &f.pending(&reference).body,
        EventBody::ItemVersioned {
            content: ItemContent::Page(PageContent { blob: Some(_), .. }),
            ..
        }
    ));
    f.ack(&reference)?;
    assert!(!s.read(&f.engine).unwrap().unsaved);

    Ok(())
}

pub(super) fn lost_commit_response_reopens_one_exact_pending_source(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let db = Database::new()?;
    let mut f = Fixture::at(&db.path())?;
    let mut s = f.session();
    let read = s.read(&f.engine).unwrap();
    apply(
        &f,
        &mut s,
        &read,
        "draft",
        &[text_edit(&read, "one source")],
    )
    .unwrap();
    s.lose_commit_response = true;
    assert!(s.commit(&mut f.engine, "lost commit return").is_err());
    let reference = journal(&s).commit.clone().unwrap();
    let ciphertext = f.cipher();
    assert_eq!(f.engine.store.outbox_len().unwrap(), 1);
    drop(s);
    f = f.reopen();
    s = f.session();
    assert_eq!(journal(&s).commit.as_ref(), Some(&reference));
    assert_eq!(
        s.commit(&mut f.engine, "retry").unwrap(),
        CommitStatus::Pending(CommitRef::Plan(reference.clone()))
    );
    assert_eq!(f.engine.store.outbox_len().unwrap(), 1);
    assert_eq!(f.cipher(), ciphertext);
    let undo = apply(&f, &mut s, &read, "undo-before-ack", &[]).unwrap();
    assert_eq!(undo.current.view, read.view);
    assert_eq!(undo.current.content_hash, read.content_hash);
    assert_eq!(journal(&s).commit.as_ref(), Some(&reference));
    let pending_ciphertext = f.cipher();
    assert_eq!(
        s.commit(&mut f.engine, "unchanged while source is pending")
            .unwrap(),
        CommitStatus::Pending(CommitRef::Plan(reference.clone()))
    );
    assert_eq!(f.cipher(), pending_ciphertext);
    assert_eq!(f.engine.store.outbox_len().unwrap(), 1);
    assert!(journal(&s).snapshot.is_some());
    assert!(journal(&s).has_draft);
    drop(s);
    f = f.reopen();
    s = f.session();
    assert_eq!(journal(&s).commit.as_ref(), Some(&reference));
    assert_eq!(s.read(&f.engine).unwrap().view, read.view);
    assert_eq!(
        s.commit(&mut f.engine, "pending after restart").unwrap(),
        CommitStatus::Pending(CommitRef::Plan(reference.clone()))
    );
    apply(
        &f,
        &mut s,
        &read,
        "redo-before-ack",
        &[text_edit(&read, "one source")],
    )
    .unwrap();
    f.ack(&CommitRef::Plan(reference))?;
    assert!(!s.read(&f.engine).unwrap().unsaved);
    drop(s);
    drop(f);

    Ok(())
}

pub(super) fn only_complete_canonical_exact_source_acks(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::new()?;
    let mut s = f.session();
    let read = s.read(&f.engine).unwrap();
    let accepted = apply(
        &f,
        &mut s,
        &read,
        "draft",
        &[text_edit(&read, "exact source only")],
    )
    .unwrap();
    let reference = queued(s.commit(&mut f.engine, "publication").unwrap());
    let event = f.pending(&reference);
    assert!(s.read(&f.engine).unwrap().unsaved);
    assert!(s.read(&f.engine).unwrap().pending_sync);
    let wrong_client = f.signed_as(&f.owner, event.body.clone())?;
    deliver(&mut f.engine, &wrong_client, Ingest::Applied)?;
    assert!(s.read(&f.engine).unwrap().unsaved);
    assert!(journal(&s).commit.is_some());
    let mut different = event.body.clone();
    let EventBody::ItemVersioned {
        content: ItemContent::Page(page),
        ..
    } = &mut different
    else {
        panic!("Page")
    };
    page.path = "different signed content".into();
    let wrong_digest = f.engine.author_for(&f.owner).unwrap().sign_event(
        &f.space,
        &event.client_id,
        event.at_ms,
        event.seen.clone(),
        different,
    )?;
    assert_ne!(
        plan::source(&f.space, &f.item, &wrong_digest, false).event_id,
        plan::source(&f.space, &f.item, &event, false).event_id
    );
    // A valid signature and matching CID do not acknowledge different content.
    deliver(&mut f.engine, &wrong_digest, Ingest::Applied)?;
    assert_eq!(f.engine.store.outbox_len().unwrap(), 1);
    assert_eq!(f.pending(&reference), event);
    let still_pending = s.read(&f.engine).unwrap();
    assert!(still_pending.unsaved);
    assert!(still_pending.pending_sync);
    assert!(journal(&s).commit.is_some());
    deliver(&mut f.engine, &event, Ingest::Confirmed)?;
    assert_eq!(f.engine.store.outbox_len().unwrap(), 0);
    let final_read = s.read(&f.engine).unwrap();
    assert!(!final_read.unsaved);
    assert!(!final_read.pending_sync);
    assert!(journal(&s).commit.is_none());
    assert_eq!(final_read.view, accepted.current.view);
    let peer = f.peer()?;
    assert_eq!(
        peer.engine
            .plan_timeline(&peer.item)
            .unwrap()
            .document
            .unwrap()
            .blocks()
            .unwrap(),
        blocks(&final_read)
    );
    encrypted_exact_source_and_peer_readability()?;

    Ok(())
}

pub(super) fn semantically_refused_source_preserves_draft_and_visible_history(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::new()?;
    let mut s = f.session();
    let read = s.read(&f.engine).unwrap();
    apply(
        &f,
        &mut s,
        &read,
        "draft",
        &[text_edit(&read, "retained after semantic refusal")],
    )
    .unwrap();
    let mut malformed = f.page(plan_doc(&s.live).unwrap());
    malformed.loro = encode_bytes(b"{\"unrecognized\":\"not a Plan packet\"}");
    let event = f.queue(f.version(ItemContent::Page(malformed)));
    expect_source(&f, &mut s, &event);
    let before = f.cipher();
    deliver(&mut f.engine, &event, Ingest::Confirmed)?;
    let current = s.read(&f.engine).unwrap();
    assert!(current.unsaved);
    assert!(current.save_error.is_some());
    assert_eq!(f.cipher(), before);
    let fold = f.engine.plan_timeline(&f.item).unwrap();
    assert!(fold.frozen_at.is_none());
    assert!(matches!(
        fold.slots.last().unwrap().acceptance,
        Acceptance::Refused(_)
    ));
    assert!(fold.slots.last().unwrap().content.is_some());
    assert_eq!(fold.slots.last().unwrap().source.client_id, event.client_id);
    let retry = queued(s.commit(&mut f.engine, "validated retry").unwrap());
    assert_ne!(f.pending(&retry).client_id, event.client_id);
    f.ack(&retry)?;
    assert!(!s.read(&f.engine).unwrap().unsaved);

    Ok(())
}

pub(super) fn waiting_blob_blocks_edit_ack_and_later_versions_until_ready(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::new()?;
    let mut s = f.session();
    let read = s.read(&f.engine).unwrap();
    let mut edit = text_edit(&read, "");
    edit.text = "g".repeat(55_000);
    let accepted = apply(&f, &mut s, &read, "blob-draft", &[edit]).unwrap();
    let reference = queued(s.commit(&mut f.engine, "blob").unwrap());
    let event = f.pending(&reference);
    let EventBody::ItemVersioned {
        content: ItemContent::Page(page),
        ..
    } = &event.body
    else {
        panic!("Page")
    };
    let blob = page.blob.as_ref().expect("actual blob publication");
    let bytes = f.engine.media(&blob.sha256).unwrap().unwrap();
    f.engine
        .store
        .conn()
        .execute("DELETE FROM media WHERE sha256 = ?1", [&blob.sha256])
        .unwrap();
    f.ack(&reference)?;
    let waiting = s.read(&f.engine).unwrap();
    assert!(waiting.unsaved);
    assert!(!waiting.can_edit);
    assert!(waiting.edit_context.is_empty());
    assert!(waiting.save_error.is_some());
    assert_eq!(waiting.view, accepted.current.view);
    let cipher = f.cipher();
    assert!(apply(&f, &mut s, &read, "blocked", &[]).is_err());
    assert!(s.commit(&mut f.engine, "blocked").is_err());
    assert_eq!(f.cipher(), cipher);
    let later = f.signed_as(&f.owner, event.body.clone())?;
    deliver(&mut f.engine, &later, Ingest::Applied)?;
    assert!(matches!(
        f.engine
            .plan_timeline(&f.item)
            .unwrap()
            .slots
            .last()
            .unwrap()
            .acceptance,
        Acceptance::Waiting(_)
    ));
    f.engine
        .store
        .put_media(
            &blob.sha256,
            "application/vnd.zoen.page",
            b"damaged",
            1_800_000_000_000,
        )
        .unwrap();
    assert!(!s.read(&f.engine).unwrap().can_edit);
    assert_eq!(f.cipher(), cipher);
    f.engine
        .store
        .conn()
        .execute("DELETE FROM media WHERE sha256 = ?1", [&blob.sha256])
        .unwrap();
    f.engine
        .store
        .put_media(
            &blob.sha256,
            "application/vnd.zoen.page",
            &bytes,
            1_800_000_000_000,
        )
        .unwrap();
    let ready = s.read(&f.engine).unwrap();
    assert!(ready.can_edit);
    assert!(!ready.unsaved);
    assert_eq!(ready.view, accepted.current.view);
    assert!(journal(&s).commit.is_none());

    Ok(())
}

pub(super) fn newer_draft_survives_older_ack_and_permanent_transport_refusal(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let db = Database::new()?;
    let mut f = Fixture::at(&db.path())?;
    let mut s = f.session();
    let read = s.read(&f.engine).unwrap();
    let first = apply(
        &f,
        &mut s,
        &read,
        "first",
        &[text_edit(&read, "first published text")],
    )
    .unwrap();
    let first_commit = queued(s.commit(&mut f.engine, "first").unwrap());
    let newer = apply(
        &f,
        &mut s,
        &first.current,
        "newer",
        &[text_edit(&first.current, "newer text remains")],
    )
    .unwrap();
    f.ack(&first_commit)?;
    let current = s.read(&f.engine).unwrap();
    assert!(current.unsaved);
    assert!(journal(&s).commit.is_none());
    assert_eq!(current.view, newer.current.view);
    let second = queued(s.commit(&mut f.engine, "second").unwrap());
    let failed = f.pending(&second);
    assert!(f
        .engine
        .reject(
            &failed.client_id,
            "actual permanent transport refusal",
            true,
            None
        )
        .user_failed());
    let refused = s.read(&f.engine).unwrap();
    assert!(refused.unsaved);
    assert!(refused.save_error.as_ref().unwrap().contains("permanent"));
    drop(s);
    f = f.reopen();
    s = f.session();
    assert_eq!(s.read(&f.engine).unwrap().view, newer.current.view);
    let retry = queued(s.commit(&mut f.engine, "new validated source").unwrap());
    let event = f.pending(&retry);
    assert_ne!(event.client_id, failed.client_id);
    f.ack(&retry)?;
    let current = s.read(&f.engine).unwrap();
    assert!(!current.unsaved);
    assert_eq!(current.view, newer.current.view);
    let peer = f.peer()?;
    assert_eq!(
        peer.engine
            .plan_timeline(&peer.item)
            .unwrap()
            .document
            .unwrap()
            .blocks()
            .unwrap(),
        blocks(&current)
    );
    drop(s);
    drop(f);

    Ok(())
}
