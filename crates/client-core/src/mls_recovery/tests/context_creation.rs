//! Preserved context creation scenarios; stable test entries stay in the parent.
use super::*;

pub(super) fn a_ready_group_creates_its_context_after_profiles_become_available() {
    let space = "late-profile-capability";
    let mut engine = Engine::open(":memory:").unwrap();
    engine
        .create_account("Ana", "ana", "http://relay.test")
        .unwrap();
    engine.profiles_session_start(false);
    let author = engine.net.author.clone().unwrap();
    let genesis = Envelope::plain(&author.sign_event(
        space,
        "late-profile-genesis",
        1,
        None,
        EventBody::SpaceCreated {
            title: "Late profile capability".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::EndToEnd,
        },
    ));
    let genesis = ordered(&engine, genesis);
    assert_eq!(engine.ingest(genesis), Ingest::Applied);
    engine.create_mls_group(space).unwrap();
    assert!(engine.ready_epoch(space).is_some());
    assert!(!engine.mls_recovery_pass().unwrap());
    assert!(!engine.mls_recovery_pass().unwrap());
    assert!(engine.recovery_context(space).is_none());
    engine.mls_key_packages_published(Ok(()));
    assert!(engine
        .mls_to_claim(Some(
            protocol::KeyPackageClaimClock::from_server_ms(crate::engine::now_ms()).unwrap(),
        ))
        .unwrap()
        .is_none());
    assert!(engine.mls_settled());
    engine.profiles_session_start(true);
    assert!(
        !engine.mls_settled(),
        "new recovery work must prevent an idle exit"
    );
    assert!(engine.mls_recovery_pass().unwrap());
    let handshakes = engine.store.outbox_handshakes().unwrap();
    assert!(handshakes
        .iter()
        .any(|(_, id, kind)| id == space && kind == SealedKind::Commit.name()));
    assert!(!engine.pending_uploads(8).is_empty());
    assert!(!engine.mls_recovery_pass().unwrap());
}

pub(super) fn an_empty_claim_reply_resumes_context_creation_for_a_ready_group() {
    let space = "empty-claim-context";
    let mut engine = Engine::open(":memory:").unwrap();
    engine
        .create_account("Ana", "ana", "http://relay.test")
        .unwrap();
    let mut peer = Engine::open(":memory:").unwrap();
    peer.create_account("Bruno", "bruno", "http://relay.test")
        .unwrap();
    engine.profiles_session_start(true);
    peer.profiles_session_start(true);
    let identity = peer.me.clone().unwrap();
    let author = engine.net.author.clone().unwrap();
    for (cid, body) in [
        (
            "empty-claim-genesis",
            EventBody::SpaceCreated {
                title: "Empty claim context".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::EndToEnd,
            },
        ),
        (
            "empty-claim-peer",
            EventBody::MemberAdded {
                identity: identity.clone(),
                role: Role::Member,
            },
        ),
    ] {
        let env = Envelope::plain(&author.sign_event(space, cid, 1, seen(&engine, space), body));
        assert_eq!(engine.ingest(ordered(&engine, env)), Ingest::Applied);
    }
    engine.create_mls_group(space).unwrap();
    let (public, signed) = peer.agreement_to_publish().unwrap();
    engine.agreement_keys_arrived(
        vec![protocol::AgreementKeyRecord {
            identity: identity.clone(),
            public,
            signed,
        }],
        std::slice::from_ref(&identity),
    );
    let (claimed_space, targets, operation) = engine
        .mls_to_claim(Some(
            protocol::KeyPackageClaimClock::from_server_ms(crate::engine::now_ms()).unwrap(),
        ))
        .unwrap()
        .unwrap();
    assert_eq!(claimed_space, space);
    assert_eq!(targets, [identity]);
    assert!(engine.ready_epoch(space).is_none());
    assert!(!engine.mls_recovery_pass().unwrap());
    engine.mls_claimed(space, &operation, Ok(Vec::new()));
    assert!(engine.ready_epoch(space).is_some());
    assert!(
        engine.mls_recovery_pass().unwrap(),
        "a ready group must resume creating its recovery context after an empty claim reply"
    );
    assert!(!engine.pending_uploads(8).is_empty());
}

pub(super) fn a_missing_group_recovers_when_its_validated_blob_arrives_later() {
    let space = "late-recovery-blob";
    let staged = staged_recovery(":memory:", space);
    let mut engine = restored_receiver(&staged);
    assert!(!engine.mls_recovery_pass().unwrap());
    assert!(!engine.mls_recovery_pass().unwrap());
    let reference = engine.recovery_context(space).unwrap();
    assert!(!engine.recovery_context_arrived(&reference, b"altered recovery blob"));
    assert!(!engine.mls_recovery_pass().unwrap());
    assert!(engine.recovery_context_arrived(&reference, &staged.context_blob));
    assert!(engine.mls_recovery_pass().unwrap());
    assert!(engine.device().unwrap().recovery_pending(space));
    assert!(engine.outbox_envelopes().is_empty());
}

