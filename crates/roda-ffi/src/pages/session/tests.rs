use super::*;
use crate::sync::Ingest;
use crate::RodaEngine;
use roda_docs::plan::{PlanBlockContent, MAX_PLAN_PACKET_BYTES};
use roda_docs::{Kind, Span};
use roda_log::chain_hash;
use roda_proto::{Envelope, Sequenced};

const RAW: &str = "raw 😀\r\n\0e\u{301}\r\u{85}\u{2029}tail";

struct Database(std::path::PathBuf);
impl Database {
    fn new() -> Self {
        let path = std::env::temp_dir().join(new_id("zoen-plan-session"));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> String {
        self.0.join("session.sqlite").to_str().unwrap().into()
    }
}
impl Drop for Database {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

struct Fixture {
    engine: Engine,
    space: String,
    item: String,
    owner: String,
}

fn original() -> PlanDoc {
    let legacy = "duplicate legacy / 😀 / ".repeat(24);
    PlanDoc {
        title: "Trip\r\nexact".into(),
        summary: RAW.into(),
        budget_cents: Some(5_000),
        sections: vec![PlanSection {
            title: "Stay\r".into(),
            lines: vec![
                PlanLine {
                    id: legacy.clone(),
                    text: "Hotel\r\nreservation".into(),
                    cost_cents: 200,
                    done: false,
                },
                PlanLine {
                    id: legacy,
                    text: "Legacy credit".into(),
                    cost_cents: -7,
                    done: true,
                },
            ],
        }],
    }
}

fn identity(engine: &mut Engine, name: &str) -> String {
    engine
        .create_identity(
            Identity {
                id: String::new(),
                kind: IdentityKind::Person,
                name: name.into(),
                handle: name.to_lowercase(),
                tint_hex: "#778899".into(),
                glyph: None,
                owner: None,
                bio: String::new(),
                owner_proof: None,
            },
            true,
        )
        .unwrap()
}

fn envelope_frame(engine: &Engine, env: Envelope) -> Sequenced {
    let head = engine.logs.get(env.space()).and_then(|log| log.head());
    let seq = head.as_ref().map_or(0, |head| head.seq + 1);
    let prev = head.map_or_else(|| GENESIS_PREV.into(), |head| head.hash);
    Sequenced {
        seq,
        hash: chain_hash(env.space(), seq, &prev, &env.wire_hash()),
        prev,
        env,
    }
}

fn deliver(engine: &mut Engine, event: &Event, expected: Ingest) -> Event {
    let frame = envelope_frame(engine, Envelope::plain(event));
    assert_eq!(engine.ingest(frame), expected);
    engine.logs[&event.space].events().last().unwrap().clone()
}

impl Fixture {
    fn new() -> Self {
        Self::at(":memory:")
    }
    fn at(path: &str) -> Self {
        Self::with_ids(path, new_id("plan-space"), new_id("plan-item"))
    }
    fn with_ids(path: &str, space: String, item: String) -> Self {
        let mut engine = Engine::open(path).unwrap();
        let owner = identity(&mut engine, "Owner");
        engine.me = Some(owner.clone());
        engine.store.set_meta("me", &owner).unwrap();
        let mut fixture = Self {
            engine,
            space,
            item,
            owner,
        };
        fixture.ordered(EventBody::SpaceCreated {
            title: "Conversation".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::Public,
        });
        fixture.ordered(EventBody::ItemCreated {
            item: fixture.item.clone(),
            kind: ItemKind::Plan,
            content: ItemContent::Plan(original()),
            origin: "legacy user".into(),
        });
        fixture
    }
    fn reopen(self) -> Self {
        let Self {
            engine,
            space,
            item,
            owner,
        } = self;
        let path = engine.db_path.clone();
        assert_ne!(path, ":memory:");
        drop(engine);
        Self {
            engine: Engine::open(&path).unwrap(),
            space,
            item,
            owner,
        }
    }
    fn session(&self) -> PageSession {
        PageSession::open(&self.engine, &self.item).unwrap()
    }
    fn signed_as(&self, author: &str, body: EventBody) -> Event {
        self.engine.author_for(author).unwrap().sign_event(
            &self.space,
            &new_id("session-event"),
            1_800_000_000_000,
            self.engine.logs.get(&self.space).and_then(|log| log.head()),
            body,
        )
    }
    fn ordered(&mut self, body: EventBody) -> Event {
        let event = self.signed_as(&self.owner, body);
        deliver(&mut self.engine, &event, Ingest::Applied)
    }
    fn queue(&mut self, body: EventBody) -> Event {
        let author = self.engine.author_for(&self.owner).unwrap();
        self.engine
            .append_synced(&self.space, &author, 1_800_000_000_000, body)
            .unwrap()
    }
    fn version(&self, content: ItemContent) -> EventBody {
        EventBody::ItemVersioned {
            item: self.item.clone(),
            content,
            note: "Exact version".into(),
        }
    }
    fn page(&self, document: &PlanDocument) -> PageContent {
        PageContent {
            title: document.view().unwrap().title,
            path: String::new(),
            loro: encode_bytes(&document.full_update().to_json().unwrap()),
            blob: None,
            manifest: Some(PageManifest::PlanV1(
                self.engine
                    .plan_timeline(&self.item)
                    .unwrap()
                    .manifest()
                    .unwrap(),
            )),
        }
    }
    fn remote(&mut self, edit: PlanBlockEdit) -> Event {
        let fold = self.engine.plan_timeline(&self.item).unwrap();
        let mut doc = fold.document.as_ref().unwrap().fork().unwrap();
        let revision = doc.revision().unwrap();
        doc.apply_from(&revision, &order_doc(&doc), &[edit])
            .unwrap();
        self.ordered(self.version(ItemContent::Page(self.page(&doc))))
    }
    fn pending(&self, reference: &CommitRef) -> Event {
        let client_id = match reference {
            CommitRef::Plan(c) => &c.client_id,
            CommitRef::BinaryPage { client_id } => client_id,
        };
        self.engine.store.outbox_get(client_id).unwrap().unwrap()
    }
    fn ack(&mut self, reference: &CommitRef) -> Event {
        let event = self.pending(reference);
        deliver(&mut self.engine, &event, Ingest::Confirmed)
    }
    fn cipher(&self) -> Option<String> {
        let it = self.engine.item_state(&self.item).unwrap();
        self.engine
            .store
            .meta(&row_id(&self.engine, it).unwrap())
            .unwrap()
    }
    fn peer(&self) -> Fixture {
        let mut peer = Engine::open(":memory:").unwrap();
        for (identity, secret) in self.engine.store.identities().unwrap() {
            peer.store.put_identity(&identity, secret.as_ref()).unwrap();
        }
        peer.store.set_meta("me", &self.owner).unwrap();
        peer.reload().unwrap();
        for event in self.engine.logs[&self.space].events() {
            let env = Envelope::plain(event);
            assert_eq!(
                peer.ingest(Sequenced {
                    seq: event.seq,
                    prev: event.prev.clone(),
                    hash: event.hash.clone(),
                    env
                }),
                Ingest::Applied
            );
        }
        Fixture {
            engine: peer,
            space: self.space.clone(),
            item: self.item.clone(),
            owner: self.owner.clone(),
        }
    }
}

fn blocks(read: &SessionRead) -> &[PlanBlock] {
    let DocumentView::Plan { blocks } = &read.view else {
        panic!("expected strict Plan projection")
    };
    blocks
}
fn order_read(read: &SessionRead) -> Vec<String> {
    blocks(read).iter().map(|b| b.id.clone()).collect()
}
fn order_doc(doc: &PlanDocument) -> Vec<String> {
    doc.blocks().unwrap().into_iter().map(|b| b.id).collect()
}
fn changed(block: &PlanBlock) -> PlanBlockEdit {
    PlanBlockEdit {
        id: block.id.clone(),
        content: block.content.clone(),
        text: block.plain_text(),
        spans: roda_docs::spans_of(&block.runs),
    }
}
fn line(read: &SessionRead) -> PlanBlockEdit {
    changed(
        blocks(read)
            .iter()
            .find(|b| {
                matches!(
                    b.content,
                    PlanBlockContent::Line {
                        cost_cents: 200,
                        ..
                    }
                )
            })
            .unwrap(),
    )
}
fn text_edit(read: &SessionRead, text: &str) -> PlanBlockEdit {
    let mut edit = line(read);
    edit.text = text.into();
    edit
}
fn apply(
    f: &Fixture,
    s: &mut PageSession,
    read: &SessionRead,
    id: &str,
    edits: &[PlanBlockEdit],
) -> R<AcceptedEdit> {
    s.apply_from(
        &f.engine,
        id,
        &read.edit_context,
        DocumentEdit::Plan {
            order: &order_read(read),
            changed: edits,
        },
    )
}
fn queued(status: CommitStatus) -> CommitRef {
    let CommitStatus::Queued(reference) = status else {
        panic!("expected queued publication: {status:?}")
    };
    reference
}
fn journal(s: &PageSession) -> &PlanJournalV1 {
    let SessionJournal::Plan(journal) = &s.journal else {
        panic!("expected Plan journal")
    };
    journal
}
fn exact(f: &Fixture, s: &PageSession) -> (Option<String>, Vec<u8>, u64, usize) {
    (
        f.cipher(),
        plan_doc(&s.live).unwrap().full_update().to_json().unwrap(),
        f.engine.store.outbox_len().unwrap(),
        f.engine.logs[&f.space].events().len(),
    )
}
fn no_change(f: &Fixture, s: &PageSession, before: &(Option<String>, Vec<u8>, u64, usize)) {
    assert_eq!(&exact(f, s), before);
}
fn trigger(f: &Fixture, name: &str, action: &str, table: &str, condition: &str) {
    f.engine.store.conn().execute_batch(&format!("CREATE TRIGGER {name} BEFORE {action} ON {table} {condition} BEGIN SELECT RAISE(ABORT, 'actual injected SQL failure'); END;")).unwrap();
}
fn drop_trigger(f: &Fixture, name: &str) {
    f.engine
        .store
        .conn()
        .execute_batch(&format!("DROP TRIGGER {name}"))
        .unwrap();
}

#[test]
fn legacy_binary_bytes_context_and_journal_remain_unchanged() {
    let public = RodaEngine::open(":memory:".into(), "en".into()).unwrap();
    public.seed_demo_if_empty().unwrap();
    let space = public
        .spaces()
        .into_iter()
        .find(|s| s.title == "Paraty with Marina")
        .unwrap()
        .id;
    let id = public
        .page_import_markdown(
            space,
            "legacy.md".into(),
            "# Title\n\nLegacy binary text\n".into(),
        )
        .unwrap()
        .id;
    let read = public.page(id.clone()).unwrap();
    let mut e = public.lock();
    let it = e.item_state(&id).unwrap();
    let ItemContent::Page(content) = &it.versions[0].content else {
        panic!("Page")
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&content.loro)
        .unwrap();
    assert!(content.manifest.is_none());
    let literal = serde_json::to_vec(content).unwrap();
    assert!(!String::from_utf8(literal.clone())
        .unwrap()
        .contains("manifest"));
    assert_eq!(
        serde_json::from_slice::<PageContent>(&literal).unwrap(),
        *content
    );
    let decoded = e.decode_page_edit_context(it, &read.edit_context).unwrap();
    assert!(read.edit_context.starts_with("v1:"));
    let raw_context = serde_json::to_value(drafts::EditContext {
        generation: decoded.generation,
        frontiers: decoded.frontiers.clone(),
    })
    .unwrap();
    assert_eq!(raw_context.as_object().unwrap().len(), 2);
    let page = Page::new();
    page.import(&bytes).unwrap();
    assert_eq!(page.frontiers().encode(), decoded.frontiers);
    // A real encrypted old journal omits accepted_generation, which still defaults.
    let old = serde_json::json!({"generation":0,"minimum_generation":0,"snapshot":null,"has_draft":false,"commit":null,"receipts":[{"fingerprint":"old","mutation_id":"old-receipt","context_hash":"old-context","generation":0,"content_hash":"old-hash","frontiers":decoded.frontiers}]});
    let cipher = write_legacy_fixture(&e, it, &serde_json::to_vec(&old).unwrap()).unwrap();
    assert_eq!(
        e.store.meta(&row_id(&e, it).unwrap()).unwrap(),
        Some(cipher)
    );
    assert_eq!(
        e.load_page_journal(it)
            .unwrap()
            .receipt("old-receipt")
            .unwrap()
            .accepted_generation,
        0
    );
    let mut session = PageSession::open(&e, &id).unwrap();
    let binary = session.read(&e).unwrap();
    assert_eq!(binary.tag, DocumentTag::BinaryPageV1);
    assert_eq!(binary.content_hash, read.content_hash);
    let DocumentView::BinaryPage { blocks } = binary.view else {
        panic!("Binary")
    };
    let mut edit = super::super::block_dto(&blocks[1]);
    edit.text = "private binary driver".into();
    edit.spans.clear();
    let order: Vec<_> = blocks.iter().map(|b| b.id.clone()).collect();
    let accepted = session
        .apply_from(
            &e,
            "binary-driver",
            &binary.edit_context,
            DocumentEdit::BinaryPage {
                order: &order,
                changed: std::slice::from_ref(&edit),
            },
        )
        .unwrap();
    assert!(accepted.applied_edit_context.starts_with("v1:"));
    let space = e.item_state(&id).unwrap().space.clone();
    e.store.set_synced(&space).unwrap();
    e.net.synced.insert(space);
    let reference = queued(session.commit(&mut e, "binary compatibility").unwrap());
    let CommitRef::BinaryPage { client_id } = reference else {
        panic!("binary commit")
    };
    let event = e.store.outbox_get(&client_id).unwrap().unwrap();
    assert!(matches!(
        &event.body,
        EventBody::ItemVersioned {
            content: ItemContent::Page(PageContent { manifest: None, .. }),
            ..
        }
    ));
    let frame = envelope_frame(&e, Envelope::plain(&event));
    assert_eq!(e.ingest(frame), Ingest::Confirmed);
    assert!(!session.read(&e).unwrap().unsaved);
    edit.text = "public binary driver".into();
    drop(e);
    public.lock().reload().unwrap();
    let public_read = public.page(id.clone()).unwrap();
    let mut dto = public_read.blocks[1].clone();
    dto.text = edit.text;
    let accepted = public
        .page_apply_from(
            id.clone(),
            "legacy-public".into(),
            public_read.edit_context.clone(),
            public_read.blocks.iter().map(|b| b.id.clone()).collect(),
            vec![dto.clone()],
        )
        .unwrap();
    public.lock().reload().unwrap();
    let replay = public
        .page_apply_from(
            id.clone(),
            "legacy-public".into(),
            public_read.edit_context,
            public_read.blocks.iter().map(|b| b.id.clone()).collect(),
            vec![dto],
        )
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.applied_content_hash, accepted.applied_content_hash);
    assert!(public
        .page(id)
        .unwrap()
        .blocks
        .iter()
        .any(|b| b.text.contains("public binary driver")));
}

fn edited_binary_fixture() -> (std::sync::Arc<RodaEngine>, String, PageSession) {
    let public = RodaEngine::open(":memory:".into(), "en".into()).unwrap();
    public.seed_demo_if_empty().unwrap();
    let space = public
        .spaces()
        .into_iter()
        .find(|s| s.title == "Paraty with Marina")
        .unwrap()
        .id;
    let id = public
        .page_import_markdown(space, "offline.md".into(), "# Offline\n\nOriginal\n".into())
        .unwrap()
        .id;
    let mut engine = public.lock();
    let creations: Vec<_> = engine
        .store
        .outbox()
        .unwrap()
        .into_iter()
        .filter(|p| matches!(&p.event.body, EventBody::ItemCreated { item, .. } if item == &id))
        .map(|p| p.event)
        .collect();
    for event in creations {
        deliver(&mut engine, &event, Ingest::Confirmed);
    }
    drop(engine);
    let read = public.page(id.clone()).unwrap();
    let mut changed = read.blocks[1].clone();
    changed.text = "Durable offline edit".into();
    public
        .page_apply_from(
            id.clone(),
            "offline-edit".into(),
            read.edit_context,
            read.blocks.iter().map(|b| b.id.clone()).collect(),
            vec![changed],
        )
        .unwrap();
    let engine = public.lock();
    let session = PageSession::open(&engine, &id).unwrap();
    drop(engine);
    (public, id, session)
}

#[test]
fn binary_offline_commit_preserves_the_existing_page_contract() {
    let (legacy, legacy_id, _) = edited_binary_fixture();
    let mut legacy_engine = legacy.lock();
    let space = legacy_engine.item_state(&legacy_id).unwrap().space.clone();
    legacy_engine.net.synced.remove(&space);
    assert!(legacy_engine
        .page_commit(&legacy_id, "offline baseline")
        .unwrap());
    let legacy_read = legacy_engine.page(&legacy_id).unwrap();
    assert!(!legacy_read.pending_sync);

    let (public, id, mut session) = edited_binary_fixture();
    let mut engine = public.lock();
    let space = engine.item_state(&id).unwrap().space.clone();
    engine.net.synced.remove(&space);
    assert!(!engine.net.synced.contains(&space));
    let CommitRef::BinaryPage { client_id } =
        queued(session.commit(&mut engine, "offline save").unwrap())
    else {
        panic!("BinaryPage commit")
    };
    assert!(engine.store.outbox_get(&client_id).unwrap().is_none());
    let event = engine.logs[&space]
        .events()
        .iter()
        .find(|event| event.client_id == client_id)
        .unwrap();
    assert!(
        matches!(&event.body, EventBody::ItemVersioned { item, content: ItemContent::Page(PageContent { manifest: None, .. }), .. } if item == &id)
    );
    let read = session.read(&engine).unwrap();
    assert_eq!(read.pending_sync, legacy_read.pending_sync);
    assert_eq!(read.unsaved, legacy_read.unsaved);
    engine.reload().unwrap();
    let mut reopened = PageSession::open(&engine, &id).unwrap();
    assert_eq!(
        reopened.read(&engine).unwrap().content_hash,
        read.content_hash
    );
}

#[test]
fn binary_replay_keeps_the_exact_pre_upgrade_payload() {
    let (public, id, _) = edited_binary_fixture();
    let read = public.page(id.clone()).unwrap();
    let order: Vec<_> = read.blocks.iter().map(|b| b.id.clone()).collect();
    let mut dto = read.blocks[1].clone();
    assert_eq!(dto.kind, "paragraph");
    dto.level = 5;
    dto.text = "Exact legacy payload on retry".into();
    dto.spans.clear();
    let accepted = public
        .page_apply_from(
            id.clone(),
            "binary-raw-upgrade".into(),
            read.edit_context.clone(),
            order.clone(),
            vec![dto.clone()],
        )
        .unwrap();
    let legacy_replay = public
        .page_apply_from(
            id.clone(),
            "binary-raw-upgrade".into(),
            read.edit_context.clone(),
            order.clone(),
            vec![dto.clone()],
        )
        .unwrap();
    assert!(legacy_replay.replayed);
    assert_eq!(legacy_replay.mutation_hash, accepted.mutation_hash);
    let engine = public.lock();
    let it = engine.item_state(&id).unwrap();
    let before = serde_json::to_vec(&engine.load_page_journal(it).unwrap()).unwrap();
    let mut session = PageSession::open(&engine, &id).unwrap();
    let replay = session
        .apply_from(
            &engine,
            "binary-raw-upgrade",
            &read.edit_context,
            DocumentEdit::BinaryPage {
                order: &order,
                changed: std::slice::from_ref(&dto),
            },
        )
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.request_hash, accepted.mutation_hash);
    assert_eq!(replay.applied_content_hash, accepted.applied_content_hash);
    let original_context = engine
        .decode_page_edit_context(it, &accepted.applied_edit_context)
        .unwrap();
    for context in [
        &legacy_replay.applied_edit_context,
        &replay.applied_edit_context,
    ] {
        let decoded = engine.decode_page_edit_context(it, context).unwrap();
        assert_eq!(decoded.generation, original_context.generation);
        assert_eq!(decoded.frontiers, original_context.frontiers);
    }
    let mut changed_payload = dto.clone();
    changed_payload.level = 6;
    assert!(matches!(
        session.apply_from(
            &engine,
            "binary-raw-upgrade",
            &read.edit_context,
            DocumentEdit::BinaryPage {
                order: &order,
                changed: std::slice::from_ref(&changed_payload),
            },
        ),
        Err(CoreError::Stale { .. })
    ));
    assert_eq!(
        serde_json::to_vec(&engine.load_page_journal(it).unwrap()).unwrap(),
        before
    );
}

