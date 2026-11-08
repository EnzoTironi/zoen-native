use super::{
    Annotation, EntryBody, EventBody, MessagePinChange, MessagePinEdit, MessagePinPage,
    MessagePinScope,
};
use crate::{Client, SecretVault};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use types::{Identity, IdentityKind, Privacy, Role, SpaceKind};
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
#[derive(Default)]
struct Vault(Mutex<HashMap<String, Vec<u8>>>);
impl SecretVault for Vault {
    fn load(&self, key: String) -> Option<Vec<u8>> {
        self.0.lock().ok()?.get(&key).cloned()
    }
    fn save(&self, key: String, bytes: Vec<u8>) -> bool {
        if let Ok(mut map) = self.0.lock() {
            map.insert(key, bytes);
            true
        } else {
            false
        }
    }
    fn delete(&self, key: String) {
        if let Ok(mut map) = self.0.lock() {
            map.remove(&key);
        }
    }
}
struct Fixture {
    client: Arc<Client>,
    vault: Arc<Vault>,
    scope: MessagePinScope,
    peer: String,
}
impl Fixture {
    fn new(path: &str, kind: SpaceKind) -> TestResult<Self> {
        let client = Client::open(path.into(), "en".into())?;
        let vault = Arc::new(Vault::default());
        client.create_account(
            "Pin owner".into(),
            "pin_owner".into(),
            "http://127.0.0.1:1".into(),
            vault.clone(),
        )?;
        let acct = client.account().ok_or("account")?;
        let mut e = client.lock();
        e.net.account.as_mut().ok_or("native account")?.registered = true;
        e.store.set_meta(
            "account",
            &serde_json::to_string(e.net.account.as_ref().ok_or("registered fixture account")?)?,
        )?;
        let peer = e.create_identity(
            Identity {
                id: String::new(),
                kind: IdentityKind::Person,
                name: "Peer".into(),
                handle: "peer".into(),
                tint_hex: "#123456".into(),
                glyph: None,
                owner: None,
                owner_proof: None,
                bio: String::new(),
            },
            true,
        )?;
        let space = types::new_id("pins")?;
        e.append(
            &space,
            &peer,
            EventBody::SpaceCreated {
                title: "Shared pins".into(),
                kind,
                privacy: Privacy::Closed,
            },
        )?;
        e.append(
            &space,
            &peer,
            EventBody::MemberAdded {
                identity: acct.identity_id.clone(),
                role: Role::Admin,
            },
        )?;
        drop(e);
        let scope = MessagePinScope {
            expected_identity_id: acct.identity_id,
            expected_device_id: acct.device_id,
            expected_relay_url: acct.relay_url,
            space_id: space,
        };
        Ok(Self {
            client,
            vault,
            scope,
            peer,
        })
    }
    fn send(&self, text: &str) -> TestResult<String> {
        let event = self.client.lock().append(
            &self.scope.space_id,
            &self.peer,
            EventBody::MessagePosted {
                message: types::new_id("message")?,
                text: text.into(),
                attaches: None,
                media: None,
                reply: None,
                forwarded: false,
                forwarded_author: None,
            },
        )?;
        Ok(event.client_id)
    }
    fn edit(&self, message: &str, pinned: bool, operation: &str) -> TestResult<MessagePinEdit> {
        let review = self
            .client
            .lock()
            .message_pin_review(&self.scope, message)?;
        Ok(MessagePinEdit {
            scope: self.scope.clone(),
            message_id: message.into(),
            change: MessagePinChange {
                operation_id: operation.into(),
                expected_message_revision: review.entry.text_revision.ok_or("message revision")?,
                expected_pin_revision: review.pin_revision,
                pinned,
            },
        })
    }
    fn page(&self, cursor: Option<&str>, limit: u32) -> TestResult<MessagePinPage> {
        Ok(self
            .client
            .lock()
            .message_pins(&self.scope, cursor, limit)?)
    }
}
#[test]
fn shared_message_pins_review_idempotency_edits_unpin_and_cold_replay() -> TestResult {
    let home = std::env::temp_dir().join(types::new_id("zoen-message-pins-test")?);
    std::fs::create_dir(&home)?;
    let path = home.join("pins.sqlite").to_string_lossy().into_owned();
    let mut f = Fixture::new(&path, SpaceKind::Group)?;
    let message = f.send("Original pinned text")?;
    let pin = f.edit(&message, true, "pin-one")?;
    let first = f.client.lock().set_message_pin(&pin)?;
    let before = f
        .client
        .lock()
        .logs
        .get(&f.scope.space_id)
        .ok_or("log")?
        .events()
        .len();
    let again = f.client.lock().set_message_pin(&pin)?;
    assert_eq!(first.applied_revision, again.applied_revision);
    assert_eq!(
        f.client
            .lock()
            .logs
            .get(&f.scope.space_id)
            .ok_or("log")?
            .events()
            .len(),
        before
    );
    let stale_unpin = f.edit(&message, false, "stale-unpin")?;
    f.client.lock().append(
        &f.scope.space_id,
        &f.peer,
        EventBody::MessageAnnotated {
            target: message.clone(),
            note: Annotation::Edit {
                text: "Edited pinned text".into(),
            },
        },
    )?;
    assert!(f.client.lock().set_message_pin(&stale_unpin).is_err());
    assert!(
        matches!(&f.page(None,50)?.entries[0].kind,crate::EntryKind::Message {text,..} if text=="Edited pinned text")
    );
    assert_eq!(
        f.client.lock().set_message_pin(&pin)?.applied_revision,
        first.applied_revision
    );
    let Fixture {
        client: previous,
        vault,
        scope,
        peer,
    } = f;
    previous.stop_sync();
    drop(previous);
    let reopened = Client::open(path, "en".into())?;
    assert!(reopened.unlock(vault.clone())?);
    reopened
        .lock()
        .net
        .account
        .as_mut()
        .ok_or("reopened account")?
        .registered = true;
    f = Fixture {
        client: reopened,
        vault,
        scope,
        peer,
    };
    assert_eq!(f.page(None, 50)?.entries[0].id, message);
    assert_eq!(
        f.client.lock().set_message_pin(&pin)?.applied_revision,
        first.applied_revision
    );
    let unpin = f.edit(&message, false, "unpin-one")?;
    f.client.lock().set_message_pin(&unpin)?;
    assert_eq!(f.page(None, 50)?.entries, Vec::new());
    assert!(!f.client.lock().set_message_pin(&pin)?.review.pinned);
    drop(f);
    std::fs::remove_dir_all(home)?;
    Ok(())
}
#[test]
fn roles_native_scope_and_malformed_or_changed_pins_refuse_without_writes() -> TestResult {
    let f = Fixture::new(":memory:", SpaceKind::Community)?;
    let message = f.send("Shared target")?;
    let pin = f.edit(&message, true, "role-pin")?;
    for field in ["identity", "device", "relay", "space"] {
        let mut bad = pin.clone();
        match field {
            "identity" => bad.scope.expected_identity_id = "foreign".into(),
            "device" => bad.scope.expected_device_id = "foreign-device".into(),
            "relay" => bad.scope.expected_relay_url = "https://other.invalid".into(),
            _ => bad.scope.space_id = "other-space".into(),
        }
        assert!(f.client.lock().set_message_pin(&bad).is_err());
    }
    f.client.lock().append(
        &f.scope.space_id,
        &f.peer,
        EventBody::MemberAdded {
            identity: f.scope.expected_identity_id.clone(),
            role: Role::Member,
        },
    )?;
    assert!(
        !f.client
            .lock()
            .message_pin_review(&f.scope, &message)?
            .can_pin
    );
    assert!(f.client.lock().set_message_pin(&pin).is_err());
    let forged = EventBody::MessageAnnotated {
        target: message.clone(),
        note: Annotation::Pin {
            change: pin.change.clone(),
        },
    };
    let count = f
        .client
        .lock()
        .logs
        .get(&f.scope.space_id)
        .ok_or("log")?
        .events()
        .len();
    assert!(f
        .client
        .lock()
        .append(&f.scope.space_id, &f.scope.expected_identity_id, forged)
        .is_err());
    assert_eq!(
        f.client
            .lock()
            .logs
            .get(&f.scope.space_id)
            .ok_or("log")?
            .events()
            .len(),
        count
    );
    f.client.lock().append(
        &f.scope.space_id,
        &f.peer,
        EventBody::MemberAdded {
            identity: f.scope.expected_identity_id.clone(),
            role: Role::Admin,
        },
    )?;
    let mut bad = pin.clone();
    bad.change.operation_id = "bad id".into();
    assert!(f.client.lock().set_message_pin(&bad).is_err());
    f.client.lock().set_message_pin(&pin)?;
    let mut collision = pin.clone();
    collision.change.pinned = false;
    assert!(f.client.lock().set_message_pin(&collision).is_err());
    f.client.lock().store.set_meta(
        "devices.revoked",
        &serde_json::to_string(&vec![f.scope.expected_device_id.clone()])?,
    )?;
    assert!(f.page(None, 50).is_err());
    Ok(())
}
#[test]
fn paged_shared_pins_hide_deleted_content_and_retire_changed_cursors() -> TestResult {
    let f = Fixture::new(":memory:", SpaceKind::Group)?;
    for index in 0..6 {
        let message = f.send(&format!("Pinned {index}"))?;
        let pin = f.edit(&message, true, &format!("pin-{index}"))?;
        f.client.lock().set_message_pin(&pin)?;
    }
    let first = f.page(None, 2)?;
    assert_eq!(first.total, 6);
    assert_eq!(first.entries.len(), 2);
    let second = f.page(first.next.as_deref(), 2)?;
    assert_eq!(second.entries.len(), 2);
    assert!(first
        .entries
        .iter()
        .all(|a| second.entries.iter().all(|b| a.id != b.id)));
    let target = first.entries[0].id.clone();
    f.client.lock().append(
        &f.scope.space_id,
        &f.peer,
        EventBody::MessageAnnotated {
            target: target.clone(),
            note: Annotation::Delete,
        },
    )?;
    assert!(f.page(first.next.as_deref(), 2).is_err());
    assert_eq!(f.page(None, 100)?.total, 5);
    assert!(!f.page(None, 100)?.entries.iter().any(|e| e.id == target));
    assert!(f.page(Some("invalid cursor"), 50).is_err());
    Ok(())
}

