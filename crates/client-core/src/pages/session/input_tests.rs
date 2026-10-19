use super::*;
use crate::pages::input::{PageInputDto, PageInputMode, PageInputTarget, MAX_CAPTURE};
use crate::FileEditDraftScope;

const WRITER_A: &str = "00000000-0000-4000-8000-000000000001";
const WRITER_B: &str = "00000000-0000-4000-8000-000000000002";

fn input_scope(f: &Fixture) -> FileEditDraftScope {
    let account = f.engine.account().unwrap();
    FileEditDraftScope {
        expected_identity_id: account.identity.clone(),
        expected_device_id: account.device.clone(),
        space_id: f.space.clone(),
        item_id: f.item.clone(),
    }
}

fn cold_input_fixture(kind: ItemKind) -> (Database, Fixture) {
    let (f, _, _, _) = private_document_backup_source(kind);
    let db = Database::new();
    // Materialize every actual account/key/journal row, including private rows
    // deliberately excluded by backup_snapshot, before destroying the Engine.
    f.engine
        .store
        .conn()
        .execute("VACUUM INTO ?1", [db.path()])
        .unwrap();
    let Fixture {
        engine,
        space,
        item,
        owner,
    } = f;
    let key = engine.author_for(&owner).unwrap().key.secret();
    drop(engine);
    let mut engine = Engine::open(&db.path()).unwrap();
    assert!(!engine.is_unlocked());
    assert!(engine.unlock(Some(key.to_vec())).unwrap());
    assert_eq!(engine.author_for(&owner).unwrap().key.secret(), key);
    (
        db,
        Fixture {
            engine,
            space,
            item,
            owner,
        },
    )
}

fn reopen_input_fixture(f: Fixture) -> Fixture {
    let key = f.engine.author_for(&f.owner).unwrap().key.secret();
    let mut reopened = f.reopen();
    assert!(!reopened.engine.is_unlocked());
    assert!(reopened.engine.unlock(Some(key.to_vec())).unwrap());
    assert_eq!(
        reopened
            .engine
            .author_for(&reopened.owner)
            .unwrap()
            .key
            .secret(),
        key
    );
    reopened
}

fn rich_payload(sequence: u64, text: &str) -> Vec<u8> {
    let end = text.encode_utf16().count();
    serde_json::to_vec(&serde_json::json!({
        "schema":"enrm-ios/1", "sequence":sequence.to_string(), "mode":"rich",
        "text":text, "selection":{"start":end,"end":end},
        "composition":null, "inline":[], "blocks":[],
        "typing":{"add":[],"remove":[]}
    }))
    .unwrap()
}

fn input_open(f: &Fixture, writer: &str) -> PageInputDto {
    let mut session = f.session();
    let read = session.read(&f.engine).unwrap();
    let target = match &read.view {
        DocumentView::BinaryPage { .. } => PageInputTarget::Block {
            block_id: f
                .engine
                .page(&f.item)
                .unwrap()
                .blocks
                .last()
                .unwrap()
                .id
                .clone(),
        },
        DocumentView::Plan { .. } => PageInputTarget::Block {
            block_id: line(&read).id,
        },
    };
    let current = f.engine.page_input_load(&input_scope(f)).unwrap();
    f.engine
        .page_input_open(
            &input_scope(f),
            current.revision,
            writer,
            target,
            PageInputMode::RawRich,
            &read.edit_context,
        )
        .unwrap()
}

fn journal_without_input(f: &Fixture) -> serde_json::Value {
    let session = f.session();
    let mut value = match &session.journal {
        SessionJournal::BinaryPage(j) => serde_json::to_value(j).unwrap(),
        SessionJournal::Plan(j) => serde_json::to_value(j).unwrap(),
    };
    value.as_object_mut().unwrap().remove("input");
    value
}

