use super::*;
use event_log::{Author, Signer, SpaceLog};

fn engine() -> Engine {
    let mut engine = Engine::open(":memory:").unwrap();
    engine
        .create_account("Saved owner", "saved_owner", "http://127.0.0.1:1")
        .unwrap();
    engine
}
fn owner(engine: &Engine) -> (String, String) {
    let account = engine.net.account.as_ref().unwrap();
    (account.identity.clone(), account.device.clone())
}
fn confirmed(engine: &Engine, env: Envelope) -> protocol::Sequenced {
    let mut log = SpaceLog::new(env.space());
    let event = env.plain_event().unwrap();
    log.sequence(event).unwrap();
    let event = &log.events()[0];
    assert!(engine
        .saved_messages_id(env.author(), env.device().unwrap())
        .is_ok());
    protocol::Sequenced {
        seq: event.seq,
        prev: event.prev.clone(),
        hash: event.hash.clone(),
        env,
    }
}

#[test]
fn saved_messages_legacy_negotiation_has_no_bootstrap_effect() {
    let mut engine = engine();
    let (identity, device) = owner(&engine);
    let before = engine.store.outbox().unwrap().len();
    assert!(engine.prepare_saved_messages(&identity, &device).is_err());
    assert_eq!(engine.store.outbox().unwrap().len(), before);
    let space = engine.saved_messages_id(&identity, &device).unwrap();
    assert!(!engine.state.spaces.contains_key(&space));
}
#[test]
fn saved_messages_lost_reply_reuses_exact_intent_and_tentative_keys_without_early_admission() {
    let mut engine = engine();
    let (identity, device) = owner(&engine);
    engine.net.saved_messages_supported = true;
    let proposal = engine
        .prepare_saved_messages(&identity, &device)
        .unwrap()
        .unwrap();
    let space = engine.saved_messages_id(&identity, &device).unwrap();
    let original_keys = engine.mls_status(&space).unwrap();
    assert!(engine.device().unwrap().has_group(&space));
    assert!(!engine.is_e2e(&space));
    assert!(engine.ready_epoch(&space).is_none());
    engine.reload_mls();
    assert!(engine.mls_to_claim(None).unwrap().is_none());
    assert!(!engine.space_summary(&space).unwrap().can_write);
    assert!(engine
        .send_message(&space, "keep draft until prepared")
        .is_err());
    assert_eq!(
        engine.prepare_saved_messages(&identity, &device).unwrap(),
        Some(proposal.clone())
    );
    assert_eq!(engine.mls_status(&space).unwrap(), original_keys);
    assert!(engine.outbox_envelopes().iter().all(|e| e.space() != space));
    let canonical = confirmed(&engine, proposal.envelope().unwrap());
    assert!(matches!(
        engine.ingest(canonical),
        crate::sync::Ingest::Confirmed
    ));
    assert!(engine
        .prepare_saved_messages(&identity, &device)
        .unwrap()
        .is_none());
    let summary = engine.saved_messages_ready(&space).unwrap();
    assert_eq!(summary.kind, SpaceKindDto::Saved);
    assert!(summary.can_write);
    assert!(engine
        .store
        .outbox()
        .unwrap()
        .iter()
        .all(|e| e.event.space != space));
    engine.net.saved_messages_supported = false;
    assert!(engine.saved_messages_ready(&space).unwrap().can_write);
    engine
        .send_message(&space, "retained ordinary offline message")
        .unwrap();
    let retained = engine
        .store
        .outbox()
        .unwrap()
        .into_iter()
        .find(|row| row.event.space == space)
        .unwrap()
        .event;
    assert!(Envelope::plain(&retained).unwrap().verify().is_ok());
    assert!(engine
        .outbox_envelopes()
        .iter()
        .all(|env| env.space() != space));
    engine.net.saved_messages_supported = true;
    engine.mls_seal_outbox().unwrap();
    assert!(engine
        .outbox_envelopes()
        .iter()
        .any(|env| env.space() == space
            && env.client_id() == retained.client_id
            && env.is_sealed()));
}
#[test]
fn saved_messages_foreign_owner_revocation_and_membership_are_refused() {
    let mut engine = engine();
    let (identity, device) = owner(&engine);
    engine.net.saved_messages_supported = true;
    assert!(engine
        .prepare_saved_messages(&"ab".repeat(32), &device)
        .is_err());
    engine
        .store
        .set_meta(
            "devices.revoked",
            &serde_json::to_string(&vec![&device]).unwrap(),
        )
        .unwrap();
    assert!(engine.prepare_saved_messages(&identity, &device).is_err());
    engine.store.set_meta("devices.revoked", "[]").unwrap();
    let proposal = engine
        .prepare_saved_messages(&identity, &device)
        .unwrap()
        .unwrap();
    let space = proposal.envelope().unwrap().space().to_owned();
    assert!(engine
        .saved_messages_admit(
            &space,
            &identity,
            &EventBody::MemberAdded {
                identity: "cd".repeat(32),
                role: Role::Reader
            }
        )
        .is_err());
    assert!(engine
        .saved_messages_admit(
            &space,
            &identity,
            &EventBody::MemberRemoved {
                identity: identity.clone()
            }
        )
        .is_err());
}