#[test]
fn failed_pin_persistence_rolls_back_projection_and_exact_retry_can_commit() -> TestResult {
    let f = Fixture::new(":memory:", SpaceKind::Direct)?;
    let message = f.send("Atomic pin")?;
    let pin = f.edit(&message, true, "atomic-pin")?;
    let before = f
        .client
        .lock()
        .logs
        .get(&f.scope.space_id)
        .ok_or("log")?
        .events()
        .len();
    f.client.lock().store.conn().execute_batch("CREATE TRIGGER reject_pin BEFORE INSERT ON events WHEN NEW.json LIKE '%\"Pin\"%' BEGIN SELECT RAISE(ABORT,'synthetic pin failure'); END;")?;
    assert!(f.client.lock().set_message_pin(&pin).is_err());
    assert!(
        !f.client
            .lock()
            .message_pin_review(&f.scope, &message)?
            .pinned
    );
    assert_eq!(
        f.client
            .lock()
            .logs
            .get(&f.scope.space_id)
            .ok_or("log")?
            .events()
            .len(),
        before
    );
    f.client
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER reject_pin")?;
    f.client.lock().set_message_pin(&pin)?;
    assert!(
        f.client
            .lock()
            .message_pin_review(&f.scope, &message)?
            .pinned
    );
    Ok(())
}

