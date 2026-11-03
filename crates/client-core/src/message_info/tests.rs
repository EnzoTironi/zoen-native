use super::*;
use crate::{engine::now_ms, Client, SecretVault};
use protocol::read_time::ReadTime;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use types::{EventBody, Identity, IdentityKind, Privacy, Role, SpaceKind};
#[derive(Default)]
struct Vault(Mutex<HashMap<String, Vec<u8>>>);
impl SecretVault for Vault {
    fn load(&self, key: String) -> Option<Vec<u8>> {
        self.0.lock().unwrap().get(&key).cloned()
    }
    fn save(&self, key: String, value: Vec<u8>) -> bool {
        self.0.lock().unwrap().insert(key, value);
        true
    }
    fn delete(&self, key: String) {
        self.0.lock().unwrap().remove(&key);
    }
}
struct Fixture {
    client: Arc<Client>,
    vault: Arc<Vault>,
    scope: MessageInfoScope,
    peer: String,
    outsider: String,
}
fn fixture(path: &str) -> Fixture {
    let client = Client::open(path.into(), "en".into()).unwrap();
    let vault = Arc::new(Vault::default());
    client
        .create_account(
            "Info author".into(),
            "info_author".into(),
            "http://127.0.0.1:1".into(),
            vault.clone(),
        )
        .unwrap();
    let account = client.account().unwrap();
    let me = account.identity_id.clone();
    let mut e = client.lock();
    let person = |name: &str| Identity {
        id: String::new(),
        kind: IdentityKind::Person,
        name: name.into(),
        handle: name.into(),
        tint_hex: "#123456".into(),
        glyph: None,
        owner: None,
        owner_proof: None,
        bio: String::new(),
    };
    let peer = e.create_identity(person("peer"), true).unwrap();
    let outsider = e.create_identity(person("outsider"), true).unwrap();
    let space = types::new_id("info").unwrap();
    e.append(
        &space,
        &me,
        EventBody::SpaceCreated {
            title: "Info".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::Closed,
        },
    )
    .unwrap();
    e.append(
        &space,
        &me,
        EventBody::MemberAdded {
            identity: peer.clone(),
            role: Role::Member,
        },
    )
    .unwrap();
    let message = e.post_as(&space, &me, "Original", None).unwrap();
    drop(e);
    Fixture {
        client,
        vault,
        scope: MessageInfoScope {
            expected_identity_id: me,
            expected_device_id: account.device_id,
            space_id: space,
            message_id: message.id,
        },
        peer,
        outsider,
    }
}
#[test]
fn legacy_read_is_known_without_a_time_and_later_interval_does_not_date_it() {
    let f = fixture(":memory:");
    let mut e = f.client.lock();
    let old = e.message_info(&f.scope).unwrap();
    let old_seq = e.timeline(&f.scope.space_id).unwrap()[0].seq;
    e.receipt(&f.scope.space_id, &f.peer, 0, old_seq).unwrap();
    let mut next = f.scope.clone();
    next.message_id = e
        .post_as(
            &f.scope.space_id,
            &f.scope.expected_identity_id,
            "Next",
            None,
        )
        .unwrap()
        .id;
    let next_seq = e.timeline(&f.scope.space_id).unwrap().last().unwrap().seq;
    let observed = now_ms();
    e.receive_read_time(
        &f.scope.space_id,
        &f.peer,
        &ReadTime {
            after_seq: old_seq,
            through_seq: next_seq,
            reported_at_ms: 1000,
        },
        observed,
    )
    .unwrap();
    let old = e.message_info(&old.scope).unwrap();
    assert!(old.participants[0].read);
    assert_eq!(old.participants[0].read_reported_at_ms, None);
    let new = e.message_info(&next).unwrap();
    assert_eq!(new.participants[0].read_reported_at_ms, Some(1000));
    assert_eq!(new.participants[0].read_observed_at_ms, Some(observed));
}
#[test]
fn accepted_edit_date_comes_from_the_exact_signed_edit_not_the_send_date_or_reaction() {
    let f = fixture(":memory:");
    let mut e = f.client.lock();
    let before = e.message_info(&f.scope).unwrap();
    let edited_at = before.sent_at_ms + 1234;
    e.append_at(
        &f.scope.space_id,
        &f.scope.expected_identity_id,
        edited_at,
        EventBody::MessageAnnotated {
            target: f.scope.message_id.clone(),
            note: types::Annotation::Edit {
                text: "Edited".into(),
            },
        },
    )
    .unwrap();
    e.append(
        &f.scope.space_id,
        &f.peer,
        EventBody::MessageAnnotated {
            target: f.scope.message_id.clone(),
            note: types::Annotation::React {
                emoji: "👍".into()
            },
        },
    )
    .unwrap();
    let after = e.message_info(&f.scope).unwrap();
    assert_eq!(after.sent_at_ms, before.sent_at_ms);
    assert_eq!(after.edited_at_ms, Some(edited_at));
}
#[test]
fn owner_membership_sender_and_small_group_privacy_are_authoritative() {
    let f = fixture(":memory:");
    let mut e = f.client.lock();
    let mut wrong = f.scope.clone();
    wrong.expected_device_id = "wrong".into();
    assert!(e.message_info(&wrong).is_err());
    assert!(!e
        .receive_read_time(
            &f.scope.space_id,
            &f.outsider,
            &ReadTime {
                after_seq: 0,
                through_seq: 99,
                reported_at_ms: 1
            },
            now_ms()
        )
        .unwrap());
    let incoming = e
        .post_as(&f.scope.space_id, &f.peer, "Incoming", None)
        .unwrap();
    let mut scope = f.scope.clone();
    scope.message_id = incoming.id;
    assert_eq!(
        e.message_info(&scope).unwrap().read_details,
        MessageReadDetails::NotSender
    );
    assert!(e.message_info(&scope).unwrap().participants.is_empty());
    e.append(
        &f.scope.space_id,
        &f.scope.expected_identity_id,
        EventBody::MemberRemoved {
            identity: f.peer.clone(),
        },
    )
    .unwrap();
    assert!(e.message_info(&f.scope).unwrap().participants.is_empty());
    e.append(
        &f.scope.space_id,
        &f.scope.expected_identity_id,
        EventBody::MemberRemoved {
            identity: f.scope.expected_identity_id.clone(),
        },
    )
    .unwrap();
    assert!(e.message_info(&f.scope).is_err());
}
#[test]
fn local_read_report_is_durable_idempotent_and_never_uses_reconnect_clock() {
    let path = std::env::temp_dir().join(types::new_id("info-cold").unwrap());
    let f = fixture(path.to_str().unwrap());
    let report = {
        let mut e = f.client.lock();
        let seq = e.mark_read(&f.scope.space_id).unwrap().unwrap();
        let frame = e.read_time_announcement(&f.scope.space_id, seq).unwrap();
        assert!(e.mark_read(&f.scope.space_id).unwrap().is_none());
        frame
    };
    let scope = f.scope.clone();
    let vault = f.vault.clone();
    drop(f.client);
    let client = Client::open(path.to_str().unwrap().into(), "en".into()).unwrap();
    assert!(client.unlock(vault).unwrap());
    let mut e = client.lock();
    e.net.synced.insert(scope.space_id.clone());
    let frames = e.receipts_with_times_to_replay().unwrap();
    assert!(frames
        .iter()
        .any(|(space, k)| space == &scope.space_id && k == &report));
    drop(e);
    drop(client);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn read_time_cache_is_not_backed_up_and_storage_failure_is_not_an_unknown_date() {
    let f = fixture(":memory:");
    let mut e = f.client.lock();
    let seq = e.timeline(&f.scope.space_id).unwrap()[0].seq;
    e.receive_read_time(
        &f.scope.space_id,
        &f.peer,
        &ReadTime {
            after_seq: 0,
            through_seq: seq,
            reported_at_ms: 1234,
        },
        now_ms(),
    )
    .unwrap();
    let bytes = e.backup_snapshot().unwrap();
    let path = std::env::temp_dir().join(types::new_id("info-backup").unwrap());
    std::fs::write(&path, bytes).unwrap();
    let backup = rusqlite::Connection::open(&path).unwrap();
    let tables: i64 = backup
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name='peer_read_times'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(tables, 0);
    drop(backup);
    std::fs::remove_file(path).unwrap();
    e.store.conn().execute_batch("ALTER TABLE peer_read_times RENAME TO corrupted_times; CREATE VIEW peer_read_times AS SELECT * FROM missing_receipt_source;").unwrap();
    assert!(e.message_info(&f.scope).is_err());
}
#[test]
fn optimistic_message_does_not_inherit_old_remote_receipts() {
    let f = fixture(":memory:");
    let mut e = f.client.lock();
    e.net.synced.insert(f.scope.space_id.clone());
    e.receipt(&f.scope.space_id, &f.peer, u64::MAX, u64::MAX)
        .unwrap();
    let pending = e
        .post_as(
            &f.scope.space_id,
            &f.scope.expected_identity_id,
            "Pending",
            None,
        )
        .unwrap();
    let mut scope = f.scope.clone();
    scope.message_id = pending.id;
    let info = e.message_info(&scope).unwrap();
    assert_eq!(info.read_details, MessageReadDetails::Unconfirmed);
    assert!(info.participants.is_empty());
}
#[test]
fn large_group_keeps_legacy_watermarks_without_retaining_or_disclosing_peer_dates() {
    let f = fixture(":memory:");
    let mut e = f.client.lock();
    for index in 0..15 {
        let name = format!("extra{index}");
        let identity = e
            .create_identity(
                Identity {
                    id: String::new(),
                    kind: IdentityKind::Person,
                    name: name.clone(),
                    handle: name,
                    tint_hex: "#123456".into(),
                    glyph: None,
                    owner: None,
                    owner_proof: None,
                    bio: String::new(),
                },
                true,
            )
            .unwrap();
        e.append(
            &f.scope.space_id,
            &f.scope.expected_identity_id,
            EventBody::MemberAdded {
                identity,
                role: Role::Member,
            },
        )
        .unwrap();
    }
    let seq = e.timeline(&f.scope.space_id).unwrap()[0].seq;
    assert!(!e.read_time_allowed(&f.scope.space_id).unwrap());
    e.receive_read_time(
        &f.scope.space_id,
        &f.peer,
        &ReadTime {
            after_seq: 0,
            through_seq: seq,
            reported_at_ms: 1000,
        },
        now_ms(),
    )
    .unwrap();
    let info = e.message_info(&f.scope).unwrap();
    assert_eq!(info.read_details, MessageReadDetails::NotSmallConversation);
    assert!(info.participants.is_empty());
    assert!(e
        .store
        .read_times(&f.scope.space_id, &f.peer, 0)
        .unwrap()
        .is_empty());
    assert_eq!(e.net.receipts[&f.scope.space_id][&f.peer].1, seq);
}
#[test]
fn deferred_peer_date_waits_for_membership_and_a_signed_removal_cuts_held_evidence() {
    let f = fixture(":memory:");
    let mut e = f.client.lock();
    let mut deferred = crate::net::DeferredReceipts::default();
    let report = ReadTime {
        after_seq: 0,
        through_seq: 99,
        reported_at_ms: 1000,
    };
    assert!(!deferred
        .receive_time(&mut e, &f.scope.space_id, &f.outsider, &report)
        .unwrap());
    assert!(e
        .store
        .read_times(&f.scope.space_id, &f.outsider, 0)
        .unwrap()
        .is_empty());
    e.append(
        &f.scope.space_id,
        &f.scope.expected_identity_id,
        EventBody::MemberAdded {
            identity: f.outsider.clone(),
            role: Role::Member,
        },
    )
    .unwrap();
    e.append(
        &f.scope.space_id,
        &f.scope.expected_identity_id,
        EventBody::MemberRemoved {
            identity: f.outsider.clone(),
        },
    )
    .unwrap();
    assert!(!deferred
        .receive_time(&mut e, &f.scope.space_id, &f.outsider, &report)
        .unwrap());
    e.append(
        &f.scope.space_id,
        &f.scope.expected_identity_id,
        EventBody::MemberAdded {
            identity: f.outsider.clone(),
            role: Role::Member,
        },
    )
    .unwrap();
    deferred
        .flush(&mut e, &mut std::collections::HashSet::new())
        .unwrap();
    assert!(e
        .store
        .read_times(&f.scope.space_id, &f.outsider, 0)
        .unwrap()
        .is_empty());
    assert!(
        !e.message_info(&f.scope)
            .unwrap()
            .participants
            .iter()
            .find(|p| p.person.id == f.outsider)
            .unwrap()
            .read
    );
}