#[test]
fn binary_refresh_reuses_verified_payload_and_rechecks_after_reload() {
    let mut f = Fixture::new();
    let id = new_id("binary-cache-page");
    let page = Page::from_markdown("# Cached\n\nVerified raw 😀\r\ncontent\n").unwrap();
    let blob = f.engine.page_blob(&f.space, &page.snapshot()).unwrap();
    f.ordered(EventBody::ItemCreated {
        item: id.clone(),
        kind: ItemKind::Page,
        content: ItemContent::Page(PageContent {
            title: "Cached".into(),
            path: "cached.md".into(),
            loro: String::new(),
            blob: Some(blob.clone()),
            manifest: None,
        }),
        origin: "imported binary Page".into(),
    });
    let legacy = f.engine.page(&id).unwrap();
    let mut session = PageSession::open(&f.engine, &id).unwrap();
    let before = session.read(&f.engine).unwrap();
    assert!(before.can_edit && before.save_error.is_none());
    let (mime, bytes) = f.engine.store.media(&blob.sha256).unwrap().unwrap();
    assert_eq!(
        f.engine
            .store
            .conn()
            .execute("DELETE FROM media WHERE sha256 = ?1", [&blob.sha256])
            .unwrap(),
        1
    );
    assert!(f.engine.store.media(&blob.sha256).unwrap().is_none());
    // The existing public Page session retains the bytes it already verified.
    let legacy_cached = f.engine.page(&id).unwrap();
    assert!(legacy_cached.can_edit && legacy_cached.save_error.is_none());
    assert_eq!(legacy_cached.content_hash, legacy.content_hash);
    let cached = session.read(&f.engine).unwrap();
    assert!(
        cached.can_edit && cached.save_error.is_none(),
        "already verified binary history must not be downloaded again"
    );
    assert_eq!(cached.view, before.view);
    assert_eq!(cached.content_hash, before.content_hash);

    // Reprojection discards that trust; a fresh session still needs the payload.
    f.engine.reload().unwrap();
    let reloaded = session.read(&f.engine).unwrap();
    assert!(!reloaded.can_edit);
    assert!(matches!(session.blocked, Some(SessionBlock::Waiting(_))));
    let mut reopened = PageSession::open(&f.engine, &id).unwrap();
    assert_eq!(
        reloaded.content_hash,
        reopened.read(&f.engine).unwrap().content_hash
    );
    f.engine
        .store
        .put_media(&blob.sha256, &mime, &bytes, crate::engine::now_ms())
        .unwrap();
    let resumed = session.read(&f.engine).unwrap();
    assert!(resumed.can_edit && resumed.save_error.is_none());
    assert_eq!(resumed.view, before.view);
    assert_eq!(resumed.content_hash, before.content_hash);
}

