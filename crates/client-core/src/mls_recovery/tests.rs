use super::*;
use crate::sync::Ingest;
use event_log::{chain_hash, SpaceLog};
use protocol::Sequenced;
use types::{EventBody, Privacy, SpaceKind};

fn ordered(engine: &Engine, env: Envelope) -> Sequenced {
    let (seq, prev) = engine
        .logs
        .get(env.space())
        .map(|log| {
            (
                log.next_seq(),
                log.events()
                    .last()
                    .map(|e| e.hash.clone())
                    .unwrap_or_else(|| types::GENESIS_PREV.into()),
            )
        })
        .unwrap_or_else(|| (0, types::GENESIS_PREV.into()));
    Sequenced {
        seq,
        hash: chain_hash(env.space(), seq, &prev, &env.wire_hash()),
        prev,
        env,
    }
}

fn seen(engine: &Engine, space: &str) -> Option<types::Seen> {
    engine
        .logs
        .get(space)
        .and_then(|log| log.events().last())
        .map(|e| types::Seen {
            seq: e.seq,
            hash: e.hash.clone(),
        })
}

struct StagedRecovery {
    engine: Engine,
    peer_engine: Engine,
    root: [u8; 32],
    device: [u8; 32],
    agreement: [u8; 32],
    peer: protocol::AgreementKeyRecord,
    baseline: Vec<Sequenced>,
    context_blob: Vec<u8>,
}

#[test]
fn a_ready_group_creates_its_context_after_profiles_become_available() {
    context_creation::a_ready_group_creates_its_context_after_profiles_become_available();
}

#[test]
fn an_empty_claim_reply_resumes_context_creation_for_a_ready_group() {
    context_creation::an_empty_claim_reply_resumes_context_creation_for_a_ready_group();
}

#[test]
fn a_missing_group_recovers_when_its_validated_blob_arrives_later() {
    context_creation::a_missing_group_recovers_when_its_validated_blob_arrives_later().unwrap();
}

fn restored_receiver(
    staged: &StagedRecovery,
) -> std::result::Result<Engine, Box<dyn std::error::Error>> {
    let mut engine = Engine::open(":memory:").unwrap();
    engine
        .install_restored_account(
            &event_log::Signer::from_secret(&staged.root),
            event_log::Signer::generate()?,
            "http://relay.test",
        )
        .unwrap();
    engine
        .unlock_profile(Some(staged.agreement.to_vec()))
        .unwrap();
    engine.profiles_session_start(true);
    engine.agreement_keys_arrived(
        vec![staged.peer.clone()],
        std::slice::from_ref(&staged.peer.identity),
    );
    for confirmed in &staged.baseline {
        assert_eq!(engine.ingest(confirmed.clone()), Ingest::Applied);
    }
    Ok(engine)
}

#[test]
fn a_recovery_error_keeps_its_group_queued_through_a_future_retry() {
    context_creation::a_recovery_error_keeps_its_group_queued_through_a_future_retry().unwrap();
}

#[test]
fn producing_one_context_preserves_the_other_queued_groups() {
    context_creation::producing_one_context_preserves_the_other_queued_groups();
}

