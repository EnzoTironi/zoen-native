//! Shared fixture operations preserve the original clients, identities and stores.

use super::*;

pub(super) fn input_scope(f: &Fixture) -> FileEditDraftScope {
    let account = f.engine.account().unwrap();
    FileEditDraftScope {
        expected_identity_id: account.identity.clone(),
        expected_device_id: account.device.clone(),
        space_id: f.space.clone(),
        item_id: f.item.clone(),
    }
}

pub(super) fn cold_input_fixture(
    kind: ItemKind,
) -> std::result::Result<(Database, Fixture), Box<dyn std::error::Error>> {
    let (f, _, _, _) = private_document_backup_source(kind);
    let db = Database::new()?;
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
    Ok((
        db,
        Fixture {
            engine,
            space,
            item,
            owner,
        },
    ))
}

pub(super) fn reopen_input_fixture(f: Fixture) -> Fixture {
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

pub(super) fn rich_payload(sequence: u64, text: &str) -> Vec<u8> {
    let end = text.encode_utf16().count();
    serde_json::to_vec(&serde_json::json!({
        "schema":"enrm-ios/1", "sequence":sequence.to_string(), "mode":"rich",
        "text":text, "selection":{"start":end,"end":end},
        "composition":null, "inline":[], "blocks":[],
        "typing":{"add":[],"remove":[]}
    }))
    .unwrap()
}

pub(super) fn lossless_fixture() -> std::result::Result<
    (Database, Fixture, PageInputDto, crate::pages::PageBlockDto),
    Box<dyn std::error::Error>,
> {
    let (db, f) = cold_input_fixture(ItemKind::Page)?;
    let page = f.engine.page(&f.item).unwrap();
    let mut block = page.blocks.last().unwrap().clone();
    block.kind = "quote".into();
    block.text = "😀 e\u{301} link".into();
    block.spans = vec![
        crate::pages::TextSpanDto {
            start: 0,
            end: 2,
            key: "b".into(),
            value: String::new(),
        },
        crate::pages::TextSpanDto {
            start: 6,
            end: 10,
            key: "a".into(),
            value: "https://example.test/雨?q=retained".into(),
        },
    ];
    let seeded = f
        .engine
        .page_apply_from(
            &f.item,
            "seed-lossless-block",
            &page.edit_context,
            &page.blocks.iter().map(|b| b.id.clone()).collect::<Vec<_>>(),
            std::slice::from_ref(&block),
        )
        .unwrap();
    let current = f.engine.page_input_load(&input_scope(&f)).unwrap();
    let opened = f
        .engine
        .page_input_open(
            &input_scope(&f),
            current.revision,
            WRITER_A,
            PageInputTarget::Block {
                block_id: block.id.clone(),
            },
            PageInputMode::LosslessText,
            &seeded.page.edit_context,
        )
        .unwrap();
    Ok((db, f, opened, block))
}

pub(super) fn lossless_payload(sequence: u64, block: &crate::pages::PageBlockDto) -> Vec<u8> {
    let end = block.text.encode_utf16().count();
    serde_json::to_vec_pretty(&serde_json::json!({
        "schema":"page-text/1", "sequence":sequence.to_string(), "mode":"lossless-text",
        "text":block.text, "selection":{"start":end,"end":end},
        "composition":null, "spans":block.spans
    }))
    .unwrap()
}

pub(super) fn input_open(f: &Fixture, writer: &str) -> PageInputDto {
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

pub(super) fn journal_without_input(f: &Fixture) -> serde_json::Value {
    let session = f.session();
    let mut value = match &session.journal {
        SessionJournal::BinaryPage(j) => serde_json::to_value(j).unwrap(),
        SessionJournal::Plan(j) => serde_json::to_value(j).unwrap(),
    };
    value.as_object_mut().unwrap().remove("input");
    value
}

pub(super) fn assert_input_metadata(f: &Fixture, input: &PageInputDto) {
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

pub(super) fn input_journal_crypto(f: &Fixture) -> ([u8; 32], Vec<u8>, &'static [u8]) {
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

pub(super) fn store_input_fixture(f: &Fixture, ciphertext: &str) {
    let item = f.engine.item_state(&f.item).unwrap();
    f.engine
        .store
        .set_meta(&row_id(&f.engine, item).unwrap(), ciphertext)
        .unwrap();
    f.engine.pages.sessions.lock().unwrap().clear();
}
