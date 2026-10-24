//! Shared fixture operations preserve the original clients, identities and stores.

use super::*;

pub(super) fn original() -> PlanDoc {
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

pub(super) fn identity(engine: &mut Engine, name: &str) -> String {
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

pub(super) fn envelope_frame(engine: &Engine, env: Envelope) -> Sequenced {
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

pub(super) fn deliver(engine: &mut Engine, event: &Event, expected: Ingest) -> Event {
    let frame = envelope_frame(engine, Envelope::plain(event));
    assert_eq!(engine.ingest(frame), expected);
    engine.logs[&event.space].events().last().unwrap().clone()
}

pub(super) fn blocks(read: &SessionRead) -> &[PlanBlock] {
    let DocumentView::Plan { blocks } = &read.view else {
        panic!("expected strict Plan projection")
    };
    blocks
}

pub(super) fn order_read(read: &SessionRead) -> Vec<String> {
    blocks(read).iter().map(|b| b.id.clone()).collect()
}

pub(super) fn order_doc(doc: &PlanDocument) -> Vec<String> {
    doc.blocks().unwrap().into_iter().map(|b| b.id).collect()
}

pub(super) fn changed(block: &PlanBlock) -> PlanBlockEdit {
    PlanBlockEdit {
        id: block.id.clone(),
        content: block.content.clone(),
        text: block.plain_text(),
        spans: documents::spans_of(&block.runs),
    }
}

pub(super) fn line(read: &SessionRead) -> PlanBlockEdit {
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

pub(super) fn text_edit(read: &SessionRead, text: &str) -> PlanBlockEdit {
    let mut edit = line(read);
    edit.text = text.into();
    edit
}

pub(super) fn apply(
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

pub(super) fn queued(status: CommitStatus) -> CommitRef {
    let CommitStatus::Queued(reference) = status else {
        panic!("expected queued publication: {status:?}")
    };
    reference
}

pub(super) fn journal(s: &PageSession) -> &PlanJournalV1 {
    let SessionJournal::Plan(journal) = &s.journal else {
        panic!("expected Plan journal")
    };
    journal
}

pub(super) fn exact(f: &Fixture, s: &PageSession) -> (Option<String>, Vec<u8>, u64, usize) {
    (
        f.cipher(),
        plan_doc(&s.live).unwrap().full_update().to_json().unwrap(),
        f.engine.store.outbox_len().unwrap(),
        f.engine.logs[&f.space].events().len(),
    )
}

pub(super) fn no_change(
    f: &Fixture,
    s: &PageSession,
    before: &(Option<String>, Vec<u8>, u64, usize),
) {
    assert_eq!(&exact(f, s), before);
}

pub(super) fn trigger(f: &Fixture, name: &str, action: &str, table: &str, condition: &str) {
    f.engine.store.conn().execute_batch(&format!("CREATE TRIGGER {name} BEFORE {action} ON {table} {condition} BEGIN SELECT RAISE(ABORT, 'actual injected SQL failure'); END;")).unwrap();
}

pub(super) fn drop_trigger(f: &Fixture, name: &str) {
    f.engine
        .store
        .conn()
        .execute_batch(&format!("DROP TRIGGER {name}"))
        .unwrap();
}

pub(super) fn edited_binary_fixture() -> (std::sync::Arc<Client>, String, PageSession) {
    let public = Client::open(":memory:".into(), "en".into()).unwrap();
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

pub(super) fn encrypted_exact_source_and_peer_readability(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
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
        space: new_id("mls-plan")?,
        item: new_id("mls-item")?,
    };
    f.ordered(EventBody::SpaceCreated {
        title: "Encrypted Plan".into(),
        kind: SpaceKind::Group,
        privacy: Privacy::EndToEnd,
    })?;
    f.ordered(EventBody::MemberAdded {
        identity: peer.me.clone().unwrap(),
        role: Role::Member,
    })?;
    f.ordered(EventBody::ItemCreated {
        item: f.item.clone(),
        kind: ItemKind::Plan,
        content: ItemContent::Plan(original()),
        origin: "authenticated legacy".into(),
    })?;
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
            ::mls::Opened::Commit { epoch: 1 }
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
                message: new_id("peer-readable")?,
                text: "peer after typed Plan".into(),
                attaches: None,
                reply: None,
                media: None,
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

    Ok(())
}

pub(super) fn private_document_backup_source(
    kind: ItemKind,
) -> std::result::Result<
    (Fixture, crate::backup::Header, DocumentView, DocumentView),
    Box<dyn std::error::Error>,
> {
    let mut engine = Engine::open(":memory:").unwrap();
    let (identity_secret, _, agreement_secret) = engine
        .create_account("Backup author", "backup_author", "http://127.0.0.1:1")
        .unwrap();
    let owner = engine.me.clone().unwrap();
    let mut f = Fixture {
        engine,
        owner: owner.clone(),
        space: new_id("document-backup-space")?,
        item: new_id("document-backup-item")?,
    };
    f.ordered(EventBody::SpaceCreated {
        title: "Document backup".into(),
        kind: SpaceKind::Group,
        privacy: Privacy::Public,
    })?;
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
    })?;
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
    Ok((f, header, observed.view, accepted.current.view))
}

pub(super) fn private_document_backup_edit(
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