fn staged_recovery(
    path: &str,
    space: &str,
) -> std::result::Result<StagedRecovery, Box<dyn std::error::Error>> {
    let mut engine = Engine::open(path).unwrap();
    let (root, _, agreement) = engine
        .create_account("Ana", "ana", "http://relay.test")
        .unwrap();
    let mut peer_engine = Engine::open(":memory:").unwrap();
    peer_engine
        .create_account("Bruno", "bruno", "http://relay.test")
        .unwrap();
    engine.net.profiles.supported = true;
    peer_engine.net.profiles.supported = true;
    let (public, signed) = peer_engine.agreement_to_publish().unwrap();
    let peer = protocol::AgreementKeyRecord {
        identity: peer_engine.me.clone().unwrap(),
        public,
        signed,
    };
    engine.agreement_keys_arrived(vec![peer.clone()], std::slice::from_ref(&peer.identity));
    let author = engine.net.author.clone().unwrap();
    let mut baseline = Vec::new();
    for (cid, body) in [
        (
            "genesis",
            EventBody::SpaceCreated {
                title: "Held recovery".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::EndToEnd,
            },
        ),
        (
            "add-peer",
            EventBody::MemberAdded {
                identity: peer.identity.clone(),
                role: Role::Member,
            },
        ),
    ] {
        let env = Envelope::plain(&author.sign_event(space, cid, 1, seen(&engine, space), body));
        let confirmed = ordered(&engine, env);
        assert_eq!(engine.ingest(confirmed.clone()), Ingest::Applied);
        baseline.push(confirmed);
    }
    engine.create_mls_group(space).unwrap();
    let packages = peer_engine
        .device()
        .unwrap()
        .key_packages(1, false)
        .unwrap();
    engine
        .commit_now(space, &packages, &Default::default())
        .unwrap();
    let queued = engine.store.outbox().unwrap();
    let reference =
        RecoveryRef::parse(Envelope::plain(&queued[0].event).recovery().unwrap()).unwrap();
    let context_blob = engine
        .pending_uploads(8)
        .into_iter()
        .find(|(sha, _)| *sha == reference.blob)
        .unwrap()
        .1;
    engine.upload_done(&reference.blob);
    for queued in queued {
        let confirmed = ordered(&engine, Envelope::plain(&queued.event));
        assert_eq!(engine.ingest(confirmed.clone()), Ingest::Confirmed);
        baseline.push(confirmed);
    }
    engine.device().unwrap().forget(space).unwrap();
    let replacement = event_log::Signer::generate()?;
    let device = replacement.secret();
    engine
        .install_restored_account(
            &event_log::Signer::from_secret(&root),
            replacement,
            "http://relay.test",
        )
        .unwrap();
    engine.unlock_profile(Some(agreement.to_vec())).unwrap();
    engine.net.profiles.supported = true;
    let reference = engine.recovery_context(space).unwrap();
    assert!(engine.recovery_context_arrived(&reference, &context_blob));
    assert!(engine.mls_recovery_pass().unwrap());
    assert!(engine.device().unwrap().recovery_pending(space));
    assert!(engine.outbox_envelopes().is_empty());
    Ok(StagedRecovery {
        engine,
        peer_engine,
        root,
        device,
        agreement,
        peer,
        baseline,
        context_blob,
    })
}

#[test]
fn peer_external_commit_storage_failure_keeps_the_cursor_epoch_and_context_for_retry() {
    processing_failures::peer_external_commit_storage_failure_keeps_the_cursor_epoch_and_context_for_retry().unwrap();
}

#[test]
fn peer_application_processing_failure_keeps_the_same_frame_readable_on_retry() {
    processing_failures::peer_application_processing_failure_keeps_the_same_frame_readable_on_retry(
    ).unwrap();
}

#[test]
fn welcome_storage_failure_rolls_back_the_key_package_and_cursor_for_retry() {
    processing_failures::welcome_storage_failure_rolls_back_the_key_package_and_cursor_for_retry()
        .unwrap();
}

#[test]
fn a_removal_during_held_recovery_survives_restart_and_repairs_before_sealing() {
    held_recovery::a_removal_during_held_recovery_survives_restart_and_repairs_before_sealing()
        .unwrap();
}

#[test]
fn agreement_lookup_finishes_before_the_engine_requests_one_shot_key_packages() {
    context_creation::agreement_lookup_finishes_before_the_engine_requests_one_shot_key_packages();
}

#[test]
fn erase_device_clears_held_markers_atomically_and_fresh_restore_can_recover_the_same_space() {
    held_recovery::erase_device_clears_held_markers_atomically_and_fresh_restore_can_recover_the_same_space().unwrap();
}

#[test]
fn legacy_stub_cannot_install_or_invalidate_authenticated_recovery_context() {
    held_recovery::legacy_stub_cannot_install_or_invalidate_authenticated_recovery_context();
}

#[test]
fn context_and_commit_roll_back_together_and_wait_for_upload_after_restart() {
    context_persistence::context_and_commit_roll_back_together_and_wait_for_upload_after_restart()
        .unwrap();
}

#[test]
fn live_direct_recipient_keeps_initial_welcome_before_recovery_context() {
    initial_welcome::live_direct_recipient_keeps_initial_welcome_before_recovery_context();
}

#[test]
fn live_direct_recipient_keeps_initial_welcome_after_cold_reopen() {
    initial_welcome::live_direct_recipient_keeps_initial_welcome_after_cold_reopen();
}

mod context_creation;
mod context_persistence;
mod held_recovery;
mod initial_welcome;
mod processing_failures;