#[test]
fn plan_refresh_reuses_verified_payload_and_rechecks_after_reload() {
    let mut f = Fixture::new();
    let mut session = f.session();
    let initial = session.read(&f.engine).unwrap();
    let mut document = plan_doc(&session.shadow).unwrap().fork().unwrap();
    let revision = document.revision().unwrap();
    document
        .apply_from(
            &revision,
            &order_doc(&document),
            &[text_edit(&initial, "Verified Plan 😀\r\ncontent")],
        )
        .unwrap();
    let bytes = document.full_update().to_json().unwrap();
    let blob = f.engine.page_blob(&f.space, &bytes).unwrap();
    let mut page = f.page(&document);
    page.blob = Some(blob.clone());
    page.loro.clear();
    let event = f.ordered(f.version(ItemContent::Page(page)));
    let fold = f.engine.plan_timeline(&f.item).unwrap();
    assert!(fold.acknowledges(
        &event.client_id,
        &plan::source(&f.space, &f.item, &event, true).event_id
    ));
    let before = session.read(&f.engine).unwrap();
    assert!(before.can_edit && before.save_error.is_none());
    assert_ne!(before.content_hash, initial.content_hash);
    let (mime, bytes) = f.engine.store.media(&blob.sha256).unwrap().unwrap();
    assert_eq!(
        f.engine
            .store
            .conn()
            .execute("DELETE FROM media WHERE sha256 = ?1", [&blob.sha256])
            .unwrap(),
        1
    );
    let cached = session.read(&f.engine).unwrap();
    assert!(
        cached.can_edit && cached.save_error.is_none(),
        "already verified Plan history must not be downloaded again"
    );
    assert_eq!(cached.view, before.view);
    assert_eq!(cached.content_hash, before.content_hash);

    f.engine.reload().unwrap();
    let reloaded = session.read(&f.engine).unwrap();
    assert!(!reloaded.can_edit);
    assert!(matches!(session.blocked, Some(SessionBlock::Waiting(_))));
    let mut reopened = f.session();
    assert_eq!(
        reloaded.content_hash,
        reopened.read(&f.engine).unwrap().content_hash
    );
    f.engine
        .store
        .put_media(&blob.sha256, &mime, &bytes, crate::engine::now_ms())
        .unwrap();
    let resumed = session.read(&f.engine).unwrap();
    assert!(resumed.can_edit && resumed.save_error.is_none());
    assert_eq!(resumed.view, before.view);
    assert_eq!(resumed.content_hash, before.content_hash);
}

