//! Distinct real leaves: a worker's ready group cannot attest a cold capsule's join.
use super::*;
use crate::owner_policy::OwnerPolicyContextRejection;
use std::collections::BTreeSet;

fn history(owner: &Engine) -> StdResult<Vec<Sequenced>, Box<dyn std::error::Error>> {
    Ok(owner
        .logs
        .get(SPACE)
        .ok_or("canonical owner log")?
        .events()
        .iter()
        .map(|event| Sequenced {
            seq: event.seq,
            prev: event.prev.clone(),
            hash: event.hash.clone(),
            env: Envelope::plain(event),
        })
        .collect())
}

fn ingest_at(core: &mut DeviceCore, entries: Vec<Sequenced>, phase: &str) -> TestResult {
    core.ingest(entries)
        .map(|_| ())
        .map_err(|error| format!("{phase}: {error:?}").into())
}

fn deliver_handshakes(owner: &mut Engine, worker: &mut DeviceCore, phase: &str) -> TestResult {
    for (sha, _) in owner.pending_uploads(100) {
        owner.upload_done(&sha);
    }
    let mut delivered = 0;
    while owner.outbox_len() != 0 {
        let entries = owner.outbox_envelopes();
        assert!(!entries.is_empty());
        for envelope in entries {
            let entry = ordered(owner, envelope);
            assert_eq!(owner.ingest(entry.clone()), Ingest::Confirmed);
            ingest_at(worker, vec![entry], phase)?;
            delivered += 1;
            assert!(delivered < 12, "normal handshake did not settle");
        }
    }
    Ok(())
}