#[test]
fn saved_messages_losing_device_adopts_original_genesis_without_an_independent_mls_group() {
    let mut engine = Engine::open(":memory:").unwrap();
    let (root, _, _) = engine
        .create_account("Saved owner", "saved_owner", "http://127.0.0.1:1")
        .unwrap();
    let (identity, device) = owner(&engine);
    engine.net.saved_messages_supported = true;
    let losing = engine
        .prepare_saved_messages(&identity, &device)
        .unwrap()
        .unwrap();
    let space = engine.saved_messages_id(&identity, &device).unwrap();
    let winner = Author::device(&Signer::from_secret(&root), Signer::from_secret(&[75; 32]));
    let original = winner
        .sign_event(
            &space,
            "winner",
            2,
            None,
            EventBody::SpaceCreated {
                title: String::new(),
                kind: SpaceKind::Direct,
                privacy: Privacy::EndToEnd,
            },
        )
        .unwrap();
    let mut log = SpaceLog::new(&space);
    log.sequence(original.clone()).unwrap();
    let canonical = &log.events()[0];
    let envelope = Envelope::plain(&original).unwrap();
    let receipt = protocol::Sequenced {
        seq: 0,
        prev: canonical.prev.clone(),
        hash: canonical.hash.clone(),
        env: envelope.clone(),
    };
    let tentative_keys = engine.mls_status(&space).unwrap();
    engine.store.conn().execute_batch("CREATE TEMP TRIGGER reject_saved_cleanup BEFORE DELETE ON outbox BEGIN SELECT RAISE(ABORT, 'owned fixture'); END").unwrap();
    assert!(matches!(
        engine.ingest(receipt.clone()),
        crate::sync::Ingest::Invalid(_)
    ));
    assert_eq!(engine.mls_status(&space).unwrap(), tentative_keys);
    assert!(engine
        .store
        .outbox_get(losing.envelope().unwrap().client_id())
        .unwrap()
        .is_some());
    assert_eq!(engine.logs.get(&space).map_or(0, SpaceLog::next_seq), 0);
    engine
        .store
        .conn()
        .execute_batch("DROP TRIGGER reject_saved_cleanup")
        .unwrap();
    assert!(matches!(
        engine.ingest(receipt),
        crate::sync::Ingest::Applied
    ));
    assert_eq!(
        Envelope::plain(&engine.logs[&space].events()[0]).unwrap(),
        envelope
    );
    assert!(engine
        .store
        .outbox()
        .unwrap()
        .iter()
        .all(|row| row.event.client_id != losing.envelope().unwrap().client_id()));
    assert_eq!(
        engine.saved_messages_ready(&space).unwrap().kind,
        SpaceKindDto::Saved
    );
    assert!(!engine.device().unwrap().has_group(&space));
    assert!(!engine.space_summary(&space).unwrap().can_write);
}