#[test]
fn binary_failed_publication_preserves_the_existing_visible_error() {
    let (public, id, mut session) = edited_binary_fixture();
    let mut engine = public.lock();
    let space = engine.item_state(&id).unwrap().space.clone();
    engine.store.set_synced(&space).unwrap();
    engine.net.synced.insert(space);
    let CommitRef::BinaryPage { client_id } =
        queued(session.commit(&mut engine, "save then refuse").unwrap())
    else {
        panic!("BinaryPage commit")
    };
    engine
        .store
        .outbox_note(&client_id, "the server refused this exact save", true)
        .unwrap();
    let public_read = engine.page(&id).unwrap();
    let legacy_error = public_read.save_error.clone();
    assert_eq!(
        legacy_error.as_deref(),
        Some("the server refused this exact save")
    );
    let read = session.read(&engine).unwrap();
    assert_eq!(read.save_error, legacy_error);
    assert!(read.unsaved);
    assert!(!read.pending_sync);
    assert!(public_read.unsaved);
    assert!(!public_read.pending_sync);
    assert_eq!(read.content_hash, public_read.content_hash);
    engine.reload().unwrap();
    let reopened = engine.page(&id).unwrap();
    assert_eq!(reopened.content_hash, public_read.content_hash);
    assert_eq!(reopened.save_error, legacy_error);
    assert!(reopened.unsaved);
    assert!(!reopened.pending_sync);
}

