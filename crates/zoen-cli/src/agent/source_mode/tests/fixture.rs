//! Own ephemeral homes and real signed/projected events; no user DB or provider.
use super::*;
use client_core::SecretVault;
use event_log::{Signer, SpaceLog};
use relay::Vault;

pub(super) struct Fixture {
    pub path: std::path::PathBuf,
    pub client: Arc<Client>,
    pub vault: Arc<Vault>,
    owner_vault: Arc<Vault>,
    pub owner: String,
    pub space: String,
    pub source: String,
    pub other: String,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.close();
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
impl Fixture {
    fn close(&mut self) {
        self.client.stop_sync();
        let empty = Client::open(":memory:".into(), "en".into()).unwrap();
        let current = std::mem::replace(&mut self.client, empty);
        assert_eq!(
            Arc::strong_count(&current),
            1,
            "fixture must own the only Client before offline transfer"
        );
        drop(current); // release the real Store's exclusive SQLite connection
    }
    pub(super) fn new(relay: &str) -> std::result::Result<Self, Box<dyn std::error::Error>> {
        let path = std::env::temp_dir().join(types::new_id("host-source-test")?);
        std::fs::create_dir(&path).unwrap();
        let owner_vault = Arc::new(Vault::default());
        let human = Client::open(
            path.join("owner.sqlite").to_str().unwrap().into(),
            "en".into(),
        )
        .unwrap();
        let owner = human
            .create_account(
                "Source owner".into(),
                "source_owner".into(),
                relay.into(),
                owner_vault.clone(),
            )
            .unwrap()
            .identity_id;
        let root = [91; 32];
        let agent = Signer::from_secret(&root);
        let proof = human.authorize_agent_json(agent.id()).unwrap();
        drop(human);
        let owner_store = store::Store::open(path.join("owner.sqlite").to_str().unwrap()).unwrap();
        let profile = owner_store
            .identities()
            .unwrap()
            .into_iter()
            .find(|(p, _)| p.id == owner)
            .unwrap()
            .0;
        drop(owner_store);
        let vault = Arc::new(Vault::default());
        let db = path.join("agent.sqlite");
        let client = Client::open(db.to_str().unwrap().into(), "en".into()).unwrap();
        client
            .create_agent_account(
                "Source host".into(),
                "source_host".into(),
                relay.into(),
                root.to_vec(),
                owner.clone(),
                proof,
                vault.clone(),
            )
            .unwrap();
        drop(client);
        let key: [u8; 32] = owner_vault
            .load("zoen.identity.v1".into())
            .unwrap()
            .try_into()
            .unwrap();
        let signer = Signer::from_secret(&key);
        let store = store::Store::open(db.to_str().unwrap()).unwrap();
        store.put_identity(&profile, None).unwrap(); // public only, no owner key import
        let space = types::new_id("source_space")?;
        drop(store);
        // Admit the signed canonical prefix through the normal ingestion path.
        // Direct Store insertion omits the durable relay-ordered Space marker.
        let mut engine = client_core::Engine::open(db.to_str().unwrap()).unwrap();
        let mut admit = |event: &types::Event| -> Result<(), Box<dyn std::error::Error>> {
            assert_eq!(
                engine.ingest(protocol::Sequenced {
                    seq: event.seq,
                    prev: event.prev.clone(),
                    hash: event.hash.clone(),
                    env: protocol::Envelope::plain(event)?,
                }),
                client_core::Ingest::Applied
            );
            Ok(())
        };
        let mut log = SpaceLog::new(&space);
        admit(log.append(
            &agent,
            1_792_510_809_000,
            types::EventBody::SpaceCreated {
                title: "Source fixture".into(),
                kind: types::SpaceKind::Group,
                privacy: types::Privacy::Closed,
            },
        )?)?;
        admit(log.append(
            &agent,
            1_792_510_809_001,
            types::EventBody::MemberAdded {
                identity: owner.clone(),
                role: types::Role::Member,
            },
        )?)?;
        let mut post = |text: &str| -> std::result::Result<String, Box<dyn std::error::Error>> {
            let event = log
                .append(&signer, 1_792_510_809_002, message(text, None)?)?
                .clone();
            admit(&event)?;
            Ok(event.client_id)
        };
        let other = post("Other source must never be executed by this run.")?;
        let source = post("Inspect only the selected source.")?;
        log.verify().unwrap();
        drop(engine);
        let client = Client::open(db.to_str().unwrap().into(), "en".into()).unwrap();
        assert!(client.unlock(vault.clone()).unwrap());
        Ok(Self {
            path,
            client,
            vault,
            owner_vault,
            owner,
            space,
            source,
            other,
        })
    }
    pub(super) fn reopen(&mut self) {
        self.close();
        let client = Client::open(
            self.path.join("agent.sqlite").to_str().unwrap().into(),
            "en".into(),
        )
        .unwrap();
        assert!(client.unlock(self.vault.clone()).unwrap());
        self.client = client;
    }
    pub(super) fn history(
        &mut self,
    ) -> Result<Vec<protocol::Sequenced>, Box<dyn std::error::Error>> {
        self.close();
        let db = store::Store::open(self.path.join("agent.sqlite").to_str().unwrap())?;
        let history = db
            .events(&self.space)?
            .into_iter()
            .map(|event| {
                Ok(protocol::Sequenced {
                    seq: event.seq,
                    prev: event.prev.clone(),
                    hash: event.hash.clone(),
                    env: protocol::Envelope::plain(&event)?,
                })
            })
            .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
        drop(db);
        self.reopen();
        Ok(history)
    }
    /// Sequence and ingest this synthetic agent's actual queued publication.
    /// A registered Page cannot offer an unconfirmed optimistic import as a read base.
    pub(super) fn confirm_publications(
        &mut self,
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        self.close();
        let path = self.path.join("agent.sqlite");
        let db = store::Store::open(path.to_str().unwrap()).unwrap();
        let pending = db.outbox().unwrap();
        let mut log = SpaceLog::from_events(&self.space, db.events(&self.space).unwrap()).unwrap();
        drop(db);
        let mut engine = client_core::Engine::open(path.to_str().unwrap()).unwrap();
        for row in pending.into_iter().filter(|r| r.event.space == self.space) {
            let event = log.sequence(row.event)?.clone();
            assert_eq!(
                engine.ingest(protocol::Sequenced {
                    seq: event.seq,
                    prev: event.prev.clone(),
                    hash: event.hash.clone(),
                    env: protocol::Envelope::plain(&event)?,
                }),
                client_core::Ingest::Confirmed
            );
        }
        drop(engine);
        self.reopen();
        Ok(())
    }
    pub(super) fn append(
        &mut self,
        body: types::EventBody,
        owner: bool,
    ) -> std::result::Result<String, Box<dyn std::error::Error>> {
        self.close();
        flush(&self.path, "agent.sqlite", "agent.sqlite", &self.space)?;
        let db = store::Store::open(self.path.join("agent.sqlite").to_str().unwrap()).unwrap();
        let mut log = SpaceLog::from_events(&self.space, db.events(&self.space).unwrap()).unwrap();
        let key = if owner {
            self.owner_vault
                .load("zoen.identity.v1".into())
                .unwrap()
                .try_into()
                .unwrap()
        } else {
            [91; 32]
        };
        let event = log
            .append(&Signer::from_secret(&key), 1_792_510_810_000, body)?
            .clone();
        db.append_event(&event).unwrap();
        log.verify().unwrap();
        drop(db);
        self.reopen();
        Ok(event.client_id)
    }
    /// Actual signed expired approval + inert owner intent, with no sleeps and
    /// no live account access. The device/root are this synthetic fixture's keys.
    pub(super) fn expired_review(
        &mut self,
        original: &types::AgentRequest,
    ) -> std::result::Result<
        (types::AgentRequest, String, Vec<protocol::Sequenced>),
        Box<dyn std::error::Error>,
    > {
        self.close();
        flush(&self.path, "agent.sqlite", "agent.sqlite", &self.space)?;
        let db = store::Store::open(self.path.join("agent.sqlite").to_str().unwrap()).unwrap();
        let mut log = SpaceLog::from_events(&self.space, db.events(&self.space).unwrap()).unwrap();
        let device: [u8; 32] = self
            .vault
            .load("zoen.device.v1".into())
            .unwrap()
            .try_into()
            .unwrap();
        let agent = event_log::Author::device(
            &Signer::from_secret(&[91; 32]),
            Signer::from_secret(&device),
        );
        let owner: [u8; 32] = self
            .owner_vault
            .load("zoen.identity.v1".into())
            .unwrap()
            .try_into()
            .unwrap();
        let owner = event_log::Author::root(Signer::from_secret(&owner));
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let mut expired = original.clone();
        expired.id = types::new_id("expired_review")?;
        expired.tool_proposal.as_mut().unwrap().expires_at_ms = now - 1000;
        expired.content_hash = expired.tool_proposal_hash(&self.space).unwrap();
        for (author, at, body) in [
            (
                &agent,
                now - 2000,
                types::EventBody::RequestOpened {
                    request: expired.clone(),
                },
            ),
            (
                &owner,
                now - 1001,
                types::EventBody::RequestResolved {
                    request: expired.id.clone(),
                    approved: true,
                    content_hash: expired.content_hash.clone(),
                    resolution: Some(types::RequestResolution {
                        message: "Approved once".into(),
                        created_item: None,
                        ai_cost_cents: 0,
                        standing_grant: None,
                        tool_decision: Some(types::ToolDecision::ApproveOnce),
                    }),
                },
            ),
            (
                &owner,
                now,
                types::EventBody::ToolReviewRequested {
                    request: expired.id.clone(),
                    content_hash: expired.content_hash.clone(),
                },
            ),
        ] {
            let event =
                author.sign_event(&self.space, &types::new_ulid(at)?, at, log.head(), body)?;
            db.append_event(log.sequence(event)?).unwrap();
        }
        log.verify().unwrap();
        let intent = log.events().last().unwrap().client_id.clone();
        let history = log
            .events()
            .iter()
            .map(|event| {
                Ok(protocol::Sequenced {
                    seq: event.seq,
                    prev: event.prev.clone(),
                    hash: event.hash.clone(),
                    env: protocol::Envelope::plain(event)?,
                })
            })
            .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
        drop(db);
        self.reopen();
        Ok((expired, intent, history))
    }

