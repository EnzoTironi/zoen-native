//! Complete capture acceptance scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn lossless_input_cold_recovery_preserves_kind_marks_and_exact_retry(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (_db, mut f, opened, mut block) = lossless_fixture()?;
    let scope = input_scope(&f);
    let before_page = f.engine.page(&f.item).unwrap();
    let before_events = f.engine.store.event_count().unwrap();
    let before_outbox = f.engine.store.outbox_len().unwrap();
    block.text = format!("Before {}!", block.text);
    for span in &mut block.spans {
        span.start += 7;
        span.end += 7;
    }
    let payload = lossless_payload(7, &block);
    let ack = f
        .engine
        .page_input_checkpoint(&scope, WRITER_A, opened.revision, 7, &payload)
        .unwrap();
    assert_eq!(
        ack.current.capture.as_ref().unwrap().payload,
        payload,
        "recovery retains the original native bytes"
    );
    assert_eq!(f.engine.page(&f.item).unwrap().blocks, before_page.blocks);
    assert!(
        f.engine
            .page_input_checkpoint(&scope, WRITER_A, opened.revision, 7, &payload)
            .unwrap()
            .replayed
    );
    f = reopen_input_fixture(f);
    let recovered = f.engine.page_input_load(&scope).unwrap();
    assert_eq!(recovered, ack.current);
    let capture = recovered.capture.as_ref().unwrap().clone();
    let resumed = f
        .engine
        .page_input_open(
            &scope,
            recovered.revision,
            WRITER_B,
            PageInputTarget::Block {
                block_id: block.id.clone(),
            },
            PageInputMode::LosslessText,
            &before_page.edit_context,
        )
        .unwrap();
    let applied = f
        .engine
        .page_input_apply(
            &scope,
            WRITER_B,
            resumed.revision,
            capture.sequence,
            &capture.sha256,
            "apply-recovered-lossless",
        )
        .unwrap();
    assert_eq!(
        applied.edit.page.blocks.iter().find(|b| b.id == block.id),
        Some(&block)
    );
    assert!(applied.ack.current.capture.is_none());
    assert!(applied.ack.current.binding.is_none());
    f = reopen_input_fixture(f);
    let accepted_page = f.engine.page(&f.item).unwrap();
    let retry = f
        .engine
        .page_input_apply(
            &scope,
            WRITER_B,
            resumed.revision,
            capture.sequence,
            &capture.sha256,
            "apply-recovered-lossless",
        )
        .unwrap();
    assert!(retry.ack.replayed && retry.edit.replayed);
    assert_eq!(retry.edit.page.blocks, accepted_page.blocks);
    assert_eq!(f.engine.store.event_count().unwrap(), before_events);
    assert_eq!(f.engine.store.outbox_len().unwrap(), before_outbox);

    Ok(())
}

pub(super) fn lossless_input_rejects_malformed_ranges_without_replacing_recovery(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (_db, mut f, opened, block) = lossless_fixture()?;
    let scope = input_scope(&f);
    let initial = lossless_payload(7, &block);
    let accepted = f
        .engine
        .page_input_checkpoint(&scope, WRITER_A, opened.revision, 7, &initial)
        .unwrap();
    let cipher = f.cipher();
    let mut cases = Vec::new();
    let mut reversed_range = block.clone();
    reversed_range.spans[0].start = 3;
    cases.push(reversed_range);
    let mut out_of_bounds = block.clone();
    out_of_bounds.spans[0].end = 100;
    cases.push(out_of_bounds);
    let mut split_surrogate = block.clone();
    split_surrogate.spans[0].start = 1;
    cases.push(split_surrogate);
    let mut unknown_mark = block.clone();
    unknown_mark.spans[0].key = "future-mark".into();
    cases.push(unknown_mark);
    for changed in cases {
        assert!(f
            .engine
            .page_input_checkpoint(
                &scope,
                WRITER_A,
                accepted.current.revision,
                8,
                &lossless_payload(8, &changed),
            )
            .is_err());
        assert_eq!(f.cipher(), cipher);
        assert_eq!(f.engine.page_input_load(&scope).unwrap(), accepted.current);
    }
    f = reopen_input_fixture(f);
    assert_eq!(f.engine.page_input_load(&scope).unwrap(), accepted.current);

    Ok(())
}

