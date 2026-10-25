use super::*;
use crate::SecretVault;
use event_log::Signer;
use serde_json::json;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use types::{Annotation, EventBody, Privacy, Role, SpaceKind};
pub(super) const ORIGINAL: &[u8] =
    include_bytes!("../app_binary_documents/fixtures/original.deckcraft");
pub(super) const CHANGED: &[u8] =
    include_bytes!("../app_binary_documents/fixtures/changed.deckcraft");
#[derive(Default)]
pub(crate) struct Vault(Mutex<HashMap<String, Vec<u8>>>);
impl SecretVault for Vault {
    fn load(&self, key: String) -> Option<Vec<u8>> {
        self.0.lock().unwrap().get(&key).cloned()
    }
    fn save(&self, key: String, bytes: Vec<u8>) -> bool {
        self.0.lock().unwrap().insert(key, bytes);
        true
    }
    fn delete(&self, key: String) {
        self.0.lock().unwrap().remove(&key);
    }
}
pub(crate) struct Fixture {
    pub(crate) owner_client: Arc<Client>,
    pub(crate) agent: Arc<Client>,
    pub(crate) agent_vault: Arc<Vault>,
    pub(crate) owner: String,
    pub(crate) space: String,
    pub(crate) source: String,
    pub(crate) base: AppBinaryDocumentDto,
}
pub(crate) fn fixture() -> std::result::Result<Fixture, Box<dyn std::error::Error>> {
    fixture_with_privacy(Privacy::Closed)
}
pub(crate) fn fixture_with_privacy(
    privacy: Privacy,
) -> std::result::Result<Fixture, Box<dyn std::error::Error>> {
    fixture_binary(privacy, "deckcraft", ORIGINAL)
}
pub(crate) fn grid_fixture() -> std::result::Result<Fixture, Box<dyn std::error::Error>> {
    fixture_binary(
        Privacy::Closed,
        "gridcraft",
        include_bytes!("../../../miniapps/resources/gridcraft-empty.xlsx"),
    )
}
pub(crate) fn fixture_binary(
    privacy: Privacy,
    app: &str,
    bytes: &[u8],
) -> std::result::Result<Fixture, Box<dyn std::error::Error>> {
    let owner = Client::open(":memory:".into(), "en".into()).unwrap();
    let vault = Arc::new(Vault::default());
    let owner_id = owner
        .create_account(
            "Owner".into(),
            "worker_owner".into(),
            "http://127.0.0.1:1".into(),
            vault.clone(),
        )
        .unwrap()
        .identity_id;
    let root = [71; 32];
    let agent_id = Signer::from_secret(&root).id();
    let proof = owner.authorize_agent_json(agent_id).unwrap();
    let agent = Client::open(":memory:".into(), "en".into()).unwrap();
    let agent_vault = Arc::new(Vault::default());
    agent
        .create_agent_account(
            "Worker".into(),
            "worker_agent".into(),
            "http://127.0.0.1:1".into(),
            root.to_vec(),
            owner_id.clone(),
            proof,
            agent_vault.clone(),
        )
        .unwrap();
    // Test fixture only: retain the synthetic human's root so its real signed
    // inbound messages/role changes can be authored offline, without a relay.
    {
        let profile = owner.lock().identities[&owner_id].clone();
        let key: [u8; 32] = vault
            .load("zoen.identity.v1".into())
            .unwrap()
            .try_into()
            .unwrap();
        let mut e = agent.lock();
        e.store.put_identity(&profile, Some(&key)).unwrap();
        e.identities.insert(owner_id.clone(), profile);
        e.identity_order.push(owner_id.clone());
        e.signers
            .insert(owner_id.clone(), Signer::from_secret(&key));
    }
    // A signed locally ordered fixture avoids impersonating a remote identity
    // through the real account's publish-only device/outbox admission guard.
    let space = types::new_id("worker_space")?;
    {
        let mut e = agent.lock();
        let me = e.me_id().unwrap();
        e.append(
            &space,
            &me,
            EventBody::SpaceCreated {
                title: "Worker contract".into(),
                kind: SpaceKind::Group,
                privacy,
            },
        )
        .unwrap();
        e.append(
            &space,
            &me,
            EventBody::MemberAdded {
                identity: owner_id.clone(),
                role: Role::Member,
            },
        )
        .unwrap();
    }
    let base = agent
        .app_binary_create_for(
            app.into(),
            space.clone(),
            if app == "deckcraft" {
                "Deck".into()
            } else {
                "Grid".into()
            },
            STANDARD.encode(bytes),
            "create_worker".into(),
        )
        .unwrap();
    let source = agent
        .demo_member_say(
            space.clone(),
            "worker_owner".into(),
            "Update the reviewed presentation".into(),
        )
        .unwrap()
        .id;
    Ok(Fixture {
        owner_client: owner,
        agent,
        agent_vault,
        owner: owner_id,
        space,
        source,
        base,
    })
}
pub(super) fn turn(f: &Fixture) -> AgentBinaryTurn {
    f.agent
        .agent_binary_turn(
            &f.base.item_id,
            &f.source,
            &f.owner,
            Some(f.base.version),
            Some(&f.base.sha256),
        )
        .unwrap()
}
pub(super) fn counts(f: &Fixture) -> Vec<i64> {
    let e = f.agent.lock();
    [
        "events",
        "media",
        "media_keys",
        "blob_uploads",
        "outbox",
        "meta",
    ]
    .iter()
    .map(|table| {
        e.store
            .conn()
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    })
    .collect()
}
#[test]
fn signed_source_and_actual_certified_agent_are_required_before_worker_admission() {
    let f = fixture().unwrap();
    let before = counts(&f);
    let t = turn(&f);
    assert_eq!(t.document().actor_kind, "agent");
    assert_eq!(
        t.document().actor_id,
        f.agent.account().unwrap().identity_id
    );
    assert_eq!(t.document().data_base64, STANDARD.encode(ORIGINAL));
    assert!(f
        .agent
        .agent_binary_turn(&f.base.item_id, "missing", &f.owner, None, None)
        .is_err());
    assert!(f
        .agent
        .agent_binary_turn(
            &f.base.item_id,
            &f.source,
            &t.document().actor_id,
            None,
            None
        )
        .is_err());
    let own_message = f
        .agent
        .send_message(
            f.space.clone(),
            "A model cannot certify its own trigger".into(),
        )
        .unwrap();
    assert!(f
        .agent
        .agent_binary_turn(
            &f.base.item_id,
            &own_message.id,
            &t.document().actor_id,
            None,
            None
        )
        .is_err());
    // The only additional writes were the deliberately authored message.
    let after_message = counts(&f);
    assert!(after_message[0] > before[0]);
    assert_eq!(after_message[1..4], before[1..4]);
    assert_eq!(f.agent.item(f.base.item_id.clone()).unwrap().version, 1);
}
#[test]
fn source_changes_and_device_retirement_fence_compute_results_without_partial_writes() {
    let f = fixture().unwrap();
    let t = turn(&f);
    // A reaction is outside command source content and does not revoke the turn.
    {
        let mut e = f.agent.lock();
        e.append(
            &f.space,
            &f.owner,
            EventBody::MessageAnnotated {
                target: f.source.clone(),
                note: Annotation::React {
                    emoji: "👍".into()
                },
            },
        )
        .unwrap();
    }
    f.agent.agent_binary_finish_read(&t).unwrap();
    assert!(f
        .agent
        .agent_binary_result(&t, "worker_edit", &hex::encode([1; 32]))
        .unwrap()
        .is_none());
    {
        let mut e = f.agent.lock();
        e.append(
            &f.space,
            &f.owner,
            EventBody::MessageAnnotated {
                target: f.source.clone(),
                note: Annotation::Edit {
                    text: "Cancel the prior instruction".into(),
                },
            },
        )
        .unwrap();
    }
    let before = counts(&f);
    assert!(f.agent.agent_binary_finish_read(&t).is_err());
    assert!(f
        .agent
        .agent_binary_result(&t, "worker_edit", &hex::encode([1; 32]))
        .is_err());
    assert!(f
        .agent
        .agent_binary_save(
            &t,
            "worker_edit",
            &hex::encode([1; 32]),
            &STANDARD.encode(CHANGED)
        )
        .is_err());
    assert_eq!(counts(&f), before);
    let fresh = turn(&f);
    let device = f.agent.account().unwrap().device_id;
    f.agent
        .lock()
        .store
        .set_meta("devices.revoked", &json!([device]).to_string())
        .unwrap();
    let before = counts(&f);
    assert!(f.agent.agent_binary_finish_read(&fresh).is_err());
    assert!(f
        .agent
        .agent_binary_result(&fresh, "worker_edit", &hex::encode([1; 32]))
        .is_err());
    assert!(f
        .agent
        .agent_binary_save(
            &fresh,
            "worker_edit",
            &hex::encode([1; 32]),
            &STANDARD.encode(CHANGED)
        )
        .is_err());
    assert_eq!(counts(&f), before);
}
#[test]
fn serving_writer_and_exact_native_receipts_remain_authoritative_after_compute() {
    let f = fixture().unwrap();
    let t = turn(&f);
    let intent = hex::encode([2; 32]);
    assert!(f
        .agent
        .agent_binary_result(&t, "worker_edit", &intent)
        .unwrap()
        .is_none());
    let accepted = f
        .agent
        .agent_binary_save(&t, "worker_edit", &intent, &STANDARD.encode(CHANGED))
        .unwrap();
    assert_eq!(accepted.version, 2);
    assert_eq!(accepted.author_id, t.document().actor_id);
    let before = counts(&f);
    assert_eq!(
        f.agent
            .agent_binary_result(&t, "worker_edit", &intent)
            .unwrap()
            .unwrap(),
        accepted
    );
    assert_eq!(
        f.agent
            .agent_binary_save(&t, "worker_edit", &intent, &STANDARD.encode(CHANGED))
            .unwrap(),
        accepted
    );
    assert_eq!(counts(&f), before);
    {
        let mut e = f.agent.lock();
        let agent = e.me_id().unwrap();
        e.append(
            &f.space,
            &agent,
            EventBody::MemberAdded {
                identity: f.owner.clone(),
                role: Role::Reader,
            },
        )
        .unwrap();
    }
    let before = counts(&f);
    // Even an existing receipt cannot bypass the current human's writer status.
    assert!(f
        .agent
        .agent_binary_result(&t, "worker_edit", &intent)
        .is_err());
    assert!(f
        .agent
        .agent_binary_save(&t, "worker_edit", &intent, &STANDARD.encode(CHANGED))
        .is_err());
    f.agent.agent_binary_finish_read(&t).unwrap();
    assert_eq!(counts(&f), before);
    assert!(f
        .agent
        .agent_binary_save(
            &t,
            "too_large",
            &intent,
            &STANDARD.encode(vec![0; AGENT_BINARY_DOCUMENT_LIMIT + 1])
        )
        .is_err());
    assert_eq!(counts(&f), before);
    assert_eq!(f.agent.item(f.base.item_id.clone()).unwrap().version, 2);
}

pub(crate) mod renewal_history;