#[test]
fn typed_plan_draft_and_exact_receipt_survive_file_reopen() {
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

#[test]
fn contexts_bind_actor_space_item_kind_and_creation() {
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

#[test]
fn same_mutation_replays_exact_accepted_context_but_returns_current_view() {
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

#[test]
fn changed_request_with_same_mutation_refuses_atomically() {
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

#[test]
fn lost_reply_empty_diff_undo_and_redo_continue_own_branch() {
    let db = Database::new();
    let mut f = Fixture::at(&db.path());
    let mut s = f.session();
    let read = s.read(&f.engine).unwrap();
    let edit = text_edit(&read, "abcdef repeated 😀");
    let first = apply(&f, &mut s, &read, "typed", std::slice::from_ref(&edit)).unwrap();
    drop(s);
    f = f.reopen();
    s = f.session();
    let undone = apply(&f, &mut s, &read, "undo", &[]).unwrap();
    assert_eq!(undone.current.view, read.view);
    assert_eq!(undone.applied_content_hash, read.content_hash);
    assert!(journal(&s).has_draft);
    assert!(journal(&s).commit.is_none());
    let history = plan_doc(&s.live).unwrap().full_update().to_json().unwrap();
    let receipts = serde_json::to_value(&journal(&s).receipts).unwrap();
    let snapshot = serde_json::to_value(&journal(&s).snapshot).unwrap();
    assert_eq!(
        s.commit(&mut f.engine, "accepted local undo").unwrap(),
        CommitStatus::Unchanged
    );
    assert!(!journal(&s).has_draft);
    assert_eq!(f.engine.store.outbox_len().unwrap(), 0);
    assert_eq!(
        serde_json::to_value(&journal(&s).receipts).unwrap(),
        receipts
    );
    assert_eq!(
        serde_json::to_value(&journal(&s).snapshot).unwrap(),
        snapshot
    );
    let ciphertext = f.cipher().unwrap();
    drop(s);
    f = f.reopen();
    s = f.session();
    assert!(!journal(&s).has_draft);
    assert_eq!(
        plan_doc(&s.live).unwrap().full_update().to_json().unwrap(),
        history
    );
    assert_eq!(
        serde_json::to_value(&journal(&s).receipts).unwrap(),
        receipts
    );
    assert_eq!(
        serde_json::to_value(&journal(&s).snapshot).unwrap(),
        snapshot
    );
    let first_retry = apply(&f, &mut s, &read, "typed", std::slice::from_ref(&edit)).unwrap();
    assert!(first_retry.replayed);
    assert_eq!(first_retry.applied_edit_context, first.applied_edit_context);
    assert_eq!(first_retry.current.view, read.view);
    assert!(!first_retry.current.unsaved);
    assert_eq!(f.cipher().unwrap(), ciphertext);
    let redone = apply(&f, &mut s, &read, "redo", std::slice::from_ref(&edit)).unwrap();
    assert_eq!(redone.applied_content_hash, first.applied_content_hash);
    let first_retry = apply(&f, &mut s, &read, "typed", &[edit]).unwrap();
    assert!(first_retry.replayed);
    assert_eq!(first_retry.applied_edit_context, first.applied_edit_context);
    assert_eq!(
        blocks(&redone.current)
            .iter()
            .filter(|b| b.plain_text() == "abcdef repeated 😀")
            .count(),
        1
    );
    drop(s);
    drop(f);
}

#[test]
fn continuation_does_not_replace_unseen_remote_fields_or_blocks() {
    for remote_first in [false, true] {
        let mut f = Fixture::new();
        let mut s = f.session();
        let read = s.read(&f.engine).unwrap();
        let line = line(&read);
        let mut remote = f
            .engine
            .plan_timeline(&f.item)
            .unwrap()
            .document
            .unwrap()
            .fork()
            .unwrap();
        let rev = remote.revision().unwrap();
        let mut cost = line.clone();
        if let PlanBlockContent::Line { cost_cents, .. } = &mut cost.content {
            *cost_cents = 909;
        }
        let summary = PlanBlockEdit {
            id: blocks(&read)[1].id.clone(),
            content: PlanBlockContent::Summary,
            text: "remote summary".into(),
            spans: vec![Span {
                start: 0,
                end: 6,
                key: "b".into(),
                value: String::new(),
            }],
        };
        let prose = PlanBlockEdit {
            id: "remote-prose".into(),
            content: PlanBlockContent::Prose {
                kind: Kind::Paragraph,
            },
            text: "unseen block".into(),
            spans: vec![],
        };
        let mut order = order_read(&read);
        order.push(prose.id.clone());
        remote
            .apply_from(&rev, &order, &[cost, summary, prose])
            .unwrap();
        let event = f.signed_as(&f.owner, f.version(ItemContent::Page(f.page(&remote))));
        if remote_first {
            deliver(&mut f.engine, &event, Ingest::Applied);
        }
        apply(&f, &mut s, &read, "first", &[text_edit(&read, "own first")]).unwrap();
        if !remote_first {
            deliver(&mut f.engine, &event, Ingest::Applied);
        }
        let accepted = apply(
            &f,
            &mut s,
            &read,
            "second",
            &[text_edit(&read, "own second")],
        )
        .unwrap();
        assert!(blocks(&accepted.current)
            .iter()
            .any(|b| b.plain_text() == "unseen block"));
        assert!(blocks(&accepted.current)
            .iter()
            .any(|b| b.plain_text() == "remote summary" && b.runs.iter().any(|r| r.attrs.bold)));
        let line = blocks(&accepted.current)
            .iter()
            .find(|b| b.id == line.id)
            .unwrap();
        assert_eq!(line.plain_text(), "own second");
        assert!(matches!(
            line.content,
            PlanBlockContent::Line {
                cost_cents: 909,
                ..
            }
        ));
    }
}

#[test]
fn continuation_removes_own_new_block_without_reusing_history() {
    let f = Fixture::new();
    let mut s = f.session();
    let read = s.read(&f.engine).unwrap();
    let mut order = order_read(&read);
    order.push("own-prose".into());
    let prose = PlanBlockEdit {
        id: "own-prose".into(),
        content: PlanBlockContent::Prose {
            kind: Kind::Paragraph,
        },
        text: "own insertion".into(),
        spans: vec![],
    };
    s.apply_from(
        &f.engine,
        "insert",
        &read.edit_context,
        DocumentEdit::Plan {
            order: &order,
            changed: std::slice::from_ref(&prose),
        },
    )
    .unwrap();
    let removed = apply(&f, &mut s, &read, "remove", &[]).unwrap();
    assert_eq!(removed.current.view, read.view);
    let current = s.read(&f.engine).unwrap();
    let before = exact(&f, &s);
    let error = s
        .apply_from(
            &f.engine,
            "historical-redo",
            &current.edit_context,
            DocumentEdit::Plan {
                order: &order,
                changed: &[prose],
            },
        )
        .err()
        .unwrap();
    assert!(error.to_string().contains("historical plan block ID"));
    no_change(&f, &s, &before);
    // Authenticated legacy negatives remain readable but do not become ordinary restores.
    let mut negative = changed(
        blocks(&current)
            .iter()
            .find(|b| matches!(b.content, PlanBlockContent::Line { cost_cents: -7, .. }))
            .unwrap(),
    );
    if let PlanBlockContent::Line { cost_cents, .. } = &mut negative.content {
        *cost_cents = 9;
    }
    let positive = apply(&f, &mut s, &current, "positive", &[negative]).unwrap();
    let original = changed(
        blocks(&current)
            .iter()
            .find(|b| matches!(b.content, PlanBlockContent::Line { cost_cents: -7, .. }))
            .unwrap(),
    );
    let before = exact(&f, &s);
    let error = s
        .apply_from(
            &f.engine,
            "negative-restore",
            &positive.applied_edit_context,
            DocumentEdit::Plan {
                order: &order_read(&current),
                changed: &[original],
            },
        )
        .err()
        .unwrap();
    assert!(error.to_string().contains("new cost cannot be negative"));
    no_change(&f, &s, &before);
}

#[test]
fn generation_fence_allows_exact_receipt_replay_but_refuses_new_continuation() {
    let f = Fixture::new();
    let mut s = f.session();
    let read = s.read(&f.engine).unwrap();
    let edit = text_edit(&read, "accepted before fence");
    let accepted = apply(&f, &mut s, &read, "accepted", std::slice::from_ref(&edit)).unwrap();
    let mut fenced = journal(&s).clone();
    fenced.invalidate_contexts().unwrap();
    super::super::drafts::tagged::save(&f.engine, f.engine.item_state(&f.item).unwrap(), &fenced)
        .unwrap();
    let before = exact(&f, &s);
    let retry = apply(&f, &mut s, &read, "accepted", std::slice::from_ref(&edit)).unwrap();
    assert!(retry.replayed);
    assert_eq!(retry.applied_edit_context, accepted.applied_edit_context);
    assert!(matches!(
        apply(&f, &mut s, &read, "unaccepted", std::slice::from_ref(&edit)),
        Err(CoreError::Stale { .. })
    ));
    assert!(matches!(
        s.apply_from(
            &f.engine,
            "stale-continuation",
            &retry.applied_edit_context,
            DocumentEdit::Plan {
                order: &order_read(&read),
                changed: &[edit]
            }
        ),
        Err(CoreError::Stale { .. })
    ));
    no_change(&f, &s, &before);
}

#[test]
fn receipt_eviction_fences_forgotten_retry_across_reopen() {
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

#[test]
fn journal_insert_or_update_failure_keeps_previous_draft() {
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

#[test]
fn oversized_context_or_journal_fails_before_memory_replacement() {
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

#[test]
fn lost_response_after_durable_accept_recovers_the_same_receipt() {
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

#[test]
fn outbox_insert_failure_keeps_draft_and_receipts() {
    let mut f = Fixture::new();
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
}

#[test]
fn journal_failure_after_outbox_insert_rolls_back_event_and_projection() {
    let mut f = Fixture::new();
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
    f.ack(&reference);
    assert!(!s.read(&f.engine).unwrap().unsaved);
}

#[test]
fn lost_commit_response_reopens_one_exact_pending_source() {
    let db = Database::new();
    let mut f = Fixture::at(&db.path());
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
    f.ack(&CommitRef::Plan(reference));
    assert!(!s.read(&f.engine).unwrap().unsaved);
    drop(s);
    drop(f);
}

#[test]
fn only_complete_canonical_exact_source_acks() {
    let mut f = Fixture::new();
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
    let wrong_client = f.signed_as(&f.owner, event.body.clone());
    deliver(&mut f.engine, &wrong_client, Ingest::Applied);
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
    );
    assert_ne!(
        plan::source(&f.space, &f.item, &wrong_digest, false).event_id,
        plan::source(&f.space, &f.item, &event, false).event_id
    );
    deliver(&mut f.engine, &wrong_digest, Ingest::Confirmed);
    assert_eq!(f.engine.store.outbox_len().unwrap(), 0);
    assert!(s.read(&f.engine).unwrap().unsaved);
    assert!(journal(&s).commit.is_some());
    deliver(&mut f.engine, &event, Ingest::Applied);
    let final_read = s.read(&f.engine).unwrap();
    assert!(!final_read.unsaved);
    assert!(journal(&s).commit.is_none());
    assert_eq!(final_read.view, accepted.current.view);
    let peer = f.peer();
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
    encrypted_exact_source_and_peer_readability();
}

fn encrypted_exact_source_and_peer_readability() {
    let mut owner = Engine::open(":memory:").unwrap();
    owner
        .create_account("Ana", "ana", "http://relay.test")
        .unwrap();
    let mut peer = Engine::open(":memory:").unwrap();
    peer.create_account("Bruno", "bruno", "http://relay.test")
        .unwrap();
    let mut f = Fixture {
        owner: owner.me.clone().unwrap(),
        engine: owner,
        space: new_id("mls-plan"),
        item: new_id("mls-item"),
    };
    f.ordered(EventBody::SpaceCreated {
        title: "Encrypted Plan".into(),
        kind: SpaceKind::Group,
        privacy: Privacy::EndToEnd,
    });
    f.ordered(EventBody::MemberAdded {
        identity: peer.me.clone().unwrap(),
        role: Role::Member,
    });
    f.ordered(EventBody::ItemCreated {
        item: f.item.clone(),
        kind: ItemKind::Plan,
        content: ItemContent::Plan(original()),
        origin: "authenticated legacy".into(),
    });
    for event in f.engine.logs[&f.space].events() {
        assert_eq!(
            peer.ingest(Sequenced {
                seq: event.seq,
                prev: event.prev.clone(),
                hash: event.hash.clone(),
                env: Envelope::plain(event)
            }),
            Ingest::Applied
        );
    }
    f.engine.create_mls_group(&f.space).unwrap();
    let packages = peer.device().unwrap().key_packages(1, false).unwrap();
    let commit = f
        .engine
        .device()
        .unwrap()
        .commit(&f.space, &packages, &Default::default())
        .unwrap();
    {
        let device = f.engine.device().unwrap();
        assert!(matches!(
            device
                .open(
                    &f.space,
                    &commit.commit,
                    &f.engine.roster(&f.space),
                    device.leaf()
                )
                .unwrap(),
            roda_mls::Opened::Commit { epoch: 1 }
        ));
    }
    assert!(peer
        .device()
        .unwrap()
        .join(&f.space, &commit.welcome.unwrap(), &peer.roster(&f.space))
        .unwrap());
    assert_eq!(peer.device().unwrap().epoch(&f.space).unwrap(), 1);
    let mut s = f.session();
    let read = s.read(&f.engine).unwrap();
    let mut edit = text_edit(&read, RAW);
    if let PlanBlockContent::Line {
        cost_cents, done, ..
    } = &mut edit.content
    {
        *cost_cents = 456;
        *done = true;
    }
    edit.spans = vec![Span {
        start: 0,
        end: 3,
        key: "b".into(),
        value: String::new(),
    }];
    let accepted = apply(&f, &mut s, &read, "sealed-draft", &[edit]).unwrap();
    let reference = queued(s.commit(&mut f.engine, "encrypted publication").unwrap());
    let event = f.pending(&reference);
    let CommitRef::Plan(expected) = &reference else {
        panic!("Plan")
    };
    assert!(f.engine.mls_seal_outbox().unwrap());
    let env = f
        .engine
        .outbox_envelopes()
        .into_iter()
        .find(|e| e.client_id() == event.client_id)
        .unwrap();
    assert!(env.is_sealed());
    assert_ne!(env.content(), event.content.as_slice());
    let frame = envelope_frame(&f.engine, env);
    assert_eq!(peer.ingest(frame.clone()), Ingest::Applied);
    assert_eq!(f.engine.ingest(frame), Ingest::Confirmed);
    let landed = f.engine.logs[&f.space].events().last().unwrap();
    assert_eq!(landed.content, event.content);
    assert!(landed.sealed_wire.is_some());
    assert_eq!(
        plan::source(&f.space, &f.item, landed, true).event_id,
        expected.event_id
    );
    let current = s.read(&f.engine).unwrap();
    assert!(!current.unsaved);
    assert_eq!(current.view, accepted.current.view);
    let mut peer_session = PageSession::open(&peer, &f.item).unwrap();
    let peer_read = peer_session.read(&peer).unwrap();
    assert_eq!(peer_read.view, current.view);
    assert_eq!(
        peer.device().unwrap().epoch(&f.space).unwrap(),
        f.engine.device().unwrap().epoch(&f.space).unwrap()
    );
    // The same real MLS pair still communicates in the opposite direction.
    let peer_author = peer.net.author.clone().unwrap();
    let response = peer
        .append(
            &f.space,
            &peer_author.identity,
            EventBody::MessagePosted {
                message: new_id("peer-readable"),
                text: "peer after typed Plan".into(),
                attaches: None,
                reply: None,
                forwarded: false,
            },
        )
        .unwrap();
    peer.mls_seal_outbox().unwrap();
    let env = peer
        .outbox_envelopes()
        .into_iter()
        .find(|e| e.client_id() == response.client_id)
        .unwrap();
    let frame = envelope_frame(&peer, env);
    assert_eq!(f.engine.ingest(frame), Ingest::Applied);
    assert!(
        matches!(&f.engine.logs[&f.space].events().last().unwrap().body, EventBody::MessagePosted { text, .. } if text == "peer after typed Plan")
    );
}

fn expect_source(f: &Fixture, s: &mut PageSession, event: &Event) {
    let mut journal = journal(s).clone();
    journal.commit = Some(PlanCommitRef {
        client_id: event.client_id.clone(),
        event_id: plan::source(&f.space, &f.item, event, false).event_id,
        published_revision_base64: encode_bytes(
            &plan_doc(&s.live)
                .unwrap()
                .revision()
                .unwrap()
                .encode()
                .unwrap(),
        ),
        published_content_hash: plan_doc(&s.live).unwrap().semantic_hash().unwrap(),
    });
    super::super::drafts::tagged::save(&f.engine, f.engine.item_state(&f.item).unwrap(), &journal)
        .unwrap();
    s.refresh(&f.engine).unwrap();
}

#[test]
fn semantically_refused_source_preserves_draft_and_visible_history() {
    let mut f = Fixture::new();
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
    deliver(&mut f.engine, &event, Ingest::Confirmed);
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
    f.ack(&retry);
    assert!(!s.read(&f.engine).unwrap().unsaved);
}

#[test]
fn waiting_blob_blocks_edit_ack_and_later_versions_until_ready() {
    let mut f = Fixture::new();
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
    f.ack(&reference);
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
    let later = f.signed_as(&f.owner, event.body.clone());
    deliver(&mut f.engine, &later, Ingest::Applied);
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
}

#[test]
fn pending_legacy_and_remote_cutover_never_expand_seed() {
    for undo in [false, true] {
        let mut f = Fixture::new();
        let mut s = f.session();
        let read = s.read(&f.engine).unwrap();
        let original_manifest = f.engine.plan_timeline(&f.item).unwrap().manifest().unwrap();
        let body = if undo {
            EventBody::ItemReverted {
                item: f.item.clone(),
                to_version: 1,
                note: "pending undo".into(),
            }
        } else {
            let mut doc = original();
            doc.title = "optimistic legacy title".into();
            f.version(ItemContent::Plan(doc))
        };
        let legacy = f.queue(body);
        let blocked = s.read(&f.engine).unwrap();
        assert!(!blocked.can_edit);
        assert_eq!(s.blocked, Some(SessionBlock::PendingLegacy));
        let cipher = f.cipher();
        assert!(apply(&f, &mut s, &read, "blocked", &[]).is_err());
        assert_eq!(f.cipher(), cipher);
        let fold = f.engine.plan_timeline(&f.item).unwrap();
        assert!(fold.accepts_prefix(&original_manifest));
        let mut remote = fold.document.unwrap().fork().unwrap();
        let revision = remote.revision().unwrap();
        remote
            .apply_from(
                &revision,
                &order_doc(&remote),
                &[text_edit(&read, "ordered remote cutover")],
            )
            .unwrap();
        let page = PageContent {
            title: "cached hint".into(),
            path: String::new(),
            loro: encode_bytes(&remote.full_update().to_json().unwrap()),
            blob: None,
            manifest: Some(PageManifest::PlanV1(original_manifest.clone())),
        };
        f.ordered(f.version(ItemContent::Page(page)));
        let fold = f.engine.plan_timeline(&f.item).unwrap();
        assert!(fold.frozen_at.is_some());
        assert!(fold.pending_legacy);
        assert_eq!(
            fold.document.as_ref().unwrap().blocks().unwrap(),
            remote.blocks().unwrap()
        );
        assert_eq!(
            fold.origin_tag().unwrap().2,
            original_manifest.creation_event
        );
        assert!(fold.accepts_prefix(&original_manifest));
        if undo {
            deliver(&mut f.engine, &legacy, Ingest::Confirmed);
            assert!(matches!(
                f.engine
                    .plan_timeline(&f.item)
                    .unwrap()
                    .slots
                    .last()
                    .unwrap()
                    .acceptance,
                Acceptance::Refused(_)
            ));
        } else {
            assert!(f
                .engine
                .reject(&legacy.client_id, "legacy writer refused", true, None)
                .user_failed());
        }
        let current = s.read(&f.engine).unwrap();
        assert!(current.can_edit);
        assert_eq!(blocks(&current), remote.blocks().unwrap());
        assert!(f
            .engine
            .plan_timeline(&f.item)
            .unwrap()
            .accepts_prefix(&original_manifest));
    }
}

#[test]
fn late_legacy_versions_never_ack_or_change_canonical_shadow() {
    let mut f = Fixture::new();
    let mut s = f.session();
    let read = s.read(&f.engine).unwrap();
    apply(
        &f,
        &mut s,
        &read,
        "draft",
        &[text_edit(&read, "canonical content")],
    )
    .unwrap();
    let reference = queued(s.commit(&mut f.engine, "cutover").unwrap());
    f.ack(&reference);
    let canonical = s.read(&f.engine).unwrap();
    let shadow = plan_doc(&s.shadow)
        .unwrap()
        .full_update()
        .to_json()
        .unwrap();
    let cipher = f.cipher();
    let mut legacy = original();
    legacy.title = "late incompatible title".into();
    f.ordered(f.version(ItemContent::Plan(legacy)));
    f.ordered(EventBody::ItemReverted {
        item: f.item.clone(),
        to_version: 1,
        note: "late undo".into(),
    });
    let fold = f.engine.plan_timeline(&f.item).unwrap();
    assert!(fold
        .slots
        .iter()
        .rev()
        .take(2)
        .all(|slot| matches!(slot.acceptance, Acceptance::Refused(_))));
    assert!(fold
        .slots
        .iter()
        .rev()
        .take(2)
        .all(|slot| slot.content.is_some()));
    assert_eq!(s.read(&f.engine).unwrap().view, canonical.view);
    assert_eq!(
        plan_doc(&s.shadow)
            .unwrap()
            .full_update()
            .to_json()
            .unwrap(),
        shadow
    );
    assert_eq!(f.cipher(), cipher);
    assert!(!fold.acknowledges(
        &fold.slots.last().unwrap().source.client_id,
        &fold.slots.last().unwrap().source.event_id
    ));
}

#[test]
fn reader_removed_actor_and_key_loss_refuse_mutation_but_keep_saved_read() {
    for mode in [
        "reader",
        "reader-unchanged",
        "removed",
        "key-loss",
        "pending-grant",
    ] {
        let mut f = Fixture::new();
        let mut s = f.session();
        let read = s.read(&f.engine).unwrap();
        let edit = text_edit(&read, "private intent remains");
        apply(&f, &mut s, &read, "accepted", std::slice::from_ref(&edit)).unwrap();
        if mode == "reader-unchanged" {
            let undo = apply(&f, &mut s, &read, "local-undo", &[]).unwrap();
            assert_eq!(undo.current.content_hash, read.content_hash);
            assert_eq!(
                plan_doc(&s.live).unwrap().semantic_hash().unwrap(),
                plan_doc(&s.shadow).unwrap().semantic_hash().unwrap()
            );
            assert!(journal(&s).has_draft);
            assert!(journal(&s).commit.is_none());
        }
        let author = f.owner.clone();
        let admin = if mode == "pending-grant" {
            let admin = identity(&mut f.engine, "Admin");
            f.ordered(EventBody::MemberAdded {
                identity: admin.clone(),
                role: Role::Owner,
            });
            Some(admin)
        } else {
            None
        };
        match mode {
            "reader" | "reader-unchanged" | "pending-grant" => {
                f.ordered(EventBody::MemberAdded {
                    identity: author.clone(),
                    role: Role::Reader,
                });
            }
            "removed" => {
                f.ordered(EventBody::MemberRemoved {
                    identity: author.clone(),
                });
            }
            "key-loss" => {
                f.engine.signers.remove(&author);
            }
            _ => unreachable!(),
        }
        if mode == "pending-grant" {
            // A different authenticated owner can queue an optimistic regrant; that
            // projection still must not create ordered authority for this editor.
            let writer = f.engine.author_for(admin.as_ref().unwrap()).unwrap();
            f.engine
                .append_synced(
                    &f.space,
                    &writer,
                    1_800_000_000_000,
                    EventBody::MemberAdded {
                        identity: author.clone(),
                        role: Role::Member,
                    },
                )
                .unwrap();
        }
        let readonly = s.read(&f.engine).unwrap();
        assert!(!readonly.can_edit);
        assert!(readonly.edit_context.is_empty());
        if mode == "reader-unchanged" {
            assert_eq!(readonly.view, read.view);
        } else {
            assert!(blocks(&readonly)
                .iter()
                .any(|b| b.plain_text() == "private intent remains"));
        }
        let before = exact(&f, &s);
        assert!(apply(&f, &mut s, &read, "accepted", std::slice::from_ref(&edit)).is_err());
        assert!(apply(&f, &mut s, &read, "new", &[edit]).is_err());
        assert!(s.commit(&mut f.engine, "forbidden").is_err());
        no_change(&f, &s, &before);
        assert!(journal(&s).has_draft);
    }
}

#[test]
fn newer_draft_survives_older_ack_and_permanent_transport_refusal() {
    let db = Database::new();
    let mut f = Fixture::at(&db.path());
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
    f.ack(&first_commit);
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
    f.ack(&retry);
    let current = s.read(&f.engine).unwrap();
    assert!(!current.unsaved);
    assert_eq!(current.view, newer.current.view);
    let peer = f.peer();
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
}

#[test]
fn blob_publication_and_two_first_writers_converge_without_cas() {
    for shorter_first in [false, true] {
        let mut receiver = Fixture::new();
        let mut shorter = receiver.peer();
        let mut short_session = shorter.session();
        let short_read = short_session.read(&shorter.engine).unwrap();
        let mut short_edit = text_edit(&short_read, "");
        short_edit.text = "z".repeat(55_000);
        let short_accept = apply(
            &shorter,
            &mut short_session,
            &short_read,
            "short",
            &[short_edit],
        )
        .unwrap();
        let short_commit = queued(
            short_session
                .commit(&mut shorter.engine, "shorter exact prefix")
                .unwrap(),
        );
        let short_event = shorter.pending(&short_commit);
        let EventBody::ItemVersioned {
            content: ItemContent::Page(page),
            ..
        } = &short_event.body
        else {
            panic!("Page")
        };
        let short_blob = page.blob.clone().expect("real >48KiB Plan blob");
        assert!(shorter.engine.store.uploads_pending().unwrap() > 0);
        let mut legacy = original();
        legacy.sections[0].lines[0].cost_cents = 777;
        legacy.sections[0].lines[0].done = true;
        legacy.budget_cents = Some(9_999);
        let legacy_event = receiver.ordered(receiver.version(ItemContent::Plan(legacy)));
        let mut longer = receiver.peer();
        let mut long_session = longer.session();
        let long_read = long_session.read(&longer.engine).unwrap();
        let mut title = changed(&blocks(&long_read)[0]);
        title.text = RAW.into();
        apply(&longer, &mut long_session, &long_read, "long", &[title]).unwrap();
        let long_commit = queued(
            long_session
                .commit(&mut longer.engine, "longer exact prefix")
                .unwrap(),
        );
        let long_event = longer.pending(&long_commit);
        // Both are already signed; no seen-based CAS or re-signing chooses the winner.
        let short_seen = short_event.seen.clone();
        let long_seen = long_event.seen.clone();
        let bytes = shorter.engine.media(&short_blob.sha256).unwrap().unwrap();
        receiver
            .engine
            .store
            .put_media(
                &short_blob.sha256,
                "application/vnd.zoen.page",
                &bytes,
                1_800_000_000_000,
            )
            .unwrap();
        longer
            .engine
            .store
            .put_media(
                &short_blob.sha256,
                "application/vnd.zoen.page",
                &bytes,
                1_800_000_000_000,
            )
            .unwrap();
        deliver(&mut shorter.engine, &legacy_event, Ingest::Applied);
        for event in if shorter_first {
            [&short_event, &long_event]
        } else {
            [&long_event, &short_event]
        } {
            deliver(&mut receiver.engine, event, Ingest::Applied);
            let short_expected = if event.client_id == short_event.client_id {
                Ingest::Confirmed
            } else {
                Ingest::Applied
            };
            deliver(&mut shorter.engine, event, short_expected);
            let long_expected = if event.client_id == long_event.client_id {
                Ingest::Confirmed
            } else {
                Ingest::Applied
            };
            deliver(&mut longer.engine, event, long_expected);
        }
        assert_eq!(short_event.seen, short_seen);
        assert_eq!(long_event.seen, long_seen);
        let target = receiver
            .engine
            .plan_timeline(&receiver.item)
            .unwrap()
            .document
            .unwrap();
        let a = short_session.read(&shorter.engine).unwrap();
        let b = long_session.read(&longer.engine).unwrap();
        assert_eq!(blocks(&a), target.blocks().unwrap());
        assert_eq!(a.view, b.view);
        assert!(!a.unsaved);
        assert!(!b.unsaved);
        let projected = target.view().unwrap().to_plan_doc();
        assert_eq!(projected.title, RAW);
        assert_eq!(projected.sections[0].lines[0].text, "z".repeat(55_000));
        assert_eq!(projected.sections[0].lines[0].cost_cents, 777);
        assert!(projected.sections[0].lines[0].done);
        assert_eq!(projected.budget_cents, Some(9_999));
        assert_eq!(
            target.revision().unwrap(),
            plan_doc(&short_session.live).unwrap().revision().unwrap()
        );
        assert_eq!(
            target.revision().unwrap(),
            plan_doc(&long_session.live).unwrap().revision().unwrap()
        );
        assert!(blocks(&short_accept.current)
            .iter()
            .any(|b| b.plain_text().len() == 55_000));
    }
}

// Both document tags use the existing device-key-sealed page.draft namespace.
// Exercise actual journals and a newly certified device, not fabricated ciphertext.
fn private_document_backup_source(
    kind: ItemKind,
) -> (Fixture, crate::backup::Header, DocumentView, DocumentView) {
    let mut engine = Engine::open(":memory:").unwrap();
    let (identity_secret, _, agreement_secret) = engine
        .create_account("Backup author", "backup_author", "http://127.0.0.1:1")
        .unwrap();
    let owner = engine.me.clone().unwrap();
    let mut f = Fixture {
        engine,
        owner: owner.clone(),
        space: new_id("document-backup-space"),
        item: new_id("document-backup-item"),
    };
    f.ordered(EventBody::SpaceCreated {
        title: "Document backup".into(),
        kind: SpaceKind::Group,
        privacy: Privacy::Public,
    });
    let content = match kind {
        ItemKind::Page => {
            let page = Page::from_markdown("# Published page\n\nPublished paragraph\n").unwrap();
            ItemContent::Page(PageContent {
                title: "Published page".into(),
                path: "notes/published.md".into(),
                loro: encode_bytes(&page.snapshot()),
                blob: None,
                manifest: None,
            })
        }
        ItemKind::Plan => ItemContent::Plan(original()),
        _ => panic!("document backup fixture requires Page or Plan"),
    };
    f.ordered(EventBody::ItemCreated {
        item: f.item.clone(),
        kind,
        content,
        origin: "Synthetic canonical backup fixture".into(),
    });
    f.engine.store.set_synced(&f.space).unwrap();
    f.engine.net.synced.insert(f.space.clone());
    let mut session = f.session();
    let observed = session.read(&f.engine).unwrap();
    assert!(observed.can_edit);
    assert!(!observed.unsaved);
    let events = f.engine.store.event_count().unwrap();
    let outbox = f.engine.store.outbox_len().unwrap();
    let accepted = private_document_backup_edit(
        &f,
        &mut session,
        &observed,
        "private-before-backup",
        "Device-private unsaved 雨 😀",
    )
    .unwrap();
    assert!(accepted.current.unsaved);
    assert_ne!(accepted.current.view, observed.view);
    assert_eq!(f.engine.store.event_count().unwrap(), events);
    assert_eq!(f.engine.store.outbox_len().unwrap(), outbox);
    assert_eq!(f.engine.item(&f.item).unwrap().version, 1);
    let header = crate::backup::Header {
        version: 1,
        identity: owner,
        relay_url: "http://127.0.0.1:1".into(),
        identity_secret: hex::encode(identity_secret),
        agreement_secret: hex::encode(agreement_secret),
        created_ms: 1,
    };
    (f, header, observed.view, accepted.current.view)
}

fn private_document_backup_edit(
    f: &Fixture,
    session: &mut PageSession,
    observed: &SessionRead,
    mutation: &str,
    text: &str,
) -> R<AcceptedEdit> {
    match &observed.view {
        DocumentView::Plan { .. } => {
            apply(f, session, observed, mutation, &[text_edit(observed, text)])
        }
        DocumentView::BinaryPage { .. } => {
            let page = f.engine.page(&f.item)?;
            let mut changed = page.blocks.last().unwrap().clone();
            changed.text = text.into();
            session.apply_from(
                &f.engine,
                mutation,
                &observed.edit_context,
                DocumentEdit::BinaryPage {
                    order: &page
                        .blocks
                        .iter()
                        .map(|block| block.id.clone())
                        .collect::<Vec<_>>(),
                    changed: &[changed],
                },
            )
        }
    }
}

#[test]
fn backup_export_excludes_actual_page_and_plan_private_journals_without_mutating_source() {
    for kind in [ItemKind::Page, ItemKind::Plan] {
        let (f, _, _, private) = private_document_backup_source(kind);
        let ciphertext = f.cipher().unwrap();
        let head = f.engine.logs[&f.space]
            .head()
            .map(|head| (head.seq, head.hash));
        let events = f.engine.store.event_count().unwrap();
        let outbox = f.engine.store.outbox_len().unwrap();
        let db = Database::new();
        std::fs::write(db.path(), f.engine.backup_snapshot().unwrap()).unwrap();
        let connection = rusqlite::Connection::open(db.path()).unwrap();
        let rows: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM meta WHERE key LIKE 'page.draft.%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            rows, 0,
            "a backup must not carry device-private document journals"
        );
        assert_eq!(f.cipher().as_deref(), Some(ciphertext.as_str()));
        let mut source_session = f.session();
        let source = source_session.read(&f.engine).unwrap();
        assert_eq!(source.view, private);
        assert!(source.can_edit && source.unsaved);
        assert_eq!(f.engine.store.event_count().unwrap(), events);
        assert_eq!(f.engine.store.outbox_len().unwrap(), outbox);
        assert_eq!(
            f.engine.logs[&f.space]
                .head()
                .map(|head| (head.seq, head.hash)),
            head
        );
    }
}

#[test]
fn backup_restore_strips_authentic_old_device_page_and_plan_journals_before_edit_admission() {
    for kind in [ItemKind::Page, ItemKind::Plan] {
        let (f, header, canonical, private) = private_document_backup_source(kind);
        let ciphertext = f.cipher().unwrap();
        let row = row_id(&f.engine, f.engine.item_state(&f.item).unwrap()).unwrap();
        let original_device = f.engine.account().unwrap().device.clone();
        let head = f.engine.logs[&f.space]
            .head()
            .map(|head| (head.seq, head.hash));
        let db = Database::new();
        std::fs::write(db.path(), f.engine.backup_snapshot().unwrap()).unwrap();
        let connection = rusqlite::Connection::open(db.path()).unwrap();
        // Test restore independently of export using the real old device's sealed row.
        connection
            .execute(
                "INSERT OR REPLACE INTO meta(key,value) VALUES (?1,?2)",
                rusqlite::params![row, ciphertext],
            )
            .unwrap();
        drop(connection);
        let mut restored = Engine::open(":memory:").unwrap();
        restored
            .restore_snapshot(
                &header,
                &std::fs::read(db.path()).unwrap(),
                roda_log::Signer::generate(),
            )
            .unwrap();
        assert_eq!(restored.account().unwrap().identity, f.owner);
        assert_ne!(restored.account().unwrap().device, original_device);
        assert!(restored.store.meta(&row).unwrap().is_none());
        assert_eq!(
            restored.logs[&f.space]
                .head()
                .map(|head| (head.seq, head.hash)),
            head
        );
        assert_eq!(restored.item(&f.item).unwrap().version, 1);
        let restored = Fixture {
            engine: restored,
            owner: f.owner.clone(),
            space: f.space.clone(),
            item: f.item.clone(),
        };
        let mut session = restored.session();
        let current = session.read(&restored.engine).unwrap();
        assert_eq!(current.view, canonical);
        assert!(!current.unsaved);
        assert!(
            current.can_edit,
            "foreign ciphertext must not lock the canonical document"
        );
        assert!(current.save_error.is_none());
        let accepted = private_document_backup_edit(
            &restored,
            &mut session,
            &current,
            "new-device-edit",
            "Fresh edit after recovery",
        )
        .unwrap();
        assert!(accepted.current.unsaved && accepted.current.can_edit);
        assert_ne!(accepted.current.view, canonical);
        assert_ne!(restored.cipher().as_deref(), Some(ciphertext.as_str()));
        assert_eq!(restored.engine.item(&restored.item).unwrap().version, 1);
        let mut original_session = f.session();
        assert_eq!(original_session.read(&f.engine).unwrap().view, private);
        assert_eq!(f.cipher().as_deref(), Some(ciphertext.as_str()));
    }
}

#[path = "input_tests.rs"]
mod input_tests;