    /// Supply genuine SDK-generated owner key packages to the owned loopback
    /// relay. This uses only the fixture's distinct synthetic owner database.
    pub(super) async fn publish_owner_packages(&self, server: &super::relay::Relay) {
        let owner = Client::open(
            self.path.join("owner.sqlite").to_str().unwrap().into(),
            "en".into(),
        )
        .unwrap();
        assert!(owner.unlock(self.owner_vault.clone()).unwrap());
        // Admit the synthetic owner through its normal initial Sync first.
        // Only the agent's subsequent SyncDone is held by the regression.
        server.released.store(true, Ordering::SeqCst);
        owner.start_sync(None).unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            while server
                .packages
                .lock()
                .unwrap()
                .get(&self.owner)
                .is_none_or(Vec::is_empty)
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        owner.wait_until_settled(3000).await.unwrap();
        owner.stop_sync();
        server.released.store(false, Ordering::SeqCst);
    }

    pub(super) async fn approve(
        &mut self,
        request: &client_core::AgentRequestDto,
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        self.close();
        flush(&self.path, "agent.sqlite", "owner.sqlite", &self.space)?;
        let owner = Client::open(
            self.path.join("owner.sqlite").to_str().unwrap().into(),
            "en".into(),
        )
        .unwrap();
        assert!(owner.unlock(self.owner_vault.clone()).unwrap());
        let command = json!({"op":"decide_tool_request","request_id":request.id,
            "observed_content_hash":request.tool_approval.as_ref().unwrap().content_hash,"decision":"ApproveOnce"});
        let value: serde_json::Value =
            serde_json::from_str(&owner.client_request(command.to_string()).await.unwrap())
                .unwrap();
        assert_eq!(value["request"]["status"], "Approved");
        assert_eq!(value["standing_grant_id"], serde_json::Value::Null);
        drop(owner);
        flush(&self.path, "owner.sqlite", "agent.sqlite", &self.space)?;
        self.reopen();
        Ok(())
    }
}

pub(super) fn message(
    text: &str,
    attaches: Option<String>,
) -> std::result::Result<types::EventBody, Box<dyn std::error::Error>> {
    Ok(types::EventBody::MessagePosted {
        message: types::new_id("message")?,
        text: text.into(),
        attaches,
        media: None,
        reply: None,
        forwarded: false,
    })
}

// Sequence only signed fixture outbox entries and copy public profiles/events.
fn flush(
    path: &Path,
    from: &str,
    to: &str,
    space: &str,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let source = store::Store::open(path.join(from).to_str().unwrap()).unwrap();
    let mut log = SpaceLog::from_events(space, source.events(space).unwrap()).unwrap();
    for pending in source
        .outbox()
        .unwrap()
        .into_iter()
        .filter(|p| p.event.space == space)
    {
        if !log
            .events()
            .iter()
            .any(|e| e.client_id == pending.event.client_id)
        {
            event_log::verify_author(&pending.event).unwrap();
            source
                .append_event(log.sequence(pending.event.clone())?)
                .unwrap();
        }
        source.outbox_remove(&pending.event.client_id).unwrap();
    }
    log.verify().unwrap();
    let events = source.events(space).unwrap();
    let public: Vec<_> = source
        .identities()
        .unwrap()
        .into_iter()
        .map(|(p, _)| p)
        .collect();
    drop(source);
    let target = store::Store::open(path.join(to).to_str().unwrap()).unwrap();
    for profile in public {
        target.put_identity(&profile, None).unwrap();
    }
    let existing = target.events(space).unwrap();
    assert!(existing.len() <= events.len());
    assert_eq!(
        serde_json::to_value(&existing).unwrap(),
        serde_json::to_value(&events[..existing.len()]).unwrap()
    );
    for event in &events[existing.len()..] {
        target.append_event(event).unwrap();
    }
    SpaceLog::from_events(space, target.events(space).unwrap())
        .unwrap()
        .verify()
        .unwrap();
    Ok(())
}
