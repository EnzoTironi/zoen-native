//! Complete transport recovery scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn unconfirmed_genesis_holds_its_descendants_across_retry_and_relaunch() {
    use event_log::chain_hash;
    use protocol::Sequenced;
    use types::GENESIS_PREV;

    let dir = std::env::temp_dir().join(format!("zoen-genesis-{}", types::new_id("t")));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("core.sqlite").to_string_lossy().to_string();
    let vault: Arc<dyn SecretVault> = Arc::new(MemVault::default());
    let (space, genesis, message_id) = {
        let e = Client::open(path.clone(), "en".into()).unwrap();
        e.create_account(
            "Ana".into(),
            "ana".into(),
            "http://127.0.0.1:9".into(),
            vault.clone(),
        )
        .unwrap();
        let space = e
            .create_group_with("Offline".into(), vec![], PrivacyDto::Closed)
            .unwrap();
        e.send_message(space.clone(), "keep this queued".into())
            .unwrap();
        let engine = e.lock();
        let queued = engine.store.outbox().unwrap();
        let genesis = queued
            .iter()
            .find(|p| p.event.space == space && p.event.seen.is_none())
            .unwrap();
        let message_id = queued
            .iter()
            .find(|p| p.event.space == space && p.event.seen.is_some())
            .unwrap()
            .event
            .client_id
            .clone();
        let genesis = protocol::Envelope::plain(&genesis.event);
        let outgoing = engine.outbox_envelopes();
        assert!(outgoing
            .iter()
            .any(|e| e.client_id() == genesis.client_id()));
        assert!(!outgoing.iter().any(|e| e.client_id() == message_id));
        drop(engine);
        e.lock().reject(
            genesis.client_id(),
            "owner temporarily unavailable",
            false,
            None,
        );
        assert!(!e
            .lock()
            .outbox_envelopes()
            .iter()
            .any(|e| e.client_id() == message_id));
        (space, genesis, message_id)
    };
    let e = Client::open(path, "en".into()).unwrap();
    e.unlock(vault).unwrap();
    assert!(!e
        .lock()
        .outbox_envelopes()
        .iter()
        .any(|e| e.client_id() == message_id));
    let ev = Sequenced {
        seq: 0,
        prev: GENESIS_PREV.into(),
        hash: chain_hash(&space, 0, GENESIS_PREV, &genesis.wire_hash()),
        env: genesis,
    };
    assert_eq!(e.lock().ingest(ev), crate::sync::Ingest::Confirmed);
    let outgoing = e.lock().outbox_envelopes();
    assert_eq!(
        outgoing
            .iter()
            .filter(|e| e.client_id() == message_id)
            .count(),
        1
    );
    assert_eq!(
        e.timeline(space)
            .unwrap()
            .iter()
            .filter(
                |e| matches!(&e.kind, EntryKind::Message { text, .. } if text == "keep this queued")
            )
            .count(),
        1
    );
    drop(e);
    std::fs::remove_dir_all(dir).unwrap();
}

