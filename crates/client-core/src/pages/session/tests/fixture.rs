//! Inherent operations on the one original Fixture type in the parent.

use super::*;

impl Fixture {
    pub(super) fn new() -> std::result::Result<Self, Box<dyn std::error::Error>> {
        Ok(Self::at(":memory:")?)
    }
    pub(super) fn at(path: &str) -> std::result::Result<Self, Box<dyn std::error::Error>> {
        Ok(Self::with_ids(
            path,
            new_id("plan-space")?,
            new_id("plan-item")?,
        )?)
    }
    pub(super) fn with_ids(
        path: &str,
        space: String,
        item: String,
    ) -> std::result::Result<Self, Box<dyn std::error::Error>> {
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
        })?;
        fixture.ordered(EventBody::ItemCreated {
            item: fixture.item.clone(),
            kind: ItemKind::Plan,
            content: ItemContent::Plan(original()),
            origin: "legacy user".into(),
        })?;
        Ok(fixture)
    }
    pub(super) fn reopen(self) -> Self {
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
    pub(super) fn session(&self) -> PageSession {
        PageSession::open(&self.engine, &self.item).unwrap()
    }
    pub(super) fn signed_as(
        &self,
        author: &str,
        body: EventBody,
    ) -> std::result::Result<Event, Box<dyn std::error::Error>> {
        Ok(self.engine.author_for(author).unwrap().sign_event(
            &self.space,
            &new_id("session-event")?,
            1_800_000_000_000,
            self.engine.logs.get(&self.space).and_then(|log| log.head()),
            body,
        ))
    }
    pub(super) fn ordered(
        &mut self,
        body: EventBody,
    ) -> std::result::Result<Event, Box<dyn std::error::Error>> {
        let event = self.signed_as(&self.owner, body)?;
        Ok(deliver(&mut self.engine, &event, Ingest::Applied))
    }
    pub(super) fn queue(&mut self, body: EventBody) -> Event {
        let author = self.engine.author_for(&self.owner).unwrap();
        self.engine
            .append_synced(&self.space, &author, 1_800_000_000_000, body)
            .unwrap()
    }
    pub(super) fn version(&self, content: ItemContent) -> EventBody {
        EventBody::ItemVersioned {
            item: self.item.clone(),
            content,
            note: "Exact version".into(),
        }
    }
    pub(super) fn page(&self, document: &PlanDocument) -> PageContent {
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
    pub(super) fn remote(
        &mut self,
        edit: PlanBlockEdit,
    ) -> std::result::Result<Event, Box<dyn std::error::Error>> {
        let fold = self.engine.plan_timeline(&self.item).unwrap();
        let mut doc = fold.document.as_ref().unwrap().fork().unwrap();
        let revision = doc.revision().unwrap();
        doc.apply_from(&revision, &order_doc(&doc), &[edit])
            .unwrap();
        Ok(self.ordered(self.version(ItemContent::Page(self.page(&doc))))?)
    }
    pub(super) fn pending(&self, reference: &CommitRef) -> Event {
        let client_id = match reference {
            CommitRef::Plan(c) => &c.client_id,
            CommitRef::BinaryPage { client_id } => client_id,
        };
        self.engine.store.outbox_get(client_id).unwrap().unwrap()
    }
    pub(super) fn ack(&mut self, reference: &CommitRef) -> Event {
        let event = self.pending(reference);
        deliver(&mut self.engine, &event, Ingest::Confirmed)
    }
    pub(super) fn cipher(&self) -> Option<String> {
        let it = self.engine.item_state(&self.item).unwrap();
        self.engine
            .store
            .meta(&row_id(&self.engine, it).unwrap())
            .unwrap()
    }
    pub(super) fn peer(&self) -> Fixture {
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
