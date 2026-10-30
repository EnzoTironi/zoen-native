use super::*;
mod order;
mod private;
use crate::{PrivacyDto, SecretVault, TimelineEntry};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

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
struct Fixture {
    client: Arc<Client>,
    vault: Arc<Vault>,
    input: MessageDeletion,
    founder: String,
    unrelated: TimelineEntry,
}
fn target(entry: &TimelineEntry) -> TestResult<MessageDeletionTarget> {
    Ok(MessageDeletionTarget {
        message_id: entry.id.clone(),
        expected_revision: entry.text_revision.clone().ok_or("missing revision")?,
    })
}
fn fixture(path: &str) -> TestResult<Fixture> {
    let client = Client::open(path.into(), "en".into())?;
    let vault = Arc::new(Vault::default());
    client.create_account(
        "Author".into(),
        "selected_author".into(),
        "http://127.0.0.1:1".into(),
        vault.clone(),
    )?;
    let account = client.account().ok_or("account")?;
    let space = types::new_id("sp")?;
    let founder = {
        let mut engine = client.lock();
        let founder = engine.create_identity(
            types::Identity {
                id: String::new(),
                kind: types::IdentityKind::Person,
                name: "Founder".into(),
                handle: "founder".into(),
                tint_hex: "#246844".into(),
                glyph: None,
                owner: None,
                owner_proof: None,
                bio: String::new(),
            },
            true,
        )?;
        engine.append(
            &space,
            &founder,
            types::EventBody::SpaceCreated {
                title: "Selected messages".into(),
                kind: types::SpaceKind::Group,
                privacy: types::Privacy::Closed,
            },
        )?;
        engine.append(
            &space,
            &founder,
            types::EventBody::MemberAdded {
                identity: account.identity_id.clone(),
                role: types::Role::Member,
            },
        )?;
        engine.store.set_synced(&space)?;
        engine.net.synced.insert(space.clone());
        founder
    };
    let first = client.send_message(space.clone(), "First 😀".into())?;
    let second = client.send_reply(space.clone(), "Thread reply".into(), first.id.clone(), true)?;
    let unrelated = client.send_message(space.clone(), "Keep this".into())?;
    Ok(Fixture {
        client,
        vault,
        founder,
        unrelated,
        input: MessageDeletion {
            for_me: None,
            expected_identity_id: account.identity_id,
            expected_device_id: account.device_id,
            space_id: space,
            targets: vec![target(&first)?, target(&second)?],
        },
    })
}
fn counts(client: &Client) -> TestResult<(u64, usize)> {
    let engine = client.lock();
    Ok((engine.store.event_count()?, engine.store.outbox()?.len()))
}
fn timeline(f: &Fixture) -> TestResult<Vec<TimelineEntry>> {
    Ok(f.client.timeline(f.input.space_id.clone())?)
}

fn ingest(
    engine: &mut crate::engine::Engine,
    event: types::Event,
    expected: crate::sync::Ingest,
) -> TestResult {
    let mut relay = engine
        .logs
        .get(&event.space)
        .ok_or("confirmed log")?
        .clone();
    let event = relay.sequence(event)?.clone();
    assert_eq!(
        engine.ingest(protocol::Sequenced {
            seq: event.seq,
            prev: event.prev.clone(),
            hash: event.hash.clone(),
            env: protocol::Envelope::plain(&event)?
        }),
        expected
    );
    Ok(())
}
fn ordered(f: &Fixture, body: types::EventBody) -> TestResult<String> {
    let mut engine = f.client.lock();
    let id = types::new_id("remote")?;
    let event = engine.author_for(&f.founder)?.sign_event(
        &f.input.space_id,
        &id,
        crate::engine::now_ms(),
        engine
            .logs
            .get(&f.input.space_id)
            .ok_or("confirmed log")?
            .head(),
        body,
    )?;
    ingest(&mut engine, event, crate::sync::Ingest::Applied)?;
    Ok(id)
}