#[test]
fn private_hiding_removes_pin_views_and_actions_without_clearing_the_shared_pin() -> TestResult {
    let f = Fixture::new(":memory:", SpaceKind::Group)?;
    let message = f.send("Hide only on my device")?;
    let pin = f.edit(&message, true, "private-pin")?;
    f.client.lock().set_message_pin(&pin)?;
    f.client.delete_messages(crate::MessageDeletion {
        expected_identity_id: f.scope.expected_identity_id.clone(),
        expected_device_id: f.scope.expected_device_id.clone(),
        space_id: f.scope.space_id.clone(),
        targets: vec![crate::MessageDeletionTarget {
            message_id: message.clone(),
            expected_revision: pin.change.expected_message_revision.clone(),
        }],
        for_me: Some(true),
    })?;
    assert_eq!(f.page(None, 50)?.total, 0);
    assert!(f
        .client
        .lock()
        .message_pin_review(&f.scope, &message)
        .is_err());
    assert!(f.client.lock().set_message_pin(&pin).is_err());
    let engine = f.client.lock();
    let source = engine
        .space_state(&f.scope.space_id)?
        .entries
        .iter()
        .find(|e| e.id() == message)
        .ok_or("hidden source")?;
    let EntryBody::Message { marks, .. } = &source.body else {
        return Err("message source".into());
    };
    assert!(
        marks.pin.as_ref().is_some_and(|p| p.pinned),
        "the signed shared pin remains unchanged"
    );
    let redacted = engine.entry_dto(source);
    assert!(!redacted.pinned && !redacted.can_pin);
    Ok(())
}