#[test]
fn cold_capsule_with_pruned_welcome_needs_its_own_fresh_welcome_not_worker_readiness() -> TestResult
{
    let mut before_welcome = None;
    let mut f = Fixture::new_observing_welcome(SPACE, |core| {
        before_welcome = Some(core.image().expect("retained native private package"));
    });
    let early = before_welcome.ok_or("pre-Welcome native image")?;
    let worker_key = Signer::generate();
    assert_ne!(worker_key.id(), f.agent_device.id());
    let worker_author = Author::device(&f.agent_root, worker_key.clone());
    let worker_cert = worker_author.cert.ok_or("worker certificate")?;
    let worker_credential = || {
        UnlockedDevice::verify(
            f.agent_profile.clone(),
            f.owner_profile.clone(),
            worker_cert.clone(),
            worker_key.secret(),
        )
    };
    let mut worker = DeviceCore::bootstrap(worker_credential()?, "http://relay.test")?;
    let (packages, _) = worker.prepare_initial_key_packages()?;
    f.owner
        .commit_now(SPACE, &packages.packages()[..1], &BTreeSet::new())?;
    // Feed the exact canonical prefix before its add; no imported group state.
    let initial = history(&f.owner)?;
    ingest_at(&mut worker, initial, "worker original prefix")?;
    deliver_handshakes(&mut f.owner, &mut worker, "worker own Welcome")?;
    assert!(worker.engine.ready_epoch(SPACE).is_some());
    assert!(worker
        .engine
        .mls_leaves(SPACE)
        .ok_or("worker leaves")?
        .contains(&(f.agent_profile.id.clone(), f.agent_device.id())));
    f.owner.set_registered(true)?;
    worker.engine.set_registered(true)?;

    // The original native leaf was included by the initial MemberAdded claim;
    // it has not posted its own checkpoint or DeviceJoining retention hold.
    assert!(!history(&f.owner)?.iter().any(|entry| matches!(
        entry.env.body(), Some(EventBody::DeviceJoining { device }) if device == f.agent_device.id()
    )));
    assert!(f.owner.mls_checkpoints()?);
    deliver_handshakes(&mut f.owner, &mut worker, "owner checkpoint")?;
    assert!(worker.engine.mls_checkpoints()?);
    for envelope in worker.engine.outbox_envelopes() {
        let entry = ordered(&f.owner, envelope);
        assert_eq!(f.owner.ingest(entry.clone()), Ingest::Applied);
        ingest_at(&mut worker, vec![entry], "worker checkpoint echo")?;
    }
    let checkpoint_floor = history(&f.owner)?
        .iter()
        .filter_map(|entry| match entry.env.body() {
            Some(EventBody::Checkpoint { upto, .. }) => Some(upto.seq),
            _ => None,
        })
        .min()
        .ok_or("actual warm-device checkpoints")?;
    let account = f.owner.account().ok_or("owner account")?;
    let scope = OwnerPolicyScope {
        expected_identity_id: account.identity.clone(),
        expected_device_id: account.device.clone(),
        space_id: SPACE.into(),
        agent_id: f.agent_profile.id.clone(),
    };
    let pending = f.owner.request_account_policy_options(&scope)?;
    f.owner.mls_seal_outbox()?;
    deliver_handshakes(&mut f.owner, &mut worker, "typed original inquiry")?;
    let query = worker.engine.owner_policy_inquiry(SPACE, &pending.source)?;
    let complete = history(&f.owner)?;

    // Same credential/private KP and canonical chain, changing only the allowed
    // ciphertext retention cut. Full history proves this capsule could join.
    let mut full = DeviceCore::restore(early, f.credential())?;
    let before = full.image()?;
    ingest_at(&mut full, complete.clone(), "full native control prefix")?;
    full.owner_policy_account_context(query.signed(), crate::engine::now_ms())?;
    let mut cold = DeviceCore::restore(before, f.credential())?;
    let mut pruned_welcomes = 0;
    let pruned: Vec<_> = complete
        .into_iter()
        .map(|mut entry| {
            if entry.seq < checkpoint_floor {
                if let Some(stub) = entry.env.pruned() {
                    assert_eq!(stub.wire_hash(), entry.env.wire_hash());
                    assert!(stub.verify().is_ok());
                    if entry.env.sealed_kind() == Some(SealedKind::Welcome) {
                        pruned_welcomes += 1;
                    }
                    entry.env = stub;
                }
            }
            entry
        })
        .collect();
    assert!(pruned_welcomes > 0);
    ingest_at(&mut cold, pruned, "pruned native source prefix")?;
    assert_eq!(cold.retained_head(SPACE)?, full.retained_head(SPACE)?);
    assert!(worker.engine.ready_epoch(SPACE).is_some());
    assert_eq!(
        cold.owner_policy_account_admission(query.signed(), crate::engine::now_ms()),
        Err(OwnerPolicyContextRejection::MlsGroup)
    );
    cold = DeviceCore::restore(cold.image()?, f.credential())?;
    assert_eq!(
        cold.owner_policy_account_admission(query.signed(), crate::engine::now_ms()),
        Err(OwnerPolicyContextRejection::MlsGroup)
    );

    // A normal signed sibling rejoin and fresh genuine package recover this
    // capsule. Neither the worker's group nor an approval is copied into it.
    worker.engine.append(
        SPACE,
        &f.agent_profile.id,
        EventBody::DeviceJoining {
            device: f.agent_device.id(),
        },
    )?;
    for envelope in worker.engine.outbox_envelopes() {
        let entry = ordered(&f.owner, envelope);
        assert_eq!(f.owner.ingest(entry.clone()), Ingest::Applied);
        ingest_at(&mut worker, vec![entry.clone()], "worker rejoin echo")?;
        ingest_at(&mut cold, vec![entry], "cold native rejoin event")?;
    }
    let (fresh, _) = cold.prepare_initial_key_packages()?;
    let clock = KeyPackageClaimClock::from_server_ms(crate::engine::now_ms())
        .ok_or("actual claim clock")?;
    let (_, targets, operation) = f
        .owner
        .mls_to_claim(Some(clock))?
        .ok_or("normal native rejoin claim")?;
    assert_eq!(targets, vec![f.agent_profile.id.clone()]);
    f.owner.mls_claimed(
        SPACE,
        &operation,
        Ok(vec![KeyPackageRecord {
            identity: f.agent_profile.id.clone(),
            device: f.agent_device.id(),
            data: fresh.packages()[0].clone(),
        }]),
    );
    let after = cold.retained_head(SPACE)?.ok_or("retained source")?.seq;
    deliver_handshakes(
        &mut f.owner,
        &mut worker,
        "fresh rejoin handshake on worker",
    )?;
    ingest_at(
        &mut cold,
        history(&f.owner)?
            .into_iter()
            .filter(|entry| entry.seq > after)
            .collect(),
        "fresh rejoin handshake on native",
    )?;
    cold.owner_policy_account_context(query.signed(), crate::engine::now_ms())?;
    cold = DeviceCore::restore(cold.image()?, f.credential())?;
    cold.owner_policy_account_context(query.signed(), crate::engine::now_ms())?;
    assert!(cold.engine.store.outbox()?.is_empty());
    assert!(cold.engine.state.grants.is_empty());
    assert!(cold.engine.state.requests.is_empty());
    Ok(())
}