#[test]
fn exact_set_deletes_once_preserving_other_messages_and_drafts() -> TestResult {
    let f = fixture(":memory:")?;
    let scope = crate::message_drafts::DraftScope {
        expected_identity_id: f.input.expected_identity_id.clone(),
        expected_device_id: f.input.expected_device_id.clone(),
        space_id: f.input.space_id.clone(),
        thread_root_id: None,
    };
    let draft = f
        .client
        .lock()
        .save_message_draft(&scope, "0", "Unsent answer".into(), None)?;
    let before = counts(&f.client)?;
    let ids = f.client.delete_messages(f.input.clone())?;
    assert_eq!(
        ids,
        f.input
            .targets
            .iter()
            .map(|t| t.message_id.clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(counts(&f.client)?, (before.0, before.1 + 2));
    assert_eq!(f.client.lock().message_draft(&scope)?, draft);
    let rows = timeline(&f)?;
    assert!(rows
        .iter()
        .filter(|row| ids.contains(&row.id))
        .all(|row| row.deleted));
    assert_eq!(
        rows.iter().find(|row| row.id == f.unrelated.id),
        Some(&f.unrelated)
    );
    let accepted = counts(&f.client)?;
    assert_eq!(f.client.delete_messages(f.input.clone())?, ids);
    assert_eq!(counts(&f.client)?, accepted);
    let rows = f.client.lock().store.outbox()?;
    assert_eq!(
        rows.iter()
            .filter(|row| matches!(
                &row.event.body,
                types::EventBody::MessageAnnotated {
                    note: types::Annotation::Delete,
                    ..
                }
            ))
            .count(),
        2
    );
    for row in rows {
        ingest(
            &mut f.client.lock(),
            row.event,
            crate::sync::Ingest::Confirmed,
        )?;
    }
    let confirmed = counts(&f.client)?;
    assert_eq!(confirmed.1, 0);
    assert_eq!(f.client.delete_messages(f.input.clone())?, ids);
    assert_eq!(counts(&f.client)?, confirmed);
    Ok(())
}

#[test]
fn second_write_storage_failure_rolls_back_every_event_outbox_and_projection() -> TestResult {
    let f = fixture(":memory:")?;
    let before = counts(&f.client)?;
    let rows = timeline(&f)?;
    f.client.lock().store.conn().execute_batch(&format!(
        "CREATE TEMP TRIGGER fail_second_delete BEFORE INSERT ON outbox WHEN (SELECT count(*) FROM outbox) >= {} BEGIN SELECT RAISE(ABORT, 'synthetic second delete failure'); END;", before.1 + 1))?;
    assert!(f.client.delete_messages(f.input.clone()).is_err());
    assert_eq!(counts(&f.client)?, before);
    assert_eq!(timeline(&f)?, rows);
    f.client
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER fail_second_delete;")?;
    f.client.delete_messages(f.input)?;
    assert_eq!(counts(&f.client)?, (before.0, before.1 + 2));
    Ok(())
}

#[test]
fn changed_last_message_or_foreign_author_refuses_the_entire_set() -> TestResult {
    for foreign in [false, true] {
        let mut f = fixture(":memory:")?;
        if foreign {
            let id = ordered(
                &f,
                types::EventBody::MessagePosted {
                    message: types::new_id("msg")?,
                    text: "Someone else's message".into(),
                    attaches: None,
                    reply: None,
                    media: None,
                    forwarded: false,
                },
            )?;
            let row = timeline(&f)?
                .into_iter()
                .find(|row| row.id == id)
                .ok_or("foreign message")?;
            f.input.targets.push(target(&row)?);
        } else {
            f.client.edit_message(
                f.input.space_id.clone(),
                f.input.targets[1].message_id.clone(),
                "Changed after confirmation".into(),
            )?;
        }
        let before = counts(&f.client)?;
        let rows = timeline(&f)?;
        assert!(f.client.delete_messages(f.input.clone()).is_err());
        assert_eq!(counts(&f.client)?, before);
        assert_eq!(timeline(&f)?, rows);
    }
    Ok(())
}

#[test]
fn owner_device_membership_and_integrity_are_revalidated() -> TestResult {
    for change in ["owner", "device", "membership", "reader", "integrity"] {
        let mut f = fixture(":memory:")?;
        match change {
            "owner" => f.input.expected_identity_id = f.founder.clone(),
            "device" => f.input.expected_device_id = "different-device".into(),
            "membership" => {
                ordered(
                    &f,
                    types::EventBody::MemberRemoved {
                        identity: f.input.expected_identity_id.clone(),
                    },
                )?;
            }
            "reader" => {
                ordered(
                    &f,
                    types::EventBody::MemberAdded {
                        identity: f.input.expected_identity_id.clone(),
                        role: types::Role::Reader,
                    },
                )?;
            }
            _ => {
                f.client
                    .lock()
                    .state
                    .spaces
                    .get_mut(&f.input.space_id)
                    .ok_or("space")?
                    .integrity_error = Some("synthetic integrity failure".into());
            }
        }
        let before = counts(&f.client)?;
        assert!(f.client.delete_messages(f.input).is_err(), "{change}");
        assert_eq!(counts(&f.client)?, before);
    }
    Ok(())
}

#[test]
fn missing_cross_space_duplicate_and_unbounded_sets_are_inert() -> TestResult {
    for change in [
        "missing",
        "space",
        "duplicate",
        "empty",
        "large",
        "revision",
        "id",
    ] {
        let mut f = fixture(":memory:")?;
        match change {
            "missing" => f.input.targets[1].message_id = "missing-message".into(),
            "space" => {
                f.input.space_id =
                    f.client
                        .create_group_with("Other".into(), vec![], PrivacyDto::Closed)?
            }
            "duplicate" => f.input.targets.push(f.input.targets[0].clone()),
            "empty" => f.input.targets.clear(),
            "large" => {
                f.input.targets = (0..101)
                    .map(|i| MessageDeletionTarget {
                        message_id: format!("{i}"),
                        expected_revision: "revision".into(),
                    })
                    .collect()
            }
            "revision" => f.input.targets[1].expected_revision = "".into(),
            _ => f.input.targets[1].message_id = "invalid\nmessage".into(),
        }
        let before = counts(&f.client)?;
        assert!(f.client.delete_messages(f.input).is_err(), "{change}");
        assert_eq!(counts(&f.client)?, before);
    }
    Ok(())
}

#[test]
fn lost_ack_and_cold_retry_reuse_terminal_tombstones() -> TestResult {
    let directory = std::env::temp_dir().join(types::new_id("selected-deletion-test")?);
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("client.sqlite");
    let name = path.to_str().ok_or("path")?;
    let f = fixture(name)?;
    f.client.delete_messages(f.input.clone())?;
    let before = counts(&f.client)?;
    let input = f.input.clone();
    let vault = f.vault.clone();
    drop(f);
    let reopened = Client::open(name.into(), "en".into())?;
    assert!(reopened.unlock(vault)?);
    let expected = input
        .targets
        .iter()
        .map(|target| target.message_id.clone())
        .collect::<Vec<_>>();
    assert_eq!(reopened.delete_messages(input.clone())?, expected);
    assert!(reopened
        .timeline(input.space_id)?
        .iter()
        .filter(|row| expected.contains(&row.id))
        .all(|row| row.deleted));
    assert_eq!(counts(&reopened)?, before);
    drop(reopened);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[tokio::test]
async fn actual_flat_json_command_rejects_unknown_fields_without_effects() -> TestResult {
    let f = fixture(":memory:")?;
    let mut json = serde_json::to_value(&f.input)?;
    json["op"] = "delete_messages".into();
    let before = counts(&f.client)?;
    for nested in [false, true] {
        let mut bad = json.clone();
        if nested {
            bad["targets"][0]["unexpected"] = true.into();
        } else {
            bad["unexpected"] = true.into();
        }
        assert!(f.client.client_request(bad.to_string()).await.is_err());
        assert_eq!(counts(&f.client)?, before);
    }
    let result: Vec<String> =
        serde_json::from_str(&f.client.client_request(json.to_string()).await?)?;
    assert_eq!(result.len(), 2);
    assert_eq!(counts(&f.client)?, (before.0, before.1 + 2));
    Ok(())
}
