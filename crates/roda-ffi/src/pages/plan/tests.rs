use super::*;
use crate::sync::Ingest;
use roda_docs::plan::{ExpectedLine, LinePatch, PlanCommand};
use roda_log::chain_hash;
use roda_proto::{Envelope, Sequenced};

mod api;
mod consumers;

struct Fixture {
    engine: Engine,
    space: String,
    item: String,
    owner: String,
}

fn original() -> PlanDoc {
    let external_id = "duplicated legacy id / 😀 / ".repeat(12);
    PlanDoc {
        title: "Trip\r\nexact spelling".into(),
        summary: "Keep CR\rLF\nNEL\u{85}paragraph\u{2029}bytes".into(),
        budget_cents: Some(1_000),
        sections: vec![PlanSection {
            title: "Stay".into(),
            lines: vec![
                PlanLine {
                    id: external_id.clone(),
                    text: "Hotel\r\nreservation".into(),
                    cost_cents: 200,
                    done: false,
                },
                PlanLine {
                    id: external_id,
                    text: "Legacy credit".into(),
                    cost_cents: -7,
                    done: true,
                },
            ],
        }],
    }
}

fn add_identity(engine: &mut Engine, name: &str) -> String {
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
            },
            true,
        )
        .unwrap()
}

fn frame(engine: &Engine, event: &Event) -> Sequenced {
    let head = engine.logs.get(&event.space).and_then(|log| log.head());
    let seq = head.as_ref().map_or(0, |head| head.seq + 1);
    let prev = head.map_or_else(|| GENESIS_PREV.into(), |head| head.hash);
    let env = Envelope::plain(event);
    Sequenced {
        seq,
        hash: chain_hash(&event.space, seq, &prev, &env.wire_hash()),
        prev,
        env,
    }
}

fn deliver(engine: &mut Engine, event: &Event, expected: Ingest) -> Event {
    let frame = frame(engine, event);
    assert_eq!(engine.ingest(frame), expected);
    engine.logs[&event.space].events().last().unwrap().clone()
}

