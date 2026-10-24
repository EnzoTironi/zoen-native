//! Preserved initial welcome scenarios; stable test entries stay in the parent.
use super::*;

pub(super) fn live_direct_recipient_keeps_initial_welcome_before_recovery_context(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    initial_welcome_beats_recovery(false)?;

    Ok(())
}

pub(super) fn live_direct_recipient_keeps_initial_welcome_after_cold_reopen(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    initial_welcome_beats_recovery(true)?;

    Ok(())
}

fn initial_welcome_beats_recovery(
    cold_reopen: bool,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let space = "direct-live-welcome-recovery-race";
    let mut owner = Engine::open(":memory:").unwrap();
    owner
        .create_account("Ana", "ana", "http://relay.test")
        .unwrap();
    let path = std::env::temp_dir().join(format!(
        "zoen-initial-welcome-{}.db",
        types::new_ulid(now_ms())?
    ));
    let mut peer = Engine::open(if cold_reopen {
        path.to_str().unwrap()
    } else {
        ":memory:"
    })
    .unwrap();
    let (_, peer_secret, peer_agreement) = peer
        .create_account("Bruno", "bruno", "http://relay.test")
        .unwrap();
    owner.profiles_session_start(true);
    peer.profiles_session_start(true);
    let (public, signed) = peer.agreement_to_publish().unwrap();
    let identity = peer.me.clone().unwrap();
    owner.agreement_keys_arrived(
        vec![protocol::AgreementKeyRecord {
            identity: identity.clone(),
            public,
            signed,
        }],
        &[identity],
    );
    let (public, signed) = owner.agreement_to_publish().unwrap();
    let identity = owner.me.clone().unwrap();
    peer.agreement_keys_arrived(
        vec![protocol::AgreementKeyRecord {
            identity: identity.clone(),
            public,
            signed,
        }],
        &[identity],
    );
    let author = owner.net.author.clone().unwrap();
    for (cid, body) in [
        (
            "live-genesis",
            EventBody::SpaceCreated {
                title: "Live direct".into(),
                kind: SpaceKind::Direct,
                privacy: Privacy::EndToEnd,
            },
        ),
        (
            "live-member",
            EventBody::MemberAdded {
                identity: peer.me.clone().unwrap(),
                role: Role::Member,
            },
        ),
    ] {
        let frame = ordered(
            &owner,
            Envelope::plain(&author.sign_event(space, cid, 1, seen(&owner, space), body)?),
        );
        assert_eq!(owner.ingest(frame.clone()), Ingest::Applied);
        assert_eq!(peer.ingest(frame), Ingest::Applied);
    }
    owner.create_mls_group(space).unwrap();
    let packages = peer.device().unwrap().key_packages(1, false).unwrap();
    owner
        .commit_now(space, &packages, &Default::default())
        .unwrap();
    let queued = owner.store.outbox().unwrap();
    let commit = queued.iter().find(|p| matches!(&p.event.body, EventBody::Sealed { kind } if kind == SealedKind::Commit.name())).unwrap();
    let welcome = queued.iter().find(|p| matches!(&p.event.body, EventBody::Sealed { kind } if kind == SealedKind::Welcome.name())).unwrap();
    let reference = RecoveryRef::parse(Envelope::plain(&commit.event).recovery().unwrap()).unwrap();
    let blob = owner
        .pending_uploads(8)
        .into_iter()
        .find(|(sha, _)| *sha == reference.blob)
        .unwrap()
        .1;
    owner.upload_done(&reference.blob);
    let frame = ordered(&owner, Envelope::plain(&commit.event));
    assert_eq!(owner.ingest(frame.clone()), Ingest::Confirmed);
    assert_eq!(peer.ingest(frame), Ingest::Applied);
    // Real publication waits for this Commit confirmation before submitting its
    // Welcome. The live recipient can download the public recovery context first.
    let reference = peer.recovery_context(space).unwrap();
    assert!(peer.recovery_context_arrived(&reference, &blob));
    let recovered_before_welcome = peer.mls_recovery_pass().unwrap();
    assert!(
        !recovered_before_welcome,
        "listed device must wait for its own initial Welcome"
    );
    assert!(peer.store.outbox_handshakes().unwrap().is_empty());
    if cold_reopen {
        drop(peer);
        peer = Engine::open(path.to_str().unwrap()).unwrap();
        assert!(peer.unlock(Some(peer_secret.to_vec())).unwrap());
        peer.unlock_profile(Some(peer_agreement.to_vec())).unwrap();
        peer.profiles_session_start(true);
        assert!(!peer.mls_recovery_pass().unwrap());
        assert!(peer.store.outbox_handshakes().unwrap().is_empty());
    }
    let frame = ordered(&owner, Envelope::plain(&welcome.event));
    assert_eq!(owner.ingest(frame.clone()), Ingest::Confirmed);
    assert_eq!(peer.ingest(frame), Ingest::Applied);
    let seed = owner
        .send_message(space, "original live direct seed")
        .unwrap();
    owner.mls_seal_outbox().unwrap();
    let envelope = owner
        .outbox_envelopes()
        .into_iter()
        .find(|e| e.client_id() == seed.id)
        .unwrap();
    let frame = ordered(&owner, envelope);
    assert_eq!(owner.ingest(frame.clone()), Ingest::Confirmed);
    assert_eq!(peer.ingest(frame), Ingest::Applied);
    assert!(peer.timeline(space).unwrap().iter().any(|e| matches!(&e.kind, crate::dto::EntryKind::Message { text, .. } if text == "original live direct seed")),
        "initial Welcome must remain usable after early recovery context; recovered_before_welcome={recovered_before_welcome}");
    drop(peer);
    Ok(if cold_reopen {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
    })
}