#[test]
fn saved_messages_missing_historical_mls_state_never_reinitializes_old_group() {
    let mut engine = engine();
    let (identity, device) = owner(&engine);
    engine.net.saved_messages_supported = true;
    let original = engine
        .prepare_saved_messages(&identity, &device)
        .unwrap()
        .unwrap()
        .envelope()
        .unwrap();
    let space = original.space().to_owned();
    let mut log = SpaceLog::new(&space);
    log.sequence(original.plain_event().unwrap()).unwrap();
    let genesis = log.events()[0].clone();
    assert!(matches!(
        engine.ingest(protocol::Sequenced {
            seq: 0,
            prev: genesis.prev,
            hash: genesis.hash,
            env: original
        }),
        crate::sync::Ingest::Confirmed
    ));
    let author = engine.net.author.as_ref().unwrap();
    let joining = author
        .sign_event(
            &space,
            "joining",
            2,
            log.head(),
            EventBody::DeviceJoining {
                device: device.clone(),
            },
        )
        .unwrap();
    let env = Envelope::plain(&joining).unwrap();
    log.sequence(joining).unwrap();
    let second = log.events()[1].clone();
    assert!(matches!(
        engine.ingest(protocol::Sequenced {
            seq: 1,
            prev: second.prev,
            hash: second.hash,
            env
        }),
        crate::sync::Ingest::Applied
    ));
    engine.device().unwrap().forget(&space).unwrap();
    assert!(!engine.device().unwrap().has_group(&space));
    assert!(!engine.saved_messages_ready(&space).unwrap().can_write);
    assert!(!engine.device().unwrap().has_group(&space));
}

#[test]
fn saved_messages_json_preserves_existing_kinds_and_appends_semantic_saved_kind() {
    assert_eq!(
        serde_json::to_string(&SpaceKindDto::Direct).unwrap(),
        "\"Direct\""
    );
    assert_eq!(
        serde_json::to_string(&SpaceKindDto::Saved).unwrap(),
        "\"saved\""
    );
}

#[test]
fn saved_messages_unreadable_bootstrap_is_never_replaced_or_discarded() {
    for corruption in ["signature", "client-id", "space", "json"] {
        let mut engine = engine();
        let (identity, device) = owner(&engine);
        engine.net.saved_messages_supported = true;
        let proposal = engine
            .prepare_saved_messages(&identity, &device)
            .unwrap()
            .unwrap();
        let event = proposal.envelope().unwrap().plain_event().unwrap();
        let mut damaged = event.clone();
        match corruption {
            "signature" => damaged.sig.clear(),
            "client-id" => damaged.client_id = "different".into(),
            "space" => damaged.space = "sp_other".into(),
            "json" => {}
            _ => unreachable!(),
        }
        let json = if corruption == "json" {
            "{".into()
        } else {
            serde_json::to_string(&damaged).unwrap()
        };
        engine
            .store
            .conn()
            .execute(
                "UPDATE outbox SET json=?1 WHERE client_id=?2",
                rusqlite::params![json, event.client_id],
            )
            .unwrap();
        let before = engine.store.outbox_original_json(&event.client_id).unwrap();
        assert!(
            engine.prepare_saved_messages(&identity, &device).is_err(),
            "{corruption}"
        );
        let mut winner = event.clone();
        winner.client_id = "canonical-winner".into();
        assert!(
            engine.saved_messages_superseded(&winner).is_err(),
            "{corruption}"
        );
        assert_eq!(
            engine.store.outbox_original_json(&event.client_id).unwrap(),
            before
        );
    }
}