pub(super) fn permanently_rejected_genesis_fails_descendants_atomically_across_relaunch() {
    let dir = std::env::temp_dir().join(format!("zoen-genesis-refusal-{}", types::new_id("t")));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("core.sqlite").to_string_lossy().to_string();
    let vault: Arc<dyn SecretVault> = Arc::new(MemVault::default());
    let (bad, good) = {
        let e = Client::open(path.clone(), "en".into()).unwrap();
        e.create_account(
            "Ana".into(),
            "ana".into(),
            "http://127.0.0.1:9".into(),
            vault.clone(),
        )
        .unwrap();
        let good = e
            .create_group_with("Keep queued".into(), vec![], PrivacyDto::Closed)
            .unwrap();
        e.send_message(good.clone(), "unrelated".into()).unwrap();
        let bad = e
            .create_group_with("x".repeat(100 * 1024), vec![], PrivacyDto::Closed)
            .unwrap();
        for text in ["first descendant", "second descendant"] {
            e.send_message(bad.clone(), text.into()).unwrap();
        }
        let genesis = e
            .lock()
            .outbox_envelopes()
            .into_iter()
            .find(|env| env.space() == bad)
            .unwrap();
        assert!(
            genesis.stored_len() > 90 * 1024,
            "this creation is permanently too large for the relay"
        );
        let child = e
            .lock()
            .store
            .outbox()
            .unwrap()
            .into_iter()
            .find(|p| p.event.space == bad && p.event.seen.is_some())
            .unwrap()
            .event
            .client_id;
        let invite_key = format!("invite:{child}");
        {
            let engine = e.lock();
            engine.store.set_meta(&invite_key, "0123456789").unwrap();
            engine.store.conn().execute_batch("CREATE TEMP TRIGGER refuse_meta_delete BEFORE DELETE ON meta WHEN OLD.key LIKE 'invite:%' BEGIN SELECT RAISE(ABORT, 'injected write failure'); END;").unwrap();
        }
        assert!(!e
            .lock()
            .reject(genesis.client_id(), "too large", true, None)
            .user_failed());
        {
            let engine = e.lock();
            assert!(engine.store.conn().is_autocommit());
            assert!(engine
                .store
                .outbox()
                .unwrap()
                .iter()
                .filter(|p| p.event.space == bad)
                .all(|p| !p.failed));
            assert_eq!(
                engine.store.meta(&invite_key).unwrap().as_deref(),
                Some("0123456789")
            );
            engine
                .store
                .conn()
                .execute_batch("DROP TRIGGER refuse_meta_delete")
                .unwrap();
        }
        assert!(e
            .lock()
            .reject(genesis.client_id(), "too large", true, None)
            .user_failed());
        {
            let engine = e.lock();
            let queued = engine.store.outbox().unwrap();
            let refused: Vec<_> = queued.iter().filter(|p| p.event.space == bad).collect();
            assert_eq!(refused.len(), 3);
            assert!(refused
                .iter()
                .all(|p| p.failed && p.last_error.as_deref() == Some("too large")));
            assert!(queued
                .iter()
                .filter(|p| p.event.space == good)
                .all(|p| !p.failed));
            assert!(engine.store.meta(&invite_key).unwrap().is_none());
            assert!(!engine.net.pending.values().any(|space| space == &bad));
            assert!(!engine
                .outbox_envelopes()
                .iter()
                .any(|env| env.space() == bad));
        }
        (bad, good)
    };
    let e = Client::open(path, "en".into()).unwrap();
    e.unlock(vault).unwrap();
    let engine = e.lock();
    assert!(engine
        .store
        .outbox()
        .unwrap()
        .iter()
        .filter(|p| p.event.space == bad)
        .all(|p| p.failed));
    assert!(!engine.net.pending.values().any(|space| space == &bad));
    assert!(!engine
        .outbox_envelopes()
        .iter()
        .any(|env| env.space() == bad));
    assert!(engine
        .outbox_envelopes()
        .iter()
        .any(|env| env.space() == good));
    drop(engine);
    drop(e);
    std::fs::remove_dir_all(dir).unwrap();
}

pub(super) fn a_device_refuses_a_relay_that_hides_a_message_and_rehashes() {
    use event_log::{chain_hash, content_hash_of, Author, Signer, SpaceLog};
    use protocol::{Envelope, Sequenced};
    use types::{EventBody, Privacy, SpaceKind};

    let e = Client::open(":memory:".into(), "pt-BR".into()).unwrap();
    let ana = Author::root(Signer::generate());
    let bia = Author::root(Signer::generate());
    let mut honest = SpaceLog::new("sp_fork");
    let genesis = honest
        .sequence(ana.sign_event(
            "sp_fork",
            "g",
            1,
            None,
            EventBody::SpaceCreated {
                title: "Viagem".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::Closed,
            },
        ))
        .clone();
    let seen = honest.head();
    let hidden = honest
        .sequence(ana.sign_event(
            "sp_fork",
            "h",
            2,
            seen,
            EventBody::MessagePosted {
                message: "m1".into(),
                text: "não vou mais".into(),
                attaches: None,
                reply: None,
                media: None,
                forwarded: false,
            },
        ))
        .clone();
    let reply = bia.sign_event(
        "sp_fork",
        "r",
        3,
        honest.head(),
        EventBody::MessagePosted {
            message: "m2".into(),
            text: "ok, entendi".into(),
            attaches: None,
            reply: None,
            media: None,
            forwarded: false,
        },
    );
    assert_eq!(
        reply.seen.as_ref().map(|s| s.hash.as_str()),
        Some(hidden.hash.as_str())
    );

    let shown = |ev: &types::Event, seq: u64, prev: &str| Sequenced {
        seq,
        prev: prev.to_string(),
        hash: chain_hash("sp_fork", seq, prev, &content_hash_of(ev)),
        env: Envelope::plain(ev),
    };
    assert!(matches!(
        e.lock().ingest(shown(&genesis, 0, &genesis.prev)),
        crate::sync::Ingest::Applied
    ));
    match e.lock().ingest(shown(&reply, 1, &genesis.hash)) {
        crate::sync::Ingest::Invalid(why) => assert!(why.contains("história"), "{why}"),
        _ => panic!("the forged history was accepted"),
    }
    let space = e
        .spaces()
        .into_iter()
        .find(|s| s.title == "Viagem")
        .unwrap();
    assert!(!space.last_preview.contains("ok, entendi"));
}