fn assert_input_metadata(f: &Fixture, input: &PageInputDto) {
    let (pending, revision) = match f.engine.item_state(&f.item).unwrap().kind {
        ItemKind::Page => {
            let page = f.engine.page(&f.item).unwrap();
            (page.input_pending, page.input_revision)
        }
        ItemKind::Plan => {
            let plan = f.engine.plan_page(&f.item).unwrap();
            (plan.input_pending, plan.input_revision)
        }
        _ => unreachable!(),
    };
    assert_eq!(pending, input.capture.is_some());
    assert_eq!(revision, input.revision.to_string());
}

fn input_journal_crypto(f: &Fixture) -> ([u8; 32], Vec<u8>, &'static [u8]) {
    let author = f.engine.author_for(&f.owner).unwrap();
    let mut key = [0u8; 32];
    hkdf::Hkdf::<sha2::Sha256>::new(Some(b"zoen.page.draft.v1"), &author.key.secret())
        .expand(b"local draft encryption", &mut key)
        .unwrap();
    let mut aad = serde_json::to_vec(&("zoen.page.draft.v1", &f.owner, &f.space, &f.item)).unwrap();
    match &f.session().tag {
        DocumentTag::BinaryPageV1 => (key, aad, b"binary-page"),
        DocumentTag::PlanV1(tag) => {
            for field in [
                b"\0plan.v1".as_slice(),
                tag.space.as_bytes(),
                tag.item.as_bytes(),
                tag.creation_event.as_bytes(),
            ] {
                aad.extend((field.len() as u64).to_be_bytes());
                aad.extend(field);
            }
            (key, aad, b"plan-v1")
        }
    }
}

fn store_input_fixture(f: &Fixture, ciphertext: &str) {
    let item = f.engine.item_state(&f.item).unwrap();
    f.engine
        .store
        .set_meta(&row_id(&f.engine, item).unwrap(), ciphertext)
        .unwrap();
    f.engine.pages.sessions.lock().unwrap().clear();
}