impl Fixture {
    fn at(path: &str) -> Self {
        let mut engine = Engine::open(path).unwrap();
        let owner = add_identity(&mut engine, "Owner");
        engine.me = Some(owner.clone());
        engine.store.set_meta("me", &owner).unwrap();
        let mut fixture = Self {
            engine,
            space: new_id("plan-space"),
            item: new_id("legacy-plan"),
            owner,
        };
        fixture.ordered(EventBody::SpaceCreated {
            title: "Plan conversation".into(),
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

    fn new() -> Self {
        Self::at(":memory:")
    }

    fn signed_as(&self, actor: &str, body: EventBody) -> Event {
        self.engine.author_for(actor).unwrap().sign_event(
            &self.space,
            &new_id("plan-event"),
            1_800_000_000_000,
            self.engine.logs.get(&self.space).and_then(|log| log.head()),
            body,
        )
    }

    fn ordered_as(&mut self, actor: &str, body: EventBody) -> Event {
        let event = self.signed_as(actor, body);
        deliver(&mut self.engine, &event, Ingest::Applied)
    }

    fn ordered(&mut self, body: EventBody) -> Event {
        self.ordered_as(&self.owner.clone(), body)
    }

    fn version(&self, content: ItemContent) -> EventBody {
        EventBody::ItemVersioned {
            item: self.item.clone(),
            content,
            note: "Exact version".into(),
        }
    }

    fn undo(&self, to_version: u32) -> EventBody {
        EventBody::ItemReverted {
            item: self.item.clone(),
            to_version,
            note: "Exact undo".into(),
        }
    }

    fn queue(&mut self, body: EventBody) -> Event {
        let author = self.engine.author_for(&self.owner).unwrap();
        self.engine
            .append_synced(&self.space, &author, 1_800_000_000_000, body)
            .unwrap()
    }

    fn fold(&self) -> PlanFold {
        self.engine.plan_timeline(&self.item).unwrap()
    }
}

fn edited(document: &PlanDocument, patch: LinePatch) -> PlanDocument {
    let mut document = document.fork().unwrap();
    let block = document.view().unwrap().sections[0].lines[0]
        .block_id
        .clone();
    document
        .apply(PlanCommand::ChangeLine {
            block,
            expected: ExpectedLine::AnyCurrent,
            patch,
        })
        .unwrap();
    document
}

fn page(manifest: PlanManifestV1, document: &PlanDocument) -> PageContent {
    PageContent {
        title: document.view().unwrap().title,
        path: String::new(),
        loro: base64::engine::general_purpose::STANDARD
            .encode(document.full_update().to_json().unwrap()),
        blob: None,
        manifest: Some(PageManifest::PlanV1(manifest)),
    }
}

fn view(fold: &PlanFold) -> PlanDoc {
    fold.document
        .as_ref()
        .unwrap()
        .view()
        .unwrap()
        .to_plan_doc()
}

fn ordinary_journal(engine: &crate::RodaEngine) -> Vec<String> {
    let engine = engine.lock();
    let mut query = engine
        .store
        .conn()
        .prepare("SELECT value FROM meta WHERE key LIKE 'page.draft.%' ORDER BY key")
        .unwrap();
    let rows = query.query_map([], |row| row.get(0)).unwrap();
    rows.map(Result::unwrap).collect()
}

#[test]
fn ordinary_pages_keep_their_wire_bytes_and_durable_receipt_contexts() {
    let old = br#"{"title":"ordinary","loro":"YWJj"}"#;
    let decoded: PageContent = serde_json::from_slice(old).unwrap();
    assert!(decoded.manifest.is_none());
    assert_eq!(serde_json::to_vec(&decoded).unwrap(), old);
    let malformed =
        br#"{"title":"future","loro":"YWJj","manifest":{"schema":"plan_v1","future":7}}"#;
    let decoded: PageContent = serde_json::from_slice(malformed).unwrap();
    assert!(matches!(
        decoded.manifest,
        Some(PageManifest::Unsupported(_))
    ));
    assert_eq!(
        serde_json::to_value(&decoded).unwrap(),
        serde_json::from_slice::<serde_json::Value>(malformed).unwrap()
    );

    let engine = crate::RodaEngine::open(":memory:".into(), "en".into()).unwrap();
    engine.seed_demo_if_empty().unwrap();
    let space = engine
        .spaces()
        .into_iter()
        .find(|space| space.title == "Paraty with Marina")
        .unwrap()
        .id;
    let item = engine
        .page_import_markdown(
            space,
            "ordinary.md".into(),
            "# Ordinary\n\nPreserve this\n".into(),
        )
        .unwrap()
        .id;
    let observed = engine.page(item.clone()).unwrap();
    let mut blocks = observed.blocks.clone();
    blocks[1].text = "Ordinary draft survived".into();
    let accepted = engine
        .page_apply_from(
            item.clone(),
            "ordinary-mutation".into(),
            observed.edit_context.clone(),
            blocks.iter().map(|block| block.id.clone()).collect(),
            vec![blocks[1].clone()],
        )
        .unwrap();
    let journal = ordinary_journal(&engine);
    assert_eq!(journal.len(), 1);
    assert!(!journal[0].contains("Ordinary draft survived"));
    engine.lock().reload().unwrap();
    let retry = engine
        .page_apply_from(
            item.clone(),
            "ordinary-mutation".into(),
            observed.edit_context,
            blocks.iter().map(|block| block.id.clone()).collect(),
            vec![blocks[1].clone()],
        )
        .unwrap();
    assert!(retry.replayed);
    assert_eq!(retry.applied_content_hash, accepted.applied_content_hash);
    assert_eq!(retry.page.blocks, blocks);
    assert_eq!(ordinary_journal(&engine), journal);
    assert!(matches!(
        engine.lock().plan_timeline(&item),
        Err(CoreError::Invalid { .. })
    ));
    assert!(engine.lock().state.items[&item].versions.iter().all(
        |version| matches!(&version.content, ItemContent::Page(page) if page.manifest.is_none())
    ));
}

#[test]
fn pending_legacy_snapshots_and_undos_never_enlarge_the_authenticated_seed() {
    let mut fixture = Fixture::new();
    let baseline = fixture.fold();
    let manifest = baseline.manifest().unwrap();
    let version = baseline.document.as_ref().unwrap().version();
    let mut changed = original();
    changed.summary = "Only an unconfirmed intent".into();
    let pending = fixture.queue(fixture.version(ItemContent::Plan(changed.clone())));
    let undo = fixture.queue(fixture.undo(1));
    let fold = fixture.fold();
    assert!(fold.pending_legacy);
    assert!(!fold.writable());
    assert!(fold.manifest().is_err());
    assert_eq!(view(&fold), original());
    assert_eq!(fold.document.as_ref().unwrap().version(), version);
    assert_eq!(fold.manifest.as_ref(), Some(&manifest));
    assert_eq!(fold.slots.len(), 3);
    assert!(fold.slots[1..].iter().all(
        |slot| matches!(slot.acceptance, Acceptance::Pending) && slot.source.ordered.is_none()
    ));
    assert!(!fold.acknowledges(
        &pending.client_id,
        &source(&fixture.space, &fixture.item, &pending, false).event_id
    ));

    deliver(&mut fixture.engine, &pending, Ingest::Confirmed);
    assert!(fixture.fold().pending_legacy); // The still-pending undo retains its intent.
    deliver(&mut fixture.engine, &undo, Ingest::Confirmed);
    let confirmed = fixture.fold();
    assert!(confirmed.writable());
    assert_eq!(view(&confirmed), original());
    assert_eq!(confirmed.manifest().unwrap().legacy_versions, 3);
    assert_ne!(confirmed.document.as_ref().unwrap().version(), version);
    assert!(
        confirmed
            .slots
            .iter()
            .all(|slot| matches!(slot.acceptance, Acceptance::Legacy)
                && slot.source.ordered.is_some())
    );
    assert!(fixture.engine.store.outbox().unwrap().is_empty());
}

#[test]
fn refused_legacy_intent_stays_readable_without_blocking_a_new_promotion() {
    let mut fixture = Fixture::new();
    let pending = fixture.queue(fixture.version(ItemContent::Plan(PlanDoc {
        summary: "Refused text remains visible".into(),
        ..original()
    })));
    assert!(fixture.fold().pending_legacy);
    assert!(fixture
        .engine
        .reject(&pending.client_id, "permission", true, None));
    let fold = fixture.fold();
    assert!(fold.writable());
    assert_eq!(view(&fold), original());
    assert_eq!(fold.manifest().unwrap().legacy_versions, 1);
    assert_eq!(
        fold.slots[1].content,
        Some(ItemContent::Plan(PlanDoc {
            summary: "Refused text remains visible".into(),
            ..original()
        }))
    );
    assert_eq!(
        fold.slots[1].acceptance,
        Acceptance::TransportRefused("permission".into())
    );
    assert!(fold.slots[1].source.ordered.is_none());
    assert_eq!(
        fixture.engine.store.outbox().unwrap()[0].event.content,
        pending.content
    );
}

#[test]
fn a_pending_creation_has_no_verified_seed_or_promotion_manifest() {
    let mut fixture = Fixture::new();
    let item = new_id("pending-plan");
    fixture.queue(EventBody::ItemCreated {
        item: item.clone(),
        kind: ItemKind::Plan,
        content: ItemContent::Plan(original()),
        origin: "offline".into(),
    });
    let fold = fixture.engine.plan_timeline(&item).unwrap();
    assert!(fold.document.is_none());
    assert!(fold.frozen_at.is_none());
    assert!(fold.pending_legacy);
    assert!(fold.manifest().is_err());
    assert_eq!(fold.slots.len(), 1);
    assert_eq!(fold.slots[0].acceptance, Acceptance::Pending);
}

#[test]
fn an_ordered_remote_cutover_preserves_but_never_imports_local_legacy_intent() {
    let mut fixture = Fixture::new();
    let baseline = fixture.fold();
    let document = edited(
        baseline.document.as_ref().unwrap(),
        LinePatch {
            done: Some(true),
            ..Default::default()
        },
    );
    let remote = fixture.signed_as(
        &fixture.owner,
        fixture.version(ItemContent::Page(page(
            baseline.manifest().unwrap(),
            &document,
        ))),
    );
    let intent = PlanDoc {
        summary: "Unconfirmed whole-document intent".into(),
        ..original()
    };
    let pending = fixture.queue(fixture.version(ItemContent::Plan(intent.clone())));
    assert!(!fixture.fold().writable());
    let ordered = deliver(&mut fixture.engine, &remote, Ingest::Applied);
    let cutover = fixture.fold();
    assert_eq!(
        cutover.frozen_at,
        Some(source(&fixture.space, &fixture.item, &ordered, true))
    );
    assert_eq!(view(&cutover), document.view().unwrap().to_plan_doc());
    assert!(cutover.pending_legacy);
    assert!(!cutover.writable());
    assert_eq!(
        cutover.slots.last().unwrap().content,
        Some(ItemContent::Plan(intent.clone()))
    );
    assert_eq!(
        cutover.slots.last().unwrap().acceptance,
        Acceptance::Pending
    );

    deliver(&mut fixture.engine, &pending, Ingest::Confirmed);
    let resolved = fixture.fold();
    assert!(resolved.writable());
    assert_eq!(view(&resolved), document.view().unwrap().to_plan_doc());
    assert_eq!(resolved.manifest().unwrap().legacy_versions, 1);
    assert_eq!(
        resolved.slots.last().unwrap().content,
        Some(ItemContent::Plan(intent))
    );
    assert!(matches!(
        resolved.slots.last().unwrap().acceptance,
        Acceptance::Refused(_)
    ));
}

#[test]
fn shorter_exact_ancestry_and_two_first_writers_merge_in_both_relay_orders() {
    let mut fixture = Fixture::new();
    let first = fixture.fold();
    let first_editor = edited(
        first.document.as_ref().unwrap(),
        LinePatch {
            text: Some("Text from the earlier prefix".into()),
            ..Default::default()
        },
    );
    let first_page = page(first.manifest().unwrap(), &first_editor);
    let queued = fixture.queue(fixture.version(ItemContent::Page(first_page)));
    let pending_fold = fixture.fold();
    assert!(pending_fold.frozen_at.is_none());
    assert_eq!(view(&pending_fold), original());

    let mut legacy_v2 = original();
    legacy_v2.title = "Title from authenticated legacy v2".into();
    legacy_v2.sections[0].lines[0].cost_cents = 700;
    fixture.ordered(fixture.version(ItemContent::Plan(legacy_v2.clone())));
    let second = fixture.fold();
    let second_editor = edited(
        second.document.as_ref().unwrap(),
        LinePatch {
            done: Some(true),
            ..Default::default()
        },
    );
    let second_page = page(second.manifest().unwrap(), &second_editor);
    let second_event = fixture.signed_as(
        &fixture.owner,
        fixture.version(ItemContent::Page(second_page)),
    );
    let history = fixture.engine.store.events(&fixture.space).unwrap();
    let mut reverse = Engine::open(":memory:").unwrap();
    for event in &history {
        deliver(&mut reverse, event, Ingest::Applied);
    }

    deliver(&mut fixture.engine, &queued, Ingest::Confirmed);
    deliver(&mut fixture.engine, &second_event, Ingest::Applied);
    deliver(&mut reverse, &second_event, Ingest::Applied);
    deliver(&mut reverse, &queued, Ingest::Applied);
    let forward = fixture.fold();
    let backward = reverse.plan_timeline(&fixture.item).unwrap();
    assert_eq!(view(&forward), view(&backward));
    assert_eq!(
        forward.document.as_ref().unwrap().version(),
        backward.document.as_ref().unwrap().version()
    );
    let result = view(&forward);
    assert_eq!(result.title, legacy_v2.title);
    assert_eq!(result.summary, original().summary);
    assert_eq!(
        result.sections[0].lines[0].text,
        "Text from the earlier prefix"
    );
    assert_eq!(result.sections[0].lines[0].cost_cents, 700);
    assert!(result.sections[0].lines[0].done);
    assert_eq!(result.sections[0].lines[1], original().sections[0].lines[1]);
    for fold in [&forward, &backward] {
        assert!(fold.writable());
        assert_eq!(fold.manifest().unwrap().legacy_versions, 2);
        assert!(fold.slots[2..]
            .iter()
            .all(|slot| matches!(slot.acceptance, Acceptance::Canonical(_))));
        assert!(fold.acknowledges(
            &queued.client_id,
            &source(&fixture.space, &fixture.item, &queued, false).event_id
        ));
        assert!(fold.acknowledges(
            &second_event.client_id,
            &source(&fixture.space, &fixture.item, &second_event, false).event_id
        ));
    }
}

#[test]
fn invalid_manifests_and_inline_payloads_do_not_freeze_the_first_valid_page() {
    let mut fixture = Fixture::new();
    let baseline = fixture.fold();
    let document = baseline.document.as_ref().unwrap();
    let valid = page(baseline.manifest().unwrap(), document);
    let mut variants = Vec::new();
    for field in 0..8 {
        let mut manifest = baseline.manifest().unwrap();
        match field {
            0 => manifest.schema = "future_version".into(),
            1 => manifest.space = "other-space".into(),
            2 => manifest.item = "other-item".into(),
            3 => manifest.creation_event = "other-origin".into(),
            4 => manifest.legacy_versions = 0,
            5 => manifest.legacy_versions = 2,
            6 => manifest.legacy_head = "other-head".into(),
            _ => manifest.prefix_sha256 = "other-prefix".into(),
        }
        variants.push(PageContent {
            manifest: Some(PageManifest::PlanV1(manifest)),
            ..valid.clone()
        });
    }
    variants.push(PageContent {
        manifest: None,
        ..valid.clone()
    });
    variants.push(PageContent {
        manifest: Some(PageManifest::Unsupported(
            serde_json::json!({"schema":"plan_v1","extra":true}),
        )),
        ..valid.clone()
    });
    variants.push(PageContent {
        loro: "not base64!".into(),
        ..valid.clone()
    });
    variants.push(PageContent {
        loro: base64::engine::general_purpose::STANDARD.encode(b"invalid JSON codec"),
        ..valid.clone()
    });
    for variant in variants {
        let event = fixture.ordered(fixture.version(ItemContent::Page(variant)));
        let fold = fixture.fold();
        assert!(matches!(
            fold.slots.last().unwrap().acceptance,
            Acceptance::Refused(_)
        ));
        assert!(fold.frozen_at.is_none());
        assert!(!fold.waiting);
        assert_eq!(fold.manifest().unwrap(), baseline.manifest().unwrap());
        assert_eq!(
            fold.document.as_ref().unwrap().version(),
            document.version()
        );
        assert!(!fold.acknowledges(
            &event.client_id,
            &source(&fixture.space, &fixture.item, &event, true).event_id
        ));
    }
    let accepted = fixture.ordered(fixture.version(ItemContent::Page(valid)));
    let fold = fixture.fold();
    assert_eq!(
        fold.frozen_at,
        Some(source(&fixture.space, &fixture.item, &accepted, true))
    );
    assert!(matches!(
        fold.slots.last().unwrap().acceptance,
        Acceptance::Canonical(_)
    ));
}

#[test]
fn a_missing_blob_blocks_later_legacy_history_until_its_exact_bytes_arrive() {
    let mut fixture = Fixture::new();
    let baseline = fixture.fold();
    let document = edited(
        baseline.document.as_ref().unwrap(),
        LinePatch {
            done: Some(true),
            ..Default::default()
        },
    );
    let bytes = document.full_update().to_json().unwrap();
    let sha = hex::encode(Sha256::digest(&bytes));
    let content = PageContent {
        loro: String::new(),
        blob: Some(ChunkRef {
            sha256: sha.clone(),
            bytes: bytes.len() as u64,
            blob: "ab".repeat(32),
            key: "cd".repeat(32),
        }),
        ..page(baseline.manifest().unwrap(), &document)
    };
    let waiting = fixture.ordered(fixture.version(ItemContent::Page(content)));
    let late = fixture.ordered(fixture.version(ItemContent::Plan(PlanDoc {
        summary: "Late old client write".into(),
        ..original()
    })));
    let before = fixture.fold();
    assert!(before.waiting);
    assert!(!before.writable());
    assert!(before.manifest().is_err());
    assert_eq!(view(&before), original());
    assert!(before.frozen_at.is_none());
    assert!(before.slots[1..]
        .iter()
        .all(|slot| matches!(slot.acceptance, Acceptance::Waiting(_))));
    assert!(!fixture
        .engine
        .import_media(&sha, b"wrong blob", "application/json")
        .unwrap());
    assert!(fixture.fold().waiting);
    assert!(fixture
        .engine
        .import_media(&sha, &bytes, "application/json")
        .unwrap());
    let after = fixture.fold();
    assert!(!after.waiting);
    assert!(after.writable());
    assert_eq!(
        after.frozen_at,
        Some(source(&fixture.space, &fixture.item, &waiting, true))
    );
    assert_eq!(after.manifest().unwrap().legacy_versions, 1);
    assert_eq!(view(&after), document.view().unwrap().to_plan_doc());
    assert!(matches!(after.slots[2].acceptance, Acceptance::Refused(_)));
    assert_eq!(
        after.slots[2].content,
        Some(ItemContent::Plan(PlanDoc {
            summary: "Late old client write".into(),
            ..original()
        }))
    );
    assert!(!after.acknowledges(
        &late.client_id,
        &source(&fixture.space, &fixture.item, &late, true).event_id
    ));
}

#[test]
fn damaged_cached_media_waits_and_storage_failure_is_not_an_author_refusal() {
    let mut fixture = Fixture::new();
    let baseline = fixture.fold();
    let document = baseline.document.as_ref().unwrap();
    let bytes = document.full_update().to_json().unwrap();
    let sha = hex::encode(Sha256::digest(&bytes));
    let content = PageContent {
        loro: String::new(),
        blob: Some(ChunkRef {
            sha256: sha.clone(),
            bytes: bytes.len() as u64,
            blob: "ab".repeat(32),
            key: "cd".repeat(32),
        }),
        ..page(baseline.manifest().unwrap(), document)
    };
    fixture.ordered(fixture.version(ItemContent::Page(content)));
    fixture
        .engine
        .store
        .put_media(&sha, "application/json", &vec![b'x'; bytes.len()], 0)
        .unwrap();
    assert!(fixture.fold().waiting);
    fixture
        .engine
        .store
        .conn()
        .execute(
            "UPDATE media SET bytes = ?2 WHERE sha256 = ?1",
            rusqlite::params![sha, bytes],
        )
        .unwrap();
    assert!(fixture.fold().frozen_at.is_some());
    let saved = fixture.engine.store.events(&fixture.space).unwrap();
    fixture
        .engine
        .store
        .conn()
        .execute_batch("DROP TABLE media")
        .unwrap();
    assert!(matches!(
        fixture.engine.plan_timeline(&fixture.item),
        Err(CoreError::Storage { .. })
    ));
    assert_eq!(fixture.engine.store.events(&fixture.space).unwrap(), saved);
}

#[test]
fn invalid_undo_sources_do_not_shift_legacy_version_addresses() {
    let mut fixture = Fixture::new();
    fixture.ordered(fixture.undo(99));
    let second = PlanDoc {
        summary: "Legacy second version".into(),
        ..original()
    };
    fixture.ordered(fixture.version(ItemContent::Plan(second.clone())));
    let undo_second = fixture.ordered(fixture.undo(2));
    let undo_first = fixture.ordered(fixture.undo(1));
    let fold = fixture.fold();
    assert_eq!(
        fold.slots
            .iter()
            .map(|slot| slot.number)
            .collect::<Vec<_>>(),
        vec![Some(1), None, Some(2), Some(3), Some(4)]
    );
    assert!(matches!(fold.slots[1].acceptance, Acceptance::Refused(_)));
    assert_eq!(fold.slots[3].content, Some(ItemContent::Plan(second)));
    assert_eq!(fold.slots[3].reverted_to, Some(2));
    assert_eq!(fold.slots[4].reverted_to, Some(1));
    assert_eq!(view(&fold), original());
    assert_eq!(fold.manifest().unwrap().legacy_versions, 4);
    assert_ne!(
        source(&fixture.space, &fixture.item, &undo_second, true).event_id,
        source(&fixture.space, &fixture.item, &undo_first, true).event_id
    );
    assert_eq!(
        fixture.engine.state.items[&fixture.item]
            .versions
            .iter()
            .map(|version| version.number)
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4]
    );
}

#[test]
fn transport_confirmation_cannot_ack_a_semantically_refused_page_or_late_legacy_undo() {
    let mut fixture = Fixture::new();
    let baseline = fixture.fold();
    let document = edited(
        baseline.document.as_ref().unwrap(),
        LinePatch {
            text: Some("Canonical saved text".into()),
            ..Default::default()
        },
    );
    let valid = page(baseline.manifest().unwrap(), &document);
    let refused = fixture.queue(fixture.version(ItemContent::Page(PageContent {
        manifest: None,
        ..valid.clone()
    })));
    let refused_source = source(&fixture.space, &fixture.item, &refused, false);
    deliver(&mut fixture.engine, &refused, Ingest::Confirmed);
    assert!(!fixture
        .fold()
        .acknowledges(&refused.client_id, &refused_source.event_id));
    assert!(fixture.fold().frozen_at.is_none());

    let valid = fixture.queue(fixture.version(ItemContent::Page(valid)));
    let valid_source = source(&fixture.space, &fixture.item, &valid, false);
    assert!(!fixture
        .fold()
        .acknowledges(&valid.client_id, &valid_source.event_id));
    let confirmed = deliver(&mut fixture.engine, &valid, Ingest::Confirmed);
    let fold = fixture.fold();
    assert_eq!(fold.slots[2].source.event_id, valid_source.event_id);
    assert!(fold.acknowledges(&valid.client_id, &valid_source.event_id));
    assert!(!fold.acknowledges(&valid.client_id, &refused_source.event_id));
    assert!(!fold.acknowledges(&refused.client_id, &valid_source.event_id));
    let frozen = fold.frozen_at;
    let undo = fixture.ordered(fixture.undo(1));
    let old_write = fixture.ordered(fixture.version(ItemContent::Plan(original())));
    let fold = fixture.fold();
    assert_eq!(fold.frozen_at, frozen);
    assert_eq!(fold.accepted_current, Some(3));
    assert_eq!(view(&fold), document.view().unwrap().to_plan_doc());
    assert_eq!(fold.manifest().unwrap().legacy_versions, 1);
    assert_eq!(fold.slots[3].reverted_to, Some(1));
    assert_eq!(fold.slots[3].content, Some(ItemContent::Plan(original())));
    for event in [&undo, &old_write] {
        assert!(!fold.acknowledges(
            &event.client_id,
            &source(&fixture.space, &fixture.item, event, true).event_id
        ));
    }
    assert_eq!(
        fold.slots[2].source.ordered,
        Some((confirmed.seq, confirmed.hash))
    );
}

#[test]
fn reader_and_removed_member_frames_never_authorize_canonical_plan_mutations() {
    let mut fixture = Fixture::new();
    let member = add_identity(&mut fixture.engine, "Member");
    fixture.ordered(EventBody::MemberAdded {
        identity: member.clone(),
        role: Role::Reader,
    });
    let baseline = fixture.fold();
    let document = edited(
        baseline.document.as_ref().unwrap(),
        LinePatch {
            done: Some(true),
            ..Default::default()
        },
    );
    let content = page(baseline.manifest().unwrap(), &document);
    let reader = fixture.ordered_as(&member, fixture.version(ItemContent::Page(content.clone())));
    assert!(matches!(
        fixture.fold().slots.last().unwrap().acceptance,
        Acceptance::Refused(_)
    ));
    assert!(fixture.fold().frozen_at.is_none());
    fixture.ordered(EventBody::MemberAdded {
        identity: member.clone(),
        role: Role::Member,
    });
    let authorized =
        fixture.ordered_as(&member, fixture.version(ItemContent::Page(content.clone())));
    assert_eq!(
        view(&fixture.fold()),
        document.view().unwrap().to_plan_doc()
    );
    fixture.ordered(EventBody::MemberRemoved {
        identity: member.clone(),
    });
    let removed = fixture.ordered_as(&member, fixture.version(ItemContent::Page(content.clone())));
    let fold = fixture.fold();
    assert_eq!(
        fold.frozen_at,
        Some(source(&fixture.space, &fixture.item, &authorized, true))
    );
    for refused in [&reader, &removed] {
        assert!(!fold.acknowledges(
            &refused.client_id,
            &source(&fixture.space, &fixture.item, refused, true).event_id
        ));
    }
    assert!(matches!(
        fold.slots.last().unwrap().acceptance,
        Acceptance::Refused(_)
    ));
    fixture.ordered(fixture.version(ItemContent::Page(content)));
    assert!(matches!(
        fixture.fold().slots.last().unwrap().acceptance,
        Acceptance::Canonical(_)
    ));
}

#[test]
fn invalid_signatures_and_oversized_payloads_cannot_establish_cutover() {
    let mut fixture = Fixture::new();
    let baseline = fixture.fold();
    let content = page(
        baseline.manifest().unwrap(),
        baseline.document.as_ref().unwrap(),
    );
    let event = fixture.signed_as(
        &fixture.owner,
        fixture.version(ItemContent::Page(content.clone())),
    );
    let mut forged = frame(&fixture.engine, &event);
    forged.env.sig = "00".repeat(64);
    assert!(matches!(fixture.engine.ingest(forged), Ingest::Invalid(_)));
    assert_eq!(fixture.fold().slots.len(), 1);
    assert!(fixture.fold().frozen_at.is_none());
    let huge = PageContent {
        loro: "a".repeat(MAX_PLAN_PACKET_BYTES.div_ceil(3) * 4 + 1),
        ..content.clone()
    };
    fixture.ordered(fixture.version(ItemContent::Page(huge)));
    let declared = PageContent {
        loro: String::new(),
        blob: Some(ChunkRef {
            sha256: "ab".repeat(32),
            bytes: MAX_PLAN_PACKET_BYTES as u64 + 1,
            blob: "cd".repeat(32),
            key: "ef".repeat(32),
        }),
        ..content
    };
    fixture.ordered(fixture.version(ItemContent::Page(declared)));
    let fold = fixture.fold();
    assert!(fold.slots[1..]
        .iter()
        .all(|slot| matches!(slot.acceptance, Acceptance::Refused(_))));
    assert!(fold.frozen_at.is_none());
    assert!(!fold.waiting);
    assert_eq!(view(&fold), original());
}

#[test]
fn reopening_the_real_database_reverifies_identical_sources_history_and_cutover() {
    let directory = std::env::temp_dir().join(new_id("plan-timeline-reopen"));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("plan.sqlite");
    let mut fixture = Fixture::at(path.to_str().unwrap());
    let baseline = fixture.fold();
    let document = edited(
        baseline.document.as_ref().unwrap(),
        LinePatch {
            done: Some(true),
            ..Default::default()
        },
    );
    fixture.ordered(fixture.version(ItemContent::Page(page(
        baseline.manifest().unwrap(),
        &document,
    ))));
    fixture.ordered(fixture.undo(1));
    let pending = fixture.queue(fixture.version(ItemContent::Plan(PlanDoc {
        summary: "Retained failed legacy intent".into(),
        ..original()
    })));
    assert!(fixture
        .engine
        .reject(&pending.client_id, "old_client", true, None));
    let before = fixture.fold();
    let raw = fixture.engine.store.events(&fixture.space).unwrap();
    let space = fixture.space.clone();
    let item = fixture.item.clone();
    drop(fixture);
    let reopened = Engine::open(path.to_str().unwrap()).unwrap();
    let after = reopened.plan_timeline(&item).unwrap();
    assert_eq!(after.slots, before.slots);
    assert_eq!(after.manifest().unwrap(), before.manifest().unwrap());
    assert_eq!(after.frozen_at, before.frozen_at);
    assert_eq!(after.accepted_current, before.accepted_current);
    assert_eq!(view(&after), view(&before));
    assert_eq!(
        after.document.as_ref().unwrap().version(),
        before.document.as_ref().unwrap().version()
    );
    assert_eq!(reopened.store.events(&space).unwrap(), raw);
    assert_eq!(reopened.logs[&space].events(), raw);
    assert!(after.writable());
    assert!(matches!(
        after.slots.last().unwrap().acceptance,
        Acceptance::TransportRefused(_)
    ));
    drop(reopened);
    std::fs::remove_dir_all(directory).unwrap();
}
