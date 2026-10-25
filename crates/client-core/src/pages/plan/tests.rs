use super::*;
use crate::sync::Ingest;
use documents::plan::{ExpectedLine, LinePatch, PlanCommand};
use event_log::chain_hash;
use protocol::{Envelope, Sequenced};

mod api;
mod approvals;
mod consumers;
mod observed;
mod prefix;

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
                owner_proof: None,
            },
            true,
        )
        .unwrap()
}

fn frame(
    engine: &Engine,
    event: &Event,
) -> std::result::Result<Sequenced, Box<dyn std::error::Error>> {
    let head = engine.logs.get(&event.space).and_then(|log| log.head());
    let seq = head.as_ref().map_or(0, |head| head.seq + 1);
    let prev = head.map_or_else(|| GENESIS_PREV.into(), |head| head.hash);
    let env = Envelope::plain(event)?;
    Ok(Sequenced {
        seq,
        hash: chain_hash(&event.space, seq, &prev, &env.wire_hash())?,
        prev,
        env,
    })
}

fn deliver(
    engine: &mut Engine,
    event: &Event,
    expected: Ingest,
) -> std::result::Result<Event, Box<dyn std::error::Error>> {
    let frame = frame(engine, event)?;
    assert_eq!(engine.ingest(frame), expected);
    Ok(engine.logs[&event.space].events().last().unwrap().clone())
}

impl Fixture {
    fn at(path: &str) -> std::result::Result<Self, Box<dyn std::error::Error>> {
        let mut engine = Engine::open(path).unwrap();
        let owner = add_identity(&mut engine, "Owner");
        engine.me = Some(owner.clone());
        engine.store.set_meta("me", &owner).unwrap();
        let mut fixture = Self {
            engine,
            space: new_id("plan-space")?,
            item: new_id("legacy-plan")?,
            owner,
        };
        fixture.ordered(EventBody::SpaceCreated {
            title: "Plan conversation".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::Public,
        })?;
        fixture.ordered(EventBody::ItemCreated {
            item: fixture.item.clone(),
            kind: ItemKind::Plan,
            content: ItemContent::Plan(original()),
            origin: "legacy user".into(),
        })?;
        Ok(fixture)
    }

    fn new() -> std::result::Result<Self, Box<dyn std::error::Error>> {
        Self::at(":memory:")
    }

    fn signed_as(
        &self,
        actor: &str,
        body: EventBody,
    ) -> std::result::Result<Event, Box<dyn std::error::Error>> {
        Ok(self.engine.author_for(actor).unwrap().sign_event(
            &self.space,
            &new_id("plan-event")?,
            1_800_000_000_000,
            self.engine.logs.get(&self.space).and_then(|log| log.head()),
            body,
        )?)
    }

    fn ordered_as(
        &mut self,
        actor: &str,
        body: EventBody,
    ) -> std::result::Result<Event, Box<dyn std::error::Error>> {
        let event = self.signed_as(actor, body)?;
        deliver(&mut self.engine, &event, Ingest::Applied)
    }

    fn ordered(
        &mut self,
        body: EventBody,
    ) -> std::result::Result<Event, Box<dyn std::error::Error>> {
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

fn ordinary_journal(engine: &crate::Client) -> Vec<String> {
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

    let engine = crate::Client::open(":memory:".into(), "en".into()).unwrap();
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
fn a_missing_blob_blocks_later_legacy_history_until_its_exact_bytes_arrive() {
    let mut fixture = Fixture::new().unwrap();
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
    let waiting = fixture
        .ordered(fixture.version(ItemContent::Page(content)))
        .unwrap();
    let late = fixture
        .ordered(fixture.version(ItemContent::Plan(PlanDoc {
            summary: "Late old client write".into(),
            ..original()
        })))
        .unwrap();
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
    let mut fixture = Fixture::new().unwrap();
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
    fixture
        .ordered(fixture.version(ItemContent::Page(content)))
        .unwrap();
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
    let mut fixture = Fixture::new().unwrap();
    fixture.ordered(fixture.undo(99)).unwrap();
    let second = PlanDoc {
        summary: "Legacy second version".into(),
        ..original()
    };
    fixture
        .ordered(fixture.version(ItemContent::Plan(second.clone())))
        .unwrap();
    let undo_second = fixture.ordered(fixture.undo(2)).unwrap();
    let undo_first = fixture.ordered(fixture.undo(1)).unwrap();
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
fn reopening_the_real_database_reverifies_identical_sources_history_and_cutover() {
    let directory = std::env::temp_dir().join(new_id("plan-timeline-reopen").unwrap());
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("plan.sqlite");
    let mut fixture = Fixture::at(path.to_str().unwrap()).unwrap();
    let baseline = fixture.fold();
    let document = edited(
        baseline.document.as_ref().unwrap(),
        LinePatch {
            done: Some(true),
            ..Default::default()
        },
    );
    fixture
        .ordered(fixture.version(ItemContent::Page(page(
            baseline.manifest().unwrap(),
            &document,
        ))))
        .unwrap();
    fixture.ordered(fixture.undo(1)).unwrap();
    let pending = fixture.queue(fixture.version(ItemContent::Plan(PlanDoc {
        summary: "Retained failed legacy intent".into(),
        ..original()
    })));
    assert!(fixture
        .engine
        .reject(&pending.client_id, "old_client", true, None)
        .user_failed());
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

mod cutover_cases;

mod authority_cases;
