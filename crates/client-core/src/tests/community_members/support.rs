use super::*;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Default)]
struct Vault(Mutex<HashMap<String, Vec<u8>>>);
impl SecretVault for Vault {
    fn load(&self, key: String) -> Option<Vec<u8>> {
        self.0.lock().ok()?.get(&key).cloned()
    }
    fn save(&self, key: String, value: Vec<u8>) -> bool {
        if let Ok(mut values) = self.0.lock() {
            values.insert(key, value);
            true
        } else {
            false
        }
    }
    fn delete(&self, key: String) {
        if let Ok(mut values) = self.0.lock() {
            values.remove(&key);
        }
    }
}
fn person(name: &str) -> types::Identity {
    types::Identity {
        id: String::new(),
        kind: types::IdentityKind::Person,
        name: name.into(),
        handle: name.to_lowercase(),
        tint_hex: "#246844".into(),
        glyph: None,
        owner: None,
        owner_proof: None,
        bio: String::new(),
    }
}
pub(super) struct Fixture {
    pub client: Arc<Client>,
    pub space: String,
    pub founder: String,
    pub member: String,
    pub identity: String,
    pub device: String,
    path: String,
    vault: Arc<Vault>,
}
impl Fixture {
    pub fn new(kind: types::SpaceKind) -> TestResult<Self> {
        Self::at(":memory:".into(), kind)
    }
    pub fn at(path: String, kind: types::SpaceKind) -> TestResult<Self> {
        let client = Client::open(path.clone(), "en".into())?;
        let vault = Arc::new(Vault::default());
        client.create_account(
            "Native admin".into(),
            "native_admin".into(),
            "http://127.0.0.1:1".into(),
            vault.clone(),
        )?;
        let account = client.account().ok_or("missing actual account")?;
        let (space, founder, member) = {
            let mut engine = client.lock();
            let founder = engine.create_identity(person("Founder"), true)?;
            let member = engine.create_identity(person("Selected"), true)?;
            let space = types::new_id("sp");
            engine.append(
                &space,
                &founder,
                types::EventBody::SpaceCreated {
                    title: "Actual membership fixture".into(),
                    kind,
                    privacy: types::Privacy::Closed,
                },
            )?;
            for (identity, role) in [
                (&account.identity_id, types::Role::Admin),
                (&member, types::Role::Member),
            ] {
                engine.append(
                    &space,
                    &founder,
                    types::EventBody::MemberAdded {
                        identity: identity.clone(),
                        role,
                    },
                )?;
            }
            engine.store.set_synced(&space)?;
            engine.net.synced.insert(space.clone());
            (space, founder, member)
        };
        Ok(Self {
            client,
            space,
            founder,
            member,
            identity: account.identity_id,
            device: account.device_id,
            path,
            vault,
        })
    }
    pub fn context(&self) -> std::result::Result<SpaceMemberContext, CoreError> {
        self.client.lock().space_member_context(
            &self.space,
            &self.member,
            &self.identity,
            &self.device,
        )
    }
    pub fn remove(&self, revision: &str) -> std::result::Result<SpaceMemberContext, CoreError> {
        self.client.lock().remove_member_reviewed(
            &self.space,
            &self.member,
            &self.identity,
            &self.device,
            revision,
        )
    }
    pub fn cold_reopen(self) -> TestResult<Self> {
        let Self {
            client,
            path,
            vault,
            space,
            founder,
            member,
            identity,
            device,
        } = self;
        drop(client);
        let client = Client::open(path.clone(), "en".into())?;
        assert!(client.unlock(vault.clone())?);
        Ok(Self {
            client,
            path,
            vault,
            space,
            founder,
            member,
            identity,
            device,
        })
    }
    pub fn counts(&self) -> TestResult<(u64, usize)> {
        let e = self.client.lock();
        Ok((e.store.event_count()?, e.store.outbox()?.len()))
    }
    /// Actual signed canonical membership from the continuing founder, not a forged projection.
    pub fn ordered(&self, body: types::EventBody) -> TestResult<()> {
        let mut engine = self.client.lock();
        let log = engine
            .logs
            .get(&self.space)
            .ok_or("missing confirmed log")?;
        let signed = engine.author_for(&self.founder)?.sign_event(
            &self.space,
            &types::new_id("membership"),
            crate::engine::now_ms(),
            log.head(),
            body,
        );
        let mut relay = log.clone();
        let event = relay.sequence(signed).clone();
        assert_eq!(
            engine.ingest(protocol::Sequenced {
                seq: event.seq,
                prev: event.prev.clone(),
                hash: event.hash.clone(),
                env: protocol::Envelope::plain(&event)
            }),
            crate::sync::Ingest::Applied
        );
        Ok(())
    }
    pub fn pending_removal(&self) -> TestResult<types::Event> {
        self.client.lock().store.outbox()?.into_iter().find(|row|
            row.event.space == self.space && !row.failed && matches!(&row.event.body, types::EventBody::MemberRemoved { identity } if identity == &self.member)
        ).map(|row| row.event).ok_or_else(|| "missing actual removal intent".into())
    }
    pub fn confirm(&self, pending: types::Event) -> TestResult<()> {
        let mut engine = self.client.lock();
        let mut relay = engine
            .logs
            .get(&self.space)
            .ok_or("missing confirmed log")?
            .clone();
        let event = relay.sequence(pending).clone();
        assert_eq!(
            engine.ingest(protocol::Sequenced {
                seq: event.seq,
                prev: event.prev.clone(),
                hash: event.hash.clone(),
                env: protocol::Envelope::plain(&event)
            }),
            crate::sync::Ingest::Confirmed
        );
        Ok(())
    }
}