pub(super) fn page_and_plan_input_cold_reopen_preserve_exact_capture_and_private_journal(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    for kind in [ItemKind::Page, ItemKind::Plan] {
        let (_db, mut f) = cold_input_fixture(kind)?;
        let mut session = f.session();
        let pending = queued(
            session
                .commit(&mut f.engine, "pending before native capture")
                .unwrap(),
        );
        let pending_event = serde_json::to_vec(&f.pending(&pending)).unwrap();
        let private = session.read(&f.engine).unwrap().view;
        drop(session);
        let journal = journal_without_input(&f);
        let events = f.engine.store.event_count().unwrap();
        let outbox = f.engine.store.outbox_len().unwrap();
        let head = f.engine.logs[&f.space].head().map(|h| (h.seq, h.hash));
        let opened = input_open(&f, WRITER_A);
        let mut value: serde_json::Value =
            serde_json::from_slice(&rich_payload(7, "😀e\u{301} marked 雨")).unwrap();
        value["composition"] = serde_json::json!({"start":4,"end":13});
        value["inline"] = serde_json::json!([
            {"start":0,"end":4,"type":"strong","url":null,"origin":"stored"},
            {"start":4,"end":13,"type":"link","url":"https://example.test/雨","origin":"detected"}
        ]);
        // Whitespace belongs to the checkpoint bytes; restoration must not reserialize.
        let payload = serde_json::to_vec_pretty(&value).unwrap();
        let ack = f
            .engine
            .page_input_checkpoint(&input_scope(&f), WRITER_A, opened.revision, 7, &payload)
            .unwrap();
        assert!(!ack.replayed);
        assert_input_metadata(&f, &ack.current);
        assert_eq!(ack.current.capture.as_ref().unwrap().payload, payload);
        assert_eq!(journal_without_input(&f), journal);
        assert_eq!(f.engine.store.event_count().unwrap(), events);
        assert_eq!(f.engine.store.outbox_len().unwrap(), outbox);
        assert_eq!(
            f.engine.logs[&f.space].head().map(|h| (h.seq, h.hash)),
            head
        );
        assert_eq!(
            serde_json::to_vec(&f.pending(&pending)).unwrap(),
            pending_event
        );
        f = reopen_input_fixture(f);
        assert_eq!(
            f.engine.page_input_load(&input_scope(&f)).unwrap(),
            ack.current
        );
        assert_eq!(journal_without_input(&f), journal);
        assert_eq!(f.session().read(&f.engine).unwrap().view, private);
        assert_input_metadata(&f, &ack.current);
        assert!(f.cipher().unwrap().starts_with("v3:"));
    }
    Ok(())
}