pub(super) fn immediate_cold_reopen_after_sync_stop() {
    let folder = std::env::temp_dir().join(types::new_id("net-cold-reopen"));
    std::fs::create_dir_all(&folder).unwrap();
    let mut failures = Vec::new();
    for sample in 0..32 {
        let path = folder
            .join(format!("{sample}.sqlite"))
            .to_string_lossy()
            .to_string();
        let plain = Client::open(path.clone(), "en".into()).unwrap();
        drop(plain);
        let core = Client::open(path.clone(), "en".into()).unwrap();
        core.create_account(
            "Lifecycle".into(),
            format!("lifecycle_{sample}"),
            "http://127.0.0.1:9".into(),
            Arc::new(MemVault::default()),
        )
        .unwrap();
        core.start_sync(None).unwrap();
        let began = std::time::Instant::now();
        core.stop_sync();
        core.stop_sync();
        drop(core);
        let elapsed = began.elapsed();
        match Client::open(path, "en".into()) {
            Ok(reopened) => {
                reopened
                    .lock()
                    .store
                    .set_meta("reopen-proof", "written")
                    .unwrap();
                assert_eq!(
                    reopened
                        .lock()
                        .store
                        .meta("reopen-proof")
                        .unwrap()
                        .as_deref(),
                    Some("written")
                );
                drop(reopened);
            }
            Err(error) => failures.push(format!("sample={sample} error={error}")),
        }
        eprintln!(
            "cold_reopen sample={sample} stop_destroy_us={}",
            elapsed.as_micros()
        );
    }
    std::fs::remove_dir_all(folder).unwrap();
    assert!(
        failures.is_empty(),
        "{} immediate reopen failures: {failures:?}",
        failures.len()
    );
}

pub(super) fn immediate_cold_reopen_after_implicit_sync_drop() {
    for sample in 0..32 {
        let path = std::env::temp_dir().join(types::new_id("implicit-net-drop"));
        let path = path.to_string_lossy().to_string();
        let core = Client::open(path.clone(), "en".into()).unwrap();
        core.create_account(
            "Lifecycle".into(),
            format!("implicit_{sample}"),
            "http://127.0.0.1:9".into(),
            Arc::new(MemVault::default()),
        )
        .unwrap();
        core.start_sync(None).unwrap();
        drop(core);
        let reopened = Client::open(path.clone(), "en".into()).unwrap();
        reopened
            .lock()
            .store
            .set_meta("reopen-proof", "written")
            .unwrap();
        assert_eq!(
            reopened
                .lock()
                .store
                .meta("reopen-proof")
                .unwrap()
                .as_deref(),
            Some("written")
        );
        drop(reopened);
        std::fs::remove_file(path).unwrap();
    }
}

pub(super) async fn immediate_cold_reopen_inside_an_unrelated_runtime() {
    immediate_cold_reopen_after_sync_stop();
    immediate_cold_reopen_after_implicit_sync_drop();
}

pub(super) fn callback_can_stop_and_destroy_its_core_before_immediate_reopen() {
    use std::sync::mpsc;
    use std::time::Duration;
    struct Listener {
        core: Mutex<Option<Arc<Client>>>,
        release: Mutex<mpsc::Receiver<()>>,
        done: mpsc::Sender<()>,
    }
    impl CoreListener for Listener {
        fn on_connection(&self, _: ConnectionDto) {
            let core = self.core.lock().unwrap().take();
            if let Some(core) = core {
                self.release
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap();
                assert_eq!(std::thread::current().name(), Some("zoen-net"));
                core.stop_sync();
                core.stop_sync();
                drop(core);
                self.done.send(()).unwrap();
            }
        }
        fn on_change(&self, _: Vec<String>) {}
        fn on_ephemeral(&self, _: String, _: String, _: String, _: String) {}
        fn on_presence(&self, _: String, _: bool) {}
        fn on_error(&self, _: String) {}
        fn on_profile_changed(&self, _: String) {}
    }
    let path = std::env::temp_dir().join(types::new_id("reentrant-net-drop"));
    let path = path.to_string_lossy().to_string();
    let core = Client::open(path.clone(), "en".into()).unwrap();
    core.create_account(
        "Reentrant".into(),
        "reentrant".into(),
        "http://127.0.0.1:9".into(),
        Arc::new(MemVault::default()),
    )
    .unwrap();
    let (release, released) = mpsc::channel();
    let (done, completed) = mpsc::channel();
    let listener = Arc::new(Listener {
        core: Mutex::new(Some(core.clone())),
        release: Mutex::new(released),
        done,
    });
    core.start_sync(Some(listener)).unwrap();
    drop(core);
    release.send(()).unwrap();
    completed.recv_timeout(Duration::from_secs(5)).unwrap();
    let reopened = Client::open(path.clone(), "en".into()).unwrap();
    reopened
        .lock()
        .store
        .set_meta("callback-proof", "written")
        .unwrap();
    drop(reopened);
    std::fs::remove_file(path).unwrap();
}