pub(super) fn a_recovery_error_keeps_its_group_queued_through_a_future_retry() {
    let space = "future-recovery-retry";
    let staged = staged_recovery(":memory:", space);
    let mut engine = restored_receiver(&staged);
    let reference = engine.recovery_context(space).unwrap();
    assert!(engine.recovery_context_arrived(&reference, &staged.context_blob));
    engine.net.profiles.agreement = None;
    assert!(engine.mls_recovery_pass().is_err());
    engine
        .unlock_profile(Some(staged.agreement.to_vec()))
        .unwrap();
    engine
        .net
        .mls
        .recovery_retry_at
        .insert(space.into(), Instant::now() + Duration::from_secs(60));
    assert!(!engine.mls_recovery_pass().unwrap());
    assert!(engine.store.outbox_handshakes().unwrap().is_empty());
    engine
        .net
        .mls
        .recovery_retry_at
        .insert(space.into(), Instant::now() - Duration::from_secs(1));
    assert!(engine.mls_recovery_pass().unwrap());
    assert!(engine.device().unwrap().recovery_pending(space));
}

pub(super) fn producing_one_context_preserves_the_other_queued_groups() {
    let mut engine = Engine::open(":memory:").unwrap();
    engine
        .create_account("Ana", "ana", "http://relay.test")
        .unwrap();
    engine.profiles_session_start(true);
    let author = engine.net.author.clone().unwrap();
    for space in ["first-context", "second-context"] {
        let env = Envelope::plain(&author.sign_event(
            space,
            &format!("genesis-{space}"),
            1,
            None,
            EventBody::SpaceCreated {
                title: space.into(),
                kind: SpaceKind::Group,
                privacy: Privacy::EndToEnd,
            },
        ));
        assert_eq!(engine.ingest(ordered(&engine, env)), Ingest::Applied);
        engine.create_mls_group(space).unwrap();
    }
    assert!(engine.mls_recovery_pass().unwrap());
    assert_eq!(engine.store.outbox_handshakes().unwrap().len(), 1);
    assert!(engine.mls_recovery_pass().unwrap());
    let handshakes = engine.store.outbox_handshakes().unwrap();
    let spaces: std::collections::BTreeSet<_> =
        handshakes.into_iter().map(|(_, space, _)| space).collect();
    assert_eq!(
        spaces,
        std::collections::BTreeSet::from(["first-context".into(), "second-context".into()])
    );
    assert!(!engine.mls_recovery_pass().unwrap());
}

pub(super) fn agreement_lookup_finishes_before_the_engine_requests_one_shot_key_packages() {
    let mut engine = Engine::open(":memory:").unwrap();
    engine
        .create_account("Ana", "ana", "http://relay.test")
        .unwrap();
    let mut peer = Engine::open(":memory:").unwrap();
    peer.create_account("Bruno", "bruno", "http://relay.test")
        .unwrap();
    engine.net.profiles.supported = true;
    peer.net.profiles.supported = true;
    let author = engine.net.author.clone().unwrap();
    let identity = peer.me.clone().unwrap();
    let space = "agreement-before-claim";
    for (cid, body) in [
        (
            "genesis",
            EventBody::SpaceCreated {
                title: "No wasted claims".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::EndToEnd,
            },
        ),
        (
            "member",
            EventBody::MemberAdded {
                identity: identity.clone(),
                role: Role::Member,
            },
        ),
    ] {
        let env = Envelope::plain(&author.sign_event(space, cid, 1, seen(&engine, space), body));
        assert_eq!(engine.ingest(ordered(&engine, env)), Ingest::Applied);
    }
    engine.create_mls_group(space).unwrap();
    assert!(engine
        .mls_to_claim(Some(
            protocol::KeyPackageClaimClock::from_server_ms(crate::engine::now_ms()).unwrap()
        ))
        .unwrap()
        .is_none());
    assert!(engine.take_need_agreement().contains(&identity));
    assert_eq!(engine.outbox_len(), 0);
    let (public, signed) = peer.agreement_to_publish().unwrap();
    engine.agreement_keys_arrived(
        vec![protocol::AgreementKeyRecord {
            identity: identity.clone(),
            public,
            signed,
        }],
        std::slice::from_ref(&identity),
    );
    let (claimed_space, targets, operation) = engine
        .mls_to_claim(Some(
            protocol::KeyPackageClaimClock::from_server_ms(crate::engine::now_ms()).unwrap(),
        ))
        .unwrap()
        .unwrap();
    assert_eq!(claimed_space, space);
    assert_eq!(targets, vec![identity.clone()]);
    let package = peer
        .device()
        .unwrap()
        .key_packages(1, false)
        .unwrap()
        .remove(0);
    engine.mls_claimed(
        space,
        &operation,
        Ok(vec![protocol::KeyPackageRecord {
            identity,
            device: peer.net.account.as_ref().unwrap().device.clone(),
            data: package,
        }]),
    );
    assert_eq!(
        engine.outbox_len(),
        2,
        "one claim stages a commit and Welcome"
    );
    assert!(engine.device().unwrap().pending(space));
    assert_eq!(engine.uploads_pending(), 1);
}