pub(super) fn input_cas_sequence_and_reacquire_replay_exact_acceptance_without_replacing_newer_input(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (_db, f) = cold_input_fixture(ItemKind::Page)?;
    let scope = input_scope(&f);
    let opened = input_open(&f, WRITER_A);
    let payload = rich_payload(7, "lease A 😀e\u{301}");
    let first = f
        .engine
        .page_input_checkpoint(&scope, WRITER_A, opened.revision, 7, &payload)
        .unwrap();
    let cipher = f.cipher();
    let replay = f
        .engine
        .page_input_checkpoint(&scope, WRITER_A, opened.revision, 7, &payload)
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.accepted_revision, first.accepted_revision);
    assert_eq!(replay.current, first.current);
    assert_eq!(f.cipher(), cipher);
    for (revision, sequence, bytes) in [
        (
            first.current.revision,
            7,
            rich_payload(7, "same sequence, different bytes"),
        ),
        (first.current.revision, 6, rich_payload(6, "late")),
        (opened.revision, 8, rich_payload(8, "unknown CAS")),
        (
            first.current.revision,
            8,
            rich_payload(9, "mismatched envelope sequence"),
        ),
    ] {
        assert!(f
            .engine
            .page_input_checkpoint(&scope, WRITER_A, revision, sequence, &bytes)
            .is_err());
        assert_eq!(f.cipher(), cipher);
    }
    let binding = first.current.binding.as_ref().unwrap();
    assert!(f
        .engine
        .page_input_open(
            &scope,
            first.current.revision,
            WRITER_B,
            PageInputTarget::AppendNew,
            PageInputMode::RawRich,
            &binding.base_edit_context
        )
        .is_err());
    let acquired = f
        .engine
        .page_input_open(
            &scope,
            first.current.revision,
            WRITER_B,
            binding.target.clone(),
            binding.mode.clone(),
            &binding.base_edit_context,
        )
        .unwrap();
    assert_eq!(acquired.revision, first.current.revision + 1);
    assert_eq!(acquired.capture, first.current.capture);
    let rebound = acquired.binding.as_ref().unwrap();
    assert_eq!(rebound.writer_id, WRITER_B);
    assert_eq!(rebound.base_block_json, binding.base_block_json);
    assert_eq!(rebound.base_edit_context, binding.base_edit_context);
    let before = f.cipher();
    assert!(f
        .engine
        .page_input_checkpoint(
            &scope,
            WRITER_A,
            acquired.revision,
            8,
            &rich_payload(8, "revoked writer")
        )
        .is_err());
    assert!(f
        .engine
        .page_input_discard(
            &scope,
            WRITER_A,
            acquired.revision,
            7,
            &acquired.capture.as_ref().unwrap().sha256,
            "late-A-discard"
        )
        .is_err());
    assert!(f
        .engine
        .page_input_checkpoint(
            &scope,
            WRITER_B,
            acquired.revision,
            0,
            &rich_payload(0, "counter reset")
        )
        .is_err());
    assert_eq!(f.cipher(), before);
    let next = f
        .engine
        .page_input_checkpoint(
            &scope,
            WRITER_B,
            acquired.revision,
            8,
            &rich_payload(8, "lease B continues at eight"),
        )
        .unwrap();
    let before = f.cipher();
    let old_ack = f
        .engine
        .page_input_checkpoint(&scope, WRITER_A, opened.revision, 7, &payload)
        .unwrap();
    assert!(old_ack.replayed);
    assert_eq!(old_ack.accepted_revision, first.accepted_revision);
    assert_eq!(old_ack.current, next.current);
    assert_eq!(f.cipher(), before);
    let mut revision = next.current.revision;
    for sequence in 9..=24 {
        revision = f
            .engine
            .page_input_checkpoint(
                &scope,
                WRITER_B,
                revision,
                sequence,
                &rich_payload(sequence, "bounded receipt history"),
            )
            .unwrap()
            .current
            .revision;
    }
    let before = f.cipher();
    assert!(f
        .engine
        .page_input_checkpoint(&scope, WRITER_A, opened.revision, 7, &payload)
        .is_err());
    assert_eq!(
        f.cipher(),
        before,
        "eviction must reject rather than infer an old acceptance"
    );

    Ok(())
}