#[test]
fn saved_messages_provider_and_bootstrap_roll_back_on_the_same_connection() {
    let mut engine = engine();
    let (identity, device) = owner(&engine);
    let space = engine.saved_messages_id(&identity, &device).unwrap();
    engine.net.saved_messages_supported = true;
    engine.store.conn().execute_batch("CREATE TEMP TRIGGER reject_saved_bootstrap BEFORE INSERT ON outbox BEGIN SELECT RAISE(ABORT, 'owned fixture'); END").unwrap();
    assert!(engine.prepare_saved_messages(&identity, &device).is_err());
    assert_eq!(
        engine.device().unwrap().group_state(&space).unwrap(),
        ::mls::GroupState::Missing
    );
    assert!(engine
        .store
        .outbox_heads()
        .unwrap()
        .iter()
        .all(|row| row.space != space));
    assert!(!engine.state.spaces.contains_key(&space));
    engine
        .store
        .conn()
        .execute_batch("DROP TRIGGER reject_saved_bootstrap")
        .unwrap();
    assert!(engine
        .prepare_saved_messages(&identity, &device)
        .unwrap()
        .is_some());
    assert_eq!(
        engine.device().unwrap().group_state(&space).unwrap(),
        ::mls::GroupState::Ready
    );
}
#[test]
fn saved_messages_lost_bootstrap_keys_preserve_original_and_require_recovery() {
    let mut engine = engine();
    let (identity, device) = owner(&engine);
    engine.net.saved_messages_supported = true;
    let proposal = engine
        .prepare_saved_messages(&identity, &device)
        .unwrap()
        .unwrap();
    let env = proposal.envelope().unwrap();
    let space = env.space().to_owned();
    let original = engine.store.outbox_original_json(env.client_id()).unwrap();
    engine.device().unwrap().forget(&space).unwrap();
    assert!(engine.prepare_saved_messages(&identity, &device).is_err());
    assert_eq!(
        engine.store.outbox_original_json(env.client_id()).unwrap(),
        original
    );
    assert!(!engine.device().unwrap().has_group(&space));
    assert!(!engine.space_summary(&space).unwrap().can_write);
}
#[test]
fn saved_messages_genesis_only_checkpoint_with_remote_tail_and_ciphertext_never_mints_keys() {
    let mut engine = engine();
    let (identity, device) = owner(&engine);
    engine.net.saved_messages_supported = true;
    let proposal = engine
        .prepare_saved_messages(&identity, &device)
        .unwrap()
        .unwrap();
    let env = proposal.envelope().unwrap();
    let space = env.space().to_owned();
    let canonical = confirmed(&engine, env.clone());
    assert!(matches!(
        engine.ingest(canonical),
        crate::sync::Ingest::Confirmed
    ));
    engine.send_message(&space, "retained ciphertext").unwrap();
    engine.mls_seal_outbox().unwrap();
    let ciphertext = engine
        .outbox_envelopes()
        .into_iter()
        .find(|e| e.space() == space)
        .unwrap();
    assert!(ciphertext.is_sealed());
    let retained_ciphertext = engine
        .store
        .meta(&format!("mls.sealed:{}", ciphertext.client_id()))
        .unwrap()
        .unwrap();
    let mut remote = SpaceLog::new(&space);
    remote.sequence(env.plain_event().unwrap()).unwrap();
    remote
        .sequence(
            engine
                .net
                .author
                .as_ref()
                .unwrap()
                .sign_event(
                    &space,
                    "remote-tail",
                    3,
                    remote.head(),
                    EventBody::DeviceJoining {
                        device: device.clone(),
                    },
                )
                .unwrap(),
        )
        .unwrap();
    assert_eq!(remote.next_seq(), 2);
    assert_eq!(engine.logs[&space].next_seq(), 1);
    engine.device().unwrap().forget(&space).unwrap();
    assert!(!engine.saved_messages_ready(&space).unwrap().can_write);
    assert!(!engine.device().unwrap().has_group(&space));
    assert_eq!(
        engine
            .store
            .meta(&format!("mls.sealed:{}", ciphertext.client_id()))
            .unwrap()
            .as_deref(),
        Some(retained_ciphertext.as_str())
    );
}