#[test]
fn page_and_plan_input_cold_reopen_preserve_exact_capture_and_private_journal() {
    for kind in [ItemKind::Page, ItemKind::Plan] {
        let (_db, mut f) = cold_input_fixture(kind);
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
}

#[test]
fn input_cas_sequence_and_reacquire_replay_exact_acceptance_without_replacing_newer_input() {
    let (_db, f) = cold_input_fixture(ItemKind::Page);
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
}

#[test]
fn input_apply_sql_failure_and_lost_ack_cold_reopen_have_one_atomic_acceptance() {
    let (_db, mut f) = cold_input_fixture(ItemKind::Page);
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
}

#[test]
fn input_discard_guards_and_publication_ack_preserve_private_document_and_future_capture() {
    for kind in [ItemKind::Page, ItemKind::Plan] {
        let (_db, mut f) = cold_input_fixture(kind);
        let mut session = f.session();
        let pending = queued(
            session
                .commit(&mut f.engine, "publication predates input")
                .unwrap(),
        );
        drop(session);
        let scope = input_scope(&f);
        let opened = input_open(&f, WRITER_A);
        let captured = f
            .engine
            .page_input_checkpoint(
                &scope,
                WRITER_A,
                opened.revision,
                7,
                &rich_payload(7, "discardable native input"),
            )
            .unwrap();
        let capture = captured.current.capture.as_ref().unwrap();
        let journal = journal_without_input(&f);
        let cipher = f.cipher();
        trigger(
            &f,
            "refuse_input_discard",
            "INSERT",
            "meta",
            "WHEN NEW.key LIKE 'page.draft.%'",
        );
        assert!(f
            .engine
            .page_input_discard(
                &scope,
                WRITER_A,
                captured.current.revision,
                7,
                &capture.sha256,
                "discard-A"
            )
            .is_err());
        assert_eq!(f.cipher(), cipher);
        assert_eq!(f.engine.page_input_load(&scope).unwrap(), captured.current);
        drop_trigger(&f, "refuse_input_discard");
        let mut s = f.session();
        let read = s.read(&f.engine).unwrap();
        assert!(
            private_document_backup_edit(&f, &mut s, &read, "old-apply", "late old editor")
                .is_err()
        );
        assert!(s.commit(&mut f.engine, "late old commit").is_err());
        if kind == ItemKind::Page {
            let old_page = f.engine.page(&f.item).unwrap();
            assert!(f
                .engine
                .page_apply_from(
                    &f.item,
                    "old-public-apply",
                    &old_page.edit_context,
                    &old_page
                        .blocks
                        .iter()
                        .map(|b| b.id.clone())
                        .collect::<Vec<_>>(),
                    &[]
                )
                .is_err());
            assert!(f.engine.page_commit(&f.item, "late public commit").is_err());
            assert!(f.engine.page_restore(&f.item, 1, "late restore").is_err());
            assert!(s.restore_binary(&f.engine, 1).is_err());
        } else {
            assert!(f
                .engine
                .page_input_apply(
                    &scope,
                    WRITER_A,
                    captured.current.revision,
                    7,
                    &capture.sha256,
                    "closed-Plan-apply"
                )
                .is_err());
        }
        assert_eq!(f.cipher(), cipher);
        assert_eq!(journal_without_input(&f), journal);
        // The real ACK for a prior Share refreshes canonical/shadow state without
        // consuming the native checkpoint retained after that Share.
        f.ack(&pending);
        assert_eq!(f.engine.page_input_load(&scope).unwrap(), captured.current);
        let after_ack = journal_without_input(&f);
        let view = f.session().read(&f.engine).unwrap().view;
        let discarded = f
            .engine
            .page_input_discard(
                &scope,
                WRITER_A,
                captured.current.revision,
                7,
                &capture.sha256,
                "discard-A",
            )
            .unwrap();
        assert!(!discarded.replayed);
        assert!(discarded.current.capture.is_none() && discarded.current.binding.is_none());
        assert_eq!(journal_without_input(&f), after_ack);
        f = reopen_input_fixture(f);
        assert_eq!(f.engine.page_input_load(&scope).unwrap(), discarded.current);
        assert_eq!(f.session().read(&f.engine).unwrap().view, view);
        let b = input_open(&f, WRITER_B);
        let b = f
            .engine
            .page_input_checkpoint(
                &scope,
                WRITER_B,
                b.revision,
                8,
                &rich_payload(8, "future B remains pending"),
            )
            .unwrap();
        let cipher = f.cipher();
        let replay = f
            .engine
            .page_input_discard(
                &scope,
                WRITER_A,
                captured.current.revision,
                7,
                &capture.sha256,
                "discard-A",
            )
            .unwrap();
        assert!(replay.replayed);
        assert_eq!(replay.accepted_revision, discarded.accepted_revision);
        assert_eq!(replay.current, b.current);
        assert_eq!(f.cipher(), cipher);
        assert_eq!(journal_without_input(&f), after_ack);
    }
}

#[test]
fn input_limits_foreign_scope_and_authenticated_journal_migration_fail_closed() {
    for kind in [ItemKind::Page, ItemKind::Plan] {
        let (_db, mut f) = cold_input_fixture(kind);
        let mut session = f.session();
        queued(
            session
                .commit(&mut f.engine, "migrate exact pending journal")
                .unwrap(),
        );
        drop(session);
        let original = journal_without_input(&f);
        let (key, aad, tag) = input_journal_crypto(&f);
        let literal = serde_json::to_vec(&original).unwrap();
        // Recreate the authentic pre-slot envelope with this actual device key,
        // origin AAD, snapshot, edit receipts and pending publication.
        let legacy = if kind == ItemKind::Page {
            write_legacy_fixture(&f.engine, f.engine.item_state(&f.item).unwrap(), &literal)
                .unwrap()
        } else {
            let sealed = crate::local_sealed::seal(&key, &aad, &literal).unwrap();
            format!("v2:{}", sealed.strip_prefix("v1:").unwrap())
        };
        store_input_fixture(&f, &legacy);
        f = reopen_input_fixture(f);
        let scope = input_scope(&f);
        let empty = f.engine.page_input_load(&scope).unwrap();
        assert_eq!(empty.revision, 0);
        assert!(empty.capture.is_none());
        assert_eq!(
            f.cipher().as_deref(),
            Some(legacy.as_str()),
            "authenticated read must not migrate by writing"
        );
        assert_eq!(journal_without_input(&f), original);
        let opened = input_open(&f, WRITER_A);
        assert!(f.cipher().unwrap().starts_with("v3:"));
        assert_eq!(journal_without_input(&f), original);
        let payload = rich_payload(7, "😀e\u{301}");
        let accepted = f
            .engine
            .page_input_checkpoint(&scope, WRITER_A, opened.revision, 7, &payload)
            .unwrap();
        let cipher = f.cipher();
        let journal = journal_without_input(&f);
        let mut invalids = vec![vec![b'x'; MAX_CAPTURE + 1]];
        let valid = rich_payload(8, "😀e\u{301}");
        for (field, replacement) in [
            ("schema", serde_json::json!("unknown/2")),
            ("mode", serde_json::json!("unknown-mode")),
            ("selection", serde_json::json!({"start":1,"end":2})),
            ("composition", serde_json::json!({"start":0,"end":1})),
            ("sequence", serde_json::json!("08")),
            (
                "inline",
                serde_json::json!([{"start":0,"end":2,"type":"strong","url":null,"origin":"future"}]),
            ),
        ] {
            let mut value: serde_json::Value = serde_json::from_slice(&valid).unwrap();
            value[field] = replacement;
            invalids.push(serde_json::to_vec(&value).unwrap());
        }
        let raw = String::from_utf8(valid.clone()).unwrap();
        invalids.push(format!("{{\"schema\":\"enrm-ios/1\",{}", &raw[1..]).into_bytes());
        let mut unknown: serde_json::Value = serde_json::from_slice(&valid).unwrap();
        unknown["future"] = serde_json::json!(true);
        invalids.push(serde_json::to_vec(&unknown).unwrap());
        let mut ranges: serde_json::Value = serde_json::from_slice(&valid).unwrap();
        ranges["inline"] = serde_json::Value::Array(vec![
            serde_json::json!({"start":0,"end":2,"type":"strong","url":null,"origin":"stored"});
            8193
        ]);
        invalids.push(serde_json::to_vec(&ranges).unwrap());
        for bytes in invalids {
            assert!(f
                .engine
                .page_input_checkpoint(&scope, WRITER_A, accepted.current.revision, 8, &bytes)
                .is_err());
            assert_eq!(f.cipher(), cipher);
            assert_eq!(f.engine.page_input_load(&scope).unwrap(), accepted.current);
        }
        let mut scopes = vec![scope.clone(); 4];
        scopes[0].expected_identity_id = "foreign-identity".into();
        scopes[1].expected_device_id = "foreign-device".into();
        scopes[2].space_id = "foreign-space".into();
        scopes[3].item_id = "foreign-item".into();
        for foreign in scopes {
            assert!(f.engine.page_input_load(&foreign).is_err());
            assert!(f
                .engine
                .page_input_checkpoint(&foreign, WRITER_A, accepted.current.revision, 8, &valid)
                .is_err());
            assert_eq!(f.cipher(), cipher);
        }
        assert!(f
            .engine
            .page_input_open(
                &scope,
                accepted.current.revision,
                WRITER_B,
                accepted.current.binding.as_ref().unwrap().target.clone(),
                PageInputMode::Markdown,
                &accepted.current.binding.as_ref().unwrap().base_edit_context
            )
            .is_err());
        let unlocked = f.engine.net.author.take();
        let root_key = f.engine.signers.remove(&f.owner);
        assert!(f.engine.page_input_load(&scope).is_err());
        assert_eq!(f.cipher(), cipher);
        f.engine.net.author = unlocked;
        if let Some(root_key) = root_key {
            f.engine.signers.insert(f.owner.clone(), root_key);
        }
        assert_eq!(journal_without_input(&f), journal);
        // Backup still excludes the whole private row after it gains input bytes.
        let backup = Database::new();
        std::fs::write(backup.path(), f.engine.backup_snapshot().unwrap()).unwrap();
        let conn = rusqlite::Connection::open(backup.path()).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM meta WHERE key LIKE 'page.draft.%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
        drop(conn);
        assert_eq!(f.cipher(), cipher);
        if kind == ItemKind::Page {
            // Both inputs fit alone. Their aggregate must be refused without
            // replacing the genuine, already retained native capture.
            let large_payload = rich_payload(8, &"x".repeat(MAX_CAPTURE - 1024));
            assert!(large_payload.len() <= MAX_CAPTURE);
            let large_ack = f
                .engine
                .page_input_checkpoint(
                    &scope,
                    WRITER_A,
                    accepted.current.revision,
                    8,
                    &large_payload,
                )
                .unwrap();
            let large_cipher = f.cipher();
            let s = f.session();
            let page = s.binary().unwrap();
            let huge = page.at(&page.frontiers()).unwrap();
            let mut seed = 0x9e3779b97f4a7c15u64;
            let alphabet = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_-";
            let description: String = (0..1024 * 1024)
                .map(|_| {
                    seed ^= seed << 13;
                    seed ^= seed >> 7;
                    seed ^= seed << 17;
                    alphabet[(seed & 63) as usize] as char
                })
                .collect();
            let blocks = huge.blocks();
            let id = new_id("oversized-input-block");
            let mut order: Vec<_> = blocks.iter().map(|b| b.id.clone()).collect();
            order.push(id.clone());
            huge.apply(
                &order,
                &[documents::BlockEdit {
                    // Image metadata uses a real CRDT value without creating
                    // millions of rich-text cursor IDs unrelated to this bound.
                    id,
                    kind: Kind::Image {
                        url: "https://example.test/large-asset.png".into(),
                        alt: description,
                        title: String::new(),
                    },
                    text: String::new(),
                    spans: vec![],
                }],
            )
            .unwrap();
            let SessionJournal::BinaryPage(mut candidate) = s.journal else {
                unreachable!()
            };
            candidate.snapshot = Some(encode_bytes(&huge.snapshot()));
            let retained_input = std::mem::take(&mut candidate.input);
            let without_input_bytes = serde_json::to_vec(&candidate).unwrap().len();
            assert!(
                without_input_bytes < MAX_JOURNAL_BYTES,
                "snapshot alone: {without_input_bytes}"
            );
            candidate.input = retained_input;
            let aggregate_bytes = serde_json::to_vec(&candidate).unwrap().len();
            assert!(
                aggregate_bytes > MAX_JOURNAL_BYTES,
                "aggregate: {aggregate_bytes}, snapshot alone: {without_input_bytes}"
            );
            assert!(f
                .engine
                .save_page_journal(f.engine.item_state(&f.item).unwrap(), &candidate)
                .is_err());
            assert_eq!(f.cipher(), large_cipher);
            assert_eq!(f.engine.page_input_load(&scope).unwrap(), large_ack.current);
        }
        if kind == ItemKind::Plan {
            let mut foreign_origin = original.clone();
            foreign_origin["tag"]["creation_event"] =
                serde_json::json!("another-authenticated-creation");
            let foreign_origin = seal_v3(
                &key,
                &aad,
                &serde_json::to_vec(&foreign_origin).unwrap(),
                tag,
            )
            .unwrap();
            store_input_fixture(&f, &foreign_origin);
            f = reopen_input_fixture(f);
            assert!(f.engine.page_input_load(&scope).is_err());
            assert_eq!(f.cipher().as_deref(), Some(foreign_origin.as_str()));
        }
        // Unknown fields in an authenticated v3 future slot stay locked and
        // intact instead of falling back to an empty/default journal.
        let mut future = original.clone();
        future["input"] = serde_json::json!({"revision":0,"binding":null,"capture":null,"receipts":[],"future_format":2});
        let future = seal_v3(&key, &aad, &serde_json::to_vec(&future).unwrap(), tag).unwrap();
        store_input_fixture(&f, &future);
        f = reopen_input_fixture(f);
        let mut s = f.session();
        let read = s.read(&f.engine).unwrap();
        assert!(!read.can_edit);
        assert!(matches!(s.blocked, Some(SessionBlock::DraftLocked(_))));
        assert!(f.engine.page_input_load(&scope).is_err());
        assert_eq!(f.cipher().as_deref(), Some(future.as_str()));
        // Restore the genuine retained row, then revoke writing through a real
        // ordered membership event rather than mutating a cached permission.
        store_input_fixture(&f, cipher.as_deref().unwrap());
        f = reopen_input_fixture(f);
        f.ordered(EventBody::MemberAdded {
            identity: f.owner.clone(),
            role: Role::Reader,
        });
        assert!(f.engine.page_input_load(&scope).is_err());
        assert!(f
            .engine
            .page_input_checkpoint(&scope, WRITER_A, accepted.current.revision, 8, &valid)
            .is_err());
        assert_eq!(f.cipher(), cipher);
    }
}

#[test]
fn release_empty_binding_is_atomic_durable_and_replays_without_dropping_newer_input() {
    for kind in [ItemKind::Page, ItemKind::Plan] {
        let (_db, mut f) = cold_input_fixture(kind);
        let mut session = f.session();
        let pending = queued(
            session
                .commit(&mut f.engine, "publication before empty release")
                .unwrap(),
        );
        drop(session);
        let scope = input_scope(&f);
        let journal = journal_without_input(&f);
        let private = f.session().read(&f.engine).unwrap().view;
        let pending_event = serde_json::to_vec(&f.pending(&pending)).unwrap();
        let events = f.engine.store.event_count().unwrap();
        let outbox = f.engine.store.outbox_len().unwrap();
        let head = f.engine.logs[&f.space].head().map(|h| (h.seq, h.hash));
        let legacy_cipher = f.cipher();
        assert!(legacy_cipher
            .as_ref()
            .unwrap()
            .starts_with(if kind == ItemKind::Page { "v1:" } else { "v2:" }));
        assert!(f
            .engine
            .page_input_release_empty(&scope, WRITER_A, 0, "no-binding-to-release")
            .is_err());
        assert_eq!(
            f.cipher(),
            legacy_cipher,
            "an absent binding cannot migrate or rewrite a legacy journal"
        );

        let opened = input_open(&f, WRITER_A);
        assert!(opened.capture.is_none());
        assert_input_metadata(&f, &opened);
        let opened_cipher = f.cipher();
        trigger(
            &f,
            "refuse_empty_release",
            "INSERT",
            "meta",
            "WHEN NEW.key LIKE 'page.draft.%'",
        );
        // A first native retention failure leaves an empty accepted slot. Release
        // needs no invented payload, sequence, or capture digest.
        assert!(f
            .engine
            .page_input_checkpoint(
                &scope,
                WRITER_A,
                opened.revision,
                7,
                &rich_payload(7, "not accepted because SQLite refused the write")
            )
            .is_err());
        assert_eq!(f.engine.page_input_load(&scope).unwrap(), opened);
        assert!(f
            .engine
            .page_input_discard(
                &scope,
                WRITER_A,
                opened.revision,
                0,
                "",
                "no-sentinel-discard"
            )
            .is_err());
        assert!(f
            .engine
            .page_input_release_empty(&scope, WRITER_A, opened.revision, "empty-release-A")
            .is_err());
        assert_eq!(f.cipher(), opened_cipher);
        assert_eq!(f.engine.page_input_load(&scope).unwrap(), opened);
        assert_eq!(journal_without_input(&f), journal);
        assert_eq!(f.session().read(&f.engine).unwrap().view, private);
        drop_trigger(&f, "refuse_empty_release");

        f = reopen_input_fixture(f);
        assert_eq!(f.engine.page_input_load(&scope).unwrap(), opened);
        let accepted = f
            .engine
            .page_input_release_empty(&scope, WRITER_A, opened.revision, "empty-release-A")
            .unwrap();
        assert!(!accepted.replayed);
        assert_eq!(accepted.accepted_revision, opened.revision + 1);
        assert_eq!(accepted.current.revision, accepted.accepted_revision);
        assert!(accepted.current.binding.is_none() && accepted.current.capture.is_none());
        assert!(f.cipher().unwrap().starts_with("v3:"));
        assert_eq!(journal_without_input(&f), journal);
        assert_input_metadata(&f, &accepted.current);
        let accepted_cipher = f.cipher();

        // Treat the first ACK as lost, destroy the Engine, unlock the same actual
        // device against the file, and replay the exact request.
        f = reopen_input_fixture(f);
        let replay = f
            .engine
            .page_input_release_empty(&scope, WRITER_A, opened.revision, "empty-release-A")
            .unwrap();
        assert!(replay.replayed);
        assert_eq!(replay.accepted_revision, accepted.accepted_revision);
        assert_eq!(replay.current, accepted.current);
        assert_eq!(f.cipher(), accepted_cipher);

        let b = input_open(&f, WRITER_B);
        let before = f.cipher();
        let old_ack = f
            .engine
            .page_input_release_empty(&scope, WRITER_A, opened.revision, "empty-release-A")
            .unwrap();
        assert!(old_ack.replayed);
        assert_eq!(old_ack.accepted_revision, accepted.accepted_revision);
        assert_eq!(old_ack.current, b);
        assert_eq!(f.cipher(), before);
        for (writer, revision, mutation) in [
            (WRITER_A, opened.revision, "stale-release-A"),
            (WRITER_A, b.revision, "wrong-current-writer"),
            (WRITER_B, accepted.current.revision, "stale-release-B"),
            ("invalid writer", b.revision, "valid-mutation"),
            (WRITER_B, b.revision, "invalid mutation"),
            (WRITER_B, b.revision, ""),
        ] {
            assert!(f
                .engine
                .page_input_release_empty(&scope, writer, revision, mutation)
                .is_err());
            assert_eq!(f.cipher(), before);
            assert_eq!(f.engine.page_input_load(&scope).unwrap(), b);
        }
        let b = f
            .engine
            .page_input_checkpoint(
                &scope,
                WRITER_B,
                b.revision,
                8,
                &rich_payload(8, "accepted B must require exact capture discard"),
            )
            .unwrap();
        let before = f.cipher();
        assert!(f
            .engine
            .page_input_release_empty(&scope, WRITER_B, b.current.revision, "B-is-not-empty")
            .is_err());
        assert_eq!(f.cipher(), before);
        let old_ack = f
            .engine
            .page_input_release_empty(&scope, WRITER_A, opened.revision, "empty-release-A")
            .unwrap();
        assert!(old_ack.replayed);
        assert_eq!(old_ack.accepted_revision, accepted.accepted_revision);
        assert_eq!(old_ack.current, b.current);
        assert_eq!(f.cipher(), before);
        assert_input_metadata(&f, &b.current);
        f = reopen_input_fixture(f);
        assert_eq!(f.engine.page_input_load(&scope).unwrap(), b.current);
        assert_input_metadata(&f, &b.current);
        assert_eq!(journal_without_input(&f), journal);
        assert_eq!(f.session().read(&f.engine).unwrap().view, private);
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
    }
}

#[test]
fn release_empty_requires_the_current_scope_key_and_writing_membership() {
    for kind in [ItemKind::Page, ItemKind::Plan] {
        let (_db, mut f) = cold_input_fixture(kind);
        let scope = input_scope(&f);
        let opened = input_open(&f, WRITER_A);
        let cipher = f.cipher();
        let journal = journal_without_input(&f);
        let mut foreign = vec![scope.clone(); 4];
        foreign[0].expected_identity_id = "another-identity".into();
        foreign[1].expected_device_id = "another-device".into();
        foreign[2].space_id = "another-space".into();
        foreign[3].item_id = "another-item".into();
        for scope in foreign {
            assert!(f
                .engine
                .page_input_release_empty(&scope, WRITER_A, opened.revision, "foreign-release")
                .is_err());
            assert_eq!(f.cipher(), cipher);
        }
        let unlocked = f.engine.net.author.take();
        let root_key = f.engine.signers.remove(&f.owner);
        assert!(f
            .engine
            .page_input_release_empty(&scope, WRITER_A, opened.revision, "locked-release")
            .is_err());
        assert_eq!(f.cipher(), cipher);
        f.engine.net.author = unlocked;
        if let Some(root_key) = root_key {
            f.engine.signers.insert(f.owner.clone(), root_key);
        }
        f.engine
            .store
            .set_meta("provider_signup_pending", "1")
            .unwrap();
        assert!(f
            .engine
            .page_input_release_empty(&scope, WRITER_A, opened.revision, "setup-pending-release")
            .is_err());
        assert_eq!(f.cipher(), cipher);
        f.engine
            .store
            .conn()
            .execute("DELETE FROM meta WHERE key='provider_signup_pending'", [])
            .unwrap();
        assert_eq!(f.engine.page_input_load(&scope).unwrap(), opened);
        f.ordered(EventBody::MemberAdded {
            identity: f.owner.clone(),
            role: Role::Reader,
        });
        let events = f.engine.store.event_count().unwrap();
        let outbox = f.engine.store.outbox_len().unwrap();
        assert!(f
            .engine
            .page_input_release_empty(&scope, WRITER_A, opened.revision, "reader-release")
            .is_err());
        assert_eq!(f.cipher(), cipher);
        assert_eq!(journal_without_input(&f), journal);
        assert_eq!(f.engine.store.event_count().unwrap(), events);
        assert_eq!(f.engine.store.outbox_len().unwrap(), outbox);
    }
}

#[test]
fn plan_input_metadata_survives_pending_legacy_without_admitting_empty_release() {
    for captured in [false, true] {
        let (_db, mut f) = cold_input_fixture(ItemKind::Plan);
        // A confirmed legacy Plan with no applied CRDT draft can still hold
        // private native input; checkpointing must not change has_draft.
        f.item = new_id("confirmed-input-plan");
        f.ordered(EventBody::ItemCreated {
            item: f.item.clone(),
            kind: ItemKind::Plan,
            content: ItemContent::Plan(original()),
            origin: "confirmed metadata fixture".into(),
        });
        let scope = input_scope(&f);
        let opened = input_open(&f, WRITER_A);
        let input = if captured {
            f.engine
                .page_input_checkpoint(
                    &scope,
                    WRITER_A,
                    opened.revision,
                    7,
                    &rich_payload(7, "private input behind pending legacy projection"),
                )
                .unwrap()
                .current
        } else {
            opened
        };
        assert_input_metadata(&f, &input);
        assert!(!f.engine.plan_page(&f.item).unwrap().unsaved);
        let mut pending = original();
        pending.title = "local legacy version remains pending".into();
        f.queue(f.version(ItemContent::Plan(pending)));
        let cipher = f.cipher();
        let read = f.engine.plan_page(&f.item).unwrap();
        assert_eq!(
            read.source,
            crate::pages::PlanPageReadSourceDto::LocalPendingLegacy
        );
        assert!(!read.unsaved);
        assert_eq!(read.input_pending, captured);
        assert_eq!(read.input_revision, input.revision.to_string());
        let outbox = f.engine.store.outbox_len().unwrap();
        assert!(f
            .engine
            .page_input_release_empty(&scope, WRITER_A, input.revision, "pending-legacy-release")
            .is_err());
        assert_eq!(f.cipher(), cipher);
        assert_eq!(f.engine.store.outbox_len().unwrap(), outbox);
        f = reopen_input_fixture(f);
        assert_input_metadata(&f, &input);
        assert_eq!(f.cipher(), cipher);

        // A purely optimistic creation has no authenticated document session.
        f.item = new_id("pending-input-plan");
        f.queue(EventBody::ItemCreated {
            item: f.item.clone(),
            kind: ItemKind::Plan,
            content: ItemContent::Plan(original()),
            origin: "pending metadata fixture".into(),
        });
        let read = f.engine.plan_page(&f.item).unwrap();
        assert_eq!(
            read.source,
            crate::pages::PlanPageReadSourceDto::LocalPendingLegacy
        );
        assert!(!read.input_pending);
        assert_eq!(read.input_revision, "0");
        let scope = input_scope(&f);
        assert!(f
            .engine
            .page_input_release_empty(&scope, WRITER_A, 0, "no-pending-authority")
            .is_err());
        assert!(f.cipher().is_none());
    }
}
