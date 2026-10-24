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

pub(super) fn deliver_handshakes(
    owner: &mut Engine,
    worker: &mut DeviceCore,
    phase: &str,
) -> TestResult {
    for (sha, _) in owner.pending_uploads(100) {
        owner.upload_done(&sha);
    }
    let mut delivered = 0;
    while owner.outbox_len() != 0 {
        let entries = owner.outbox_envelopes();
        assert!(!entries.is_empty());
        for envelope in entries {
            let entry = ordered(owner, envelope)?;
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
    recovery(false)
}

#[test]
fn typed_runtime_missing_group_hint_recovers_one_original_across_ack_loss_and_cold() -> TestResult {
    recovery(true)
}

fn recovery(typed: bool) -> TestResult {
    let mut before_welcome = None;
    let mut public_stock = None;
    let mut f = Fixture::new_with_key_package(SPACE, |core| {
        let (batch, _) = core
            .prepare_initial_key_packages()
            .expect("retained actual initial batch");
        assert_eq!(batch.packages().len(), 32);
        let first = batch.packages()[0].clone();
        public_stock = Some((batch.packages().to_vec(), batch.digest().to_owned()));
        before_welcome = Some(core.image().expect("retained native private package"));
        first
    })?;
    let early = before_welcome.ok_or("pre-Welcome native image")?;
    let (public_stock, original_digest) = public_stock.ok_or("initial public batch")?;
    let worker_key = Signer::generate()?;
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
        let entry = ordered(&f.owner, envelope)?;
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
    let inquiry_wire = f
        .owner
        .outbox_envelopes()
        .into_iter()
        .find(|envelope| envelope.client_id() == pending.source)
        .ok_or("original sealed inquiry wire")?;
    assert!(inquiry_wire.is_sealed());
    deliver_handshakes(&mut f.owner, &mut worker, "typed original inquiry")?;
    let query = worker.engine.owner_policy_inquiry(SPACE, &pending.source)?;
    let mut complete = history(&f.owner)?;
    // The owner's readable log retains its signed inner event. Reuse the actual
    // pre-send ciphertext for a source consumer, never Envelope::plain(inner).
    let inquiry = complete
        .iter_mut()
        .find(|entry| entry.env.client_id() == pending.source)
        .ok_or("canonical inquiry position")?;
    inquiry.env = inquiry_wire;

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
    let mut late_attempt = None;
    let mut second_query = None;
    if typed {
        worker.engine.devices_arrived(vec![
            protocol::DeviceRecord {
                device: worker_key.id(),
                revoked: false,
            },
            protocol::DeviceRecord {
                device: f.agent_device.id(),
                revoked: false,
            },
        ])?;
        let other = f.owner.request_account_policy_options(&scope)?;
        f.owner.mls_seal_outbox()?;
        for envelope in f.owner.outbox_envelopes() {
            let entry = ordered(&f.owner, envelope)?;
            assert_eq!(f.owner.ingest(entry.clone()), Ingest::Confirmed);
            ingest_at(
                &mut worker,
                vec![entry.clone()],
                "second original on worker",
            )?;
            ingest_at(&mut cold, vec![entry], "second original on cold native")?;
        }
        let other = worker.engine.owner_policy_inquiry(SPACE, &other.source)?;
        let attempt = worker.engine.owner_policy_welcome_attempt(&query)?;
        late_attempt = Some(worker.engine.owner_policy_welcome_attempt(&other)?);
        assert!(worker.engine.request_owner_policy_welcome(
            &query,
            &attempt,
            &f.agent_device.id()
        )?);
        assert!(!worker.engine.request_owner_policy_welcome(
            &query,
            &attempt,
            &f.agent_device.id()
        )?);
        let original = worker.engine.outbox_envelopes();
        assert_eq!(original.len(), 1);
        worker = DeviceCore::restore(worker.image()?, worker_credential()?)?;
        worker.engine.set_registered(true)?;
        let retry = worker.engine.owner_policy_welcome_attempt(&other)?;
        assert!(!worker.engine.request_owner_policy_welcome(
            &other,
            &retry,
            &f.agent_device.id()
        )?);
        assert_eq!(
            worker.engine.outbox_envelopes(),
            original,
            "one retained signed joining after cold/ACK loss"
        );
        second_query = Some(other);
    } else {
        worker.engine.append(
            SPACE,
            &f.agent_profile.id,
            EventBody::DeviceJoining {
                device: f.agent_device.id(),
            },
        )?;
    }
    for envelope in worker.engine.outbox_envelopes() {
        let entry = ordered(&f.owner, envelope)?;
        assert_eq!(f.owner.ingest(entry.clone()), Ingest::Applied);
        ingest_at(&mut worker, vec![entry.clone()], "worker rejoin echo")?;
        ingest_at(&mut cold, vec![entry], "cold native rejoin event")?;
    }
    let retained = cold
        .initial_key_package_publication()?
        .ok_or("retained initial publication")?;
    assert_eq!(retained.digest(), original_digest);
    assert_eq!(retained.packages(), public_stock);
    // Use a different, originally retained unused package. Initialization's
    // acknowledged fixed batch is not a refill mechanism after consumption.
    assert_ne!(public_stock[0], public_stock[1]);
    let clock = KeyPackageClaimClock::from_server_ms(crate::engine::now_ms())
        .ok_or("actual claim clock")?;
    // Normal rejoin is two ordered changes: first remove the stale leaf,
    // confirm that commit, then claim a fresh package for its replacement.
    assert!(f.owner.mls_to_claim(Some(clock))?.is_none());
    assert!(f.owner.store.outbox()?.iter().any(|pending| matches!(
        &pending.event.body,
        EventBody::Sealed { kind } if kind == SealedKind::Commit.name()
    )));
    let before_removal = cold.retained_head(SPACE)?.ok_or("retained rejoin")?.seq;
    deliver_handshakes(&mut f.owner, &mut worker, "normal stale-leaf removal")?;
    assert!(!f
        .owner
        .mls_leaves(SPACE)
        .ok_or("owner leaves after stale removal")?
        .contains(&(f.agent_profile.id.clone(), f.agent_device.id())));
    ingest_at(
        &mut cold,
        history(&f.owner)?
            .into_iter()
            .filter(|entry| entry.seq > before_removal)
            .collect(),
        "cold native stale-leaf removal",
    )?;
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
            data: public_stock[1].clone(),
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
    if let (Some(other), Some(late)) = (second_query.as_ref(), late_attempt.as_ref()) {
        assert!(
            worker
                .engine
                .request_owner_policy_welcome(other, late, &f.agent_device.id())
                .is_err(),
            "retired epoch cannot borrow the fresh leaf"
        );
        let fresh = worker.engine.owner_policy_welcome_attempt(other)?;
        assert!(
            !worker
                .engine
                .request_owner_policy_welcome(other, &fresh, &f.agent_device.id())?,
            "another old inquiry cannot rotate the recovered leaf"
        );
        worker = DeviceCore::restore(worker.image()?, worker_credential()?)?;
        worker.engine.set_registered(true)?;
        let fresh = worker.engine.owner_policy_welcome_attempt(other)?;
        assert!(
            !worker
                .engine
                .request_owner_policy_welcome(other, &fresh, &f.agent_device.id())?,
            "accepted association survives cold restore"
        );
        assert!(worker.engine.store.outbox()?.is_empty());
    }
    cold = DeviceCore::restore(cold.image()?, f.credential())?;
    cold.owner_policy_account_context(query.signed(), crate::engine::now_ms())?;
    assert!(cold.engine.store.outbox()?.is_empty());
    assert!(cold.engine.state.grants.is_empty());
    assert!(cold.engine.state.requests.is_empty());
    Ok(())
}