pub(super) fn input_apply_sql_failure_and_lost_ack_cold_reopen_have_one_atomic_acceptance(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (_db, mut f) = cold_input_fixture(ItemKind::Page)?;
    let mut session = f.session();
    let pending = queued(
        session
            .commit(&mut f.engine, "earlier publication remains in flight")
            .unwrap(),
    );
    drop(session);
    let scope = input_scope(&f);
    let opened = input_open(&f, WRITER_A);
    let text = "Applied native 😀 e\u{301}\nsecond paragraph line";
    let payload = rich_payload(11, text);
    let captured = f
        .engine
        .page_input_checkpoint(&scope, WRITER_A, opened.revision, 11, &payload)
        .unwrap();
    let capture = captured.current.capture.as_ref().unwrap();
    let before_journal = journal_without_input(&f);
    let before_page = f.engine.page(&f.item).unwrap();
    let before_cipher = f.cipher();
    let events = f.engine.store.event_count().unwrap();
    let outbox = f.engine.store.outbox_len().unwrap();
    let pending_event = serde_json::to_vec(&f.pending(&pending)).unwrap();
    trigger(
        &f,
        "refuse_input_apply",
        "INSERT",
        "meta",
        "WHEN NEW.key LIKE 'page.draft.%'",
    );
    assert!(f
        .engine
        .page_input_apply(
            &scope,
            WRITER_A,
            captured.current.revision,
            11,
            &capture.sha256,
            "native-apply-A"
        )
        .is_err());
    assert_eq!(f.cipher(), before_cipher);
    assert_eq!(f.engine.page_input_load(&scope).unwrap(), captured.current);
    assert_eq!(journal_without_input(&f), before_journal);
    assert_eq!(f.engine.page(&f.item).unwrap().blocks, before_page.blocks);
    drop_trigger(&f, "refuse_input_apply");
    f = reopen_input_fixture(f);
    assert_eq!(f.engine.page_input_load(&scope).unwrap(), captured.current);
    f.engine
        .with_input(&scope, |session| {
            session.lose_apply_response = true;
            Ok(())
        })
        .unwrap();
    assert!(f
        .engine
        .page_input_apply(
            &scope,
            WRITER_A,
            captured.current.revision,
            11,
            &capture.sha256,
            "native-apply-A"
        )
        .is_err());
    f = reopen_input_fixture(f);
    let consumed = f.engine.page_input_load(&scope).unwrap();
    assert!(consumed.capture.is_none() && consumed.binding.is_none());
    let accepted_journal = journal_without_input(&f);
    assert_eq!(
        accepted_journal["generation"].as_u64().unwrap(),
        before_journal["generation"].as_u64().unwrap() + 1
    );
    assert_eq!(
        accepted_journal["receipts"].as_array().unwrap().len(),
        before_journal["receipts"].as_array().unwrap().len() + 1
    );
    assert_eq!(accepted_journal["commit"], before_journal["commit"]);
    let accepted_page = f.engine.page(&f.item).unwrap();
    assert!(accepted_page
        .blocks
        .iter()
        .any(|b| b.text == text.replace('\n', "\u{2028}")));
    assert!(!f
        .engine
        .page_at(&f.item, 1)
        .unwrap()
        .blocks
        .iter()
        .any(|b| b.text == text.replace('\n', "\u{2028}")));
    assert_eq!(f.engine.store.event_count().unwrap(), events);
    assert_eq!(f.engine.store.outbox_len().unwrap(), outbox);
    assert_eq!(
        serde_json::to_vec(&f.pending(&pending)).unwrap(),
        pending_event
    );
    let b = input_open(&f, WRITER_B);
    let b = f
        .engine
        .page_input_checkpoint(
            &scope,
            WRITER_B,
            b.revision,
            12,
            &rich_payload(12, "future capture B"),
        )
        .unwrap();
    let before = f.cipher();
    let replay = f
        .engine
        .page_input_apply(
            &scope,
            WRITER_A,
            captured.current.revision,
            11,
            &capture.sha256,
            "native-apply-A",
        )
        .unwrap();
    assert!(replay.ack.replayed && replay.edit.replayed);
    assert_eq!(replay.ack.accepted_revision, consumed.revision);
    assert_eq!(replay.ack.current, b.current);
    assert_eq!(replay.edit.page.blocks, accepted_page.blocks);
    assert_eq!(journal_without_input(&f), accepted_journal);
    assert_eq!(f.cipher(), before);

    Ok(())
}
