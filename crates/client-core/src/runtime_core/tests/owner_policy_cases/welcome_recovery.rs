//! Real owner controls/group leaves; injected faults affect only this local test DB.
use super::*;
use crate::owner_policy::OwnerPolicyInquiryControl;
use crate::Client;
use std::collections::BTreeSet;

fn candidate() -> StdResult<(Fixture, String, OwnerPolicyInquiryControl), Box<dyn std::error::Error>>
{
    let (mut f, scope, _) = setup()?;
    let key = Signer::generate();
    let author = Author::device(&f.agent_root, key.clone());
    let credential = UnlockedDevice::verify(
        f.agent_profile.clone(),
        f.owner_profile.clone(),
        author.cert.ok_or("certified secondary leaf")?,
        key.secret(),
    )?;
    let mut secondary = DeviceCore::bootstrap(credential, "http://relay.test")?;
    let (packages, _) = secondary.prepare_initial_key_packages()?;
    f.owner
        .commit_now(SPACE, &packages.packages()[..1], &BTreeSet::new())?;
    retained_welcome::deliver_handshakes(&mut f.owner, &mut f.core, "secondary real leaf")?;
    f.core.engine.devices_arrived(vec![
        protocol::DeviceRecord {
            device: f.agent_device.id(),
            revoked: false,
        },
        protocol::DeviceRecord {
            device: key.id(),
            revoked: false,
        },
    ])?;
    assert!(f.core.engine.store.outbox()?.is_empty());
    let pending = f.owner.request_account_policy_options(&scope)?;
    deliver_owner(&mut f)?;
    let query = f.core.engine.owner_policy_inquiry(SPACE, &pending.source)?;
    Ok((f, key.id(), query))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn client_capture_requires_the_running_worker_device_on_the_real_current_channel(
) -> TestResult {
    let (f, target, query) = candidate()?;
    let relay = crate::link_api::directory_test_relay(target).await;
    let client = Client::open(":memory:".into(), "en".into())?;
    let current_device = f.agent_device.id();
    {
        let mut engine = client.lock();
        *engine = f.core.engine;
        let account = engine
            .net
            .account
            .as_mut()
            .ok_or("synthetic agent account")?;
        account.relay_url = relay.url.clone();
        let account = serde_json::to_string(account)?;
        engine.store.set_meta("account", &account)?;
    }
    crate::link_api::directory_test_online(&client).await;
    // Settle the normal checkpoint opportunity before observing capture. It is
    // the existing Net lifecycle, not part of a recovery/control publication.
    client.lock().mls_checkpoints()?;
    let before = client.lock().outbox_envelopes();
    let current = client.owner_policy_welcome_attempt(&query, &current_device);
    let old = client.owner_policy_welcome_attempt(&query, &"cc".repeat(32));
    let after = client.lock().outbox_envelopes();
    client.stop_sync();
    assert!(
        current.is_ok(),
        "actual current authenticated worker capture"
    );
    assert!(
        old.is_err(),
        "same agent cannot borrow a replacement device before capture"
    );
    assert_eq!(
        after, before,
        "capture does not enqueue any control or joining"
    );
    Ok(())
}

#[test]
fn missing_corrupt_incomplete_or_revoked_directory_never_requests_a_native_rotation() -> TestResult
{
    for fault in [
        "missing",
        "corrupt",
        "overlap",
        "duplicate",
        "self_revoked",
        "target_revoked",
    ] {
        let (mut f, target, query) = candidate()?;
        let attempt = f.core.engine.owner_policy_welcome_attempt(&query)?;
        let own = f.agent_device.id();
        match fault {
            "missing" => f.core.engine.store.meta_delete("devices.active")?,
            "corrupt" => f.core.engine.store.set_meta("devices.revoked", "invalid")?,
            "overlap" => f.core.engine.store.set_meta(
                "devices.revoked",
                &serde_json::to_string(&vec![target.clone()])?,
            )?,
            "duplicate" => f.core.engine.store.set_meta(
                "devices.active",
                &serde_json::to_string(&vec![own.clone(), target.clone(), target.clone()])?,
            )?,
            "self_revoked" => {
                f.core.engine.store.set_meta(
                    "devices.active",
                    &serde_json::to_string(&vec![target.clone()])?,
                )?;
                f.core
                    .engine
                    .store
                    .set_meta("devices.revoked", &serde_json::to_string(&vec![own])?)?;
            }
            "target_revoked" => {
                f.core
                    .engine
                    .store
                    .set_meta("devices.active", &serde_json::to_string(&vec![own])?)?;
                f.core.engine.store.set_meta(
                    "devices.revoked",
                    &serde_json::to_string(&vec![target.clone()])?,
                )?;
            }
            _ => unreachable!("fixed fault inventory"),
        }
        assert!(
            f.core
                .engine
                .request_owner_policy_welcome(&query, &attempt, &target)
                .is_err(),
            "{fault}"
        );
        assert!(f.core.engine.store.outbox()?.is_empty());
        assert!(f
            .core
            .engine
            .store
            .meta(&format!("link.asked:{SPACE}:{target}"))?
            .is_none());
    }
    Ok(())
}

#[test]
fn self_foreign_actor_change_and_owner_write_removal_retire_the_callback() -> TestResult {
    for fault in ["self", "foreign", "device", "certificate", "owner_reader"] {
        let (mut f, target, query) = candidate()?;
        let attempt = f.core.engine.owner_policy_welcome_attempt(&query)?;
        let requested = match fault {
            "self" => f.agent_device.id(),
            "foreign" => f.owner_profile.id.clone(),
            "device" => {
                f.core.engine.net.account.as_mut().ok_or("account")?.device = "aa".repeat(32);
                target.clone()
            }
            "certificate" => {
                f.core.engine.net.account.as_mut().ok_or("account")?.cert = "00".repeat(64);
                target.clone()
            }
            "owner_reader" => {
                f.core
                    .engine
                    .state
                    .spaces
                    .get_mut(SPACE)
                    .ok_or("space")?
                    .members
                    .iter_mut()
                    .find(|(id, _)| id == &f.owner_profile.id)
                    .ok_or("owner")?
                    .1 = Role::Reader;
                target.clone()
            }
            _ => unreachable!("fixed fault inventory"),
        };
        assert!(
            f.core
                .engine
                .request_owner_policy_welcome(&query, &attempt, &requested)
                .is_err(),
            "{fault}"
        );
        assert!(f.core.engine.store.outbox()?.is_empty());
    }
    Ok(())
}

#[test]
fn exact_expiration_boundary_and_already_reported_original_stay_closed() -> TestResult {
    let (mut f, target, query) = candidate()?;
    let attempt = f.core.engine.owner_policy_welcome_attempt(&query)?;
    let edge = query.signed().inquiry.at_ms + types::owner_policy_context::CONTEXT_TTL_MS;
    let before = f.core.image()?;
    assert!(f
        .core
        .engine
        .request_owner_policy_welcome_at(&query, &attempt, &target, edge + 1)
        .is_err());
    assert_eq!(before.as_bytes(), f.core.image()?.as_bytes());
    assert!(f
        .core
        .engine
        .request_owner_policy_welcome_at(&query, &attempt, &target, edge)?);

    let (mut f, target, query) = candidate()?;
    let attempt = f.core.engine.owner_policy_welcome_attempt(&query)?;
    f.core.engine.refuse_owner_policy_options(&query)?;
    let before = f.core.image()?;
    assert!(!f
        .core
        .engine
        .request_owner_policy_welcome(&query, &attempt, &target)?);
    assert_eq!(
        before.as_bytes(),
        f.core.image()?.as_bytes(),
        "late hint cannot overwrite real terminal report"
    );
    Ok(())
}

#[test]
fn producer_and_initialized_marker_failure_roll_back_join_and_projection() -> TestResult {
    let (mut f, target, query) = candidate()?;
    let attempt = f.core.engine.owner_policy_welcome_attempt(&query)?;
    f.core.engine.store.conn().execute_batch("CREATE TRIGGER fail_welcome_marker BEFORE INSERT ON meta WHEN NEW.key LIKE 'link.asked:%:owner-policy-initialized' BEGIN SELECT RAISE(ABORT, 'injected rejoin marker failure'); END;")?;
    assert!(f
        .core
        .engine
        .request_owner_policy_welcome(&query, &attempt, &target)
        .is_err());
    assert!(f.core.engine.store.outbox()?.is_empty());
    assert!(f
        .core
        .engine
        .store
        .meta(&format!("link.asked:{SPACE}:{target}"))?
        .is_none());
    f.core
        .engine
        .store
        .conn()
        .execute_batch("DROP TRIGGER fail_welcome_marker")?;
    let attempt = f.core.engine.owner_policy_welcome_attempt(&query)?;
    assert!(f
        .core
        .engine
        .request_owner_policy_welcome(&query, &attempt, &target)?);
    assert_eq!(f.core.engine.store.outbox()?.len(), 1);
    Ok(())
}

#[test]
fn missing_corrupt_or_legacy_association_never_mints_a_replacement_original() -> TestResult {
    for fault in ["missing", "corrupt", "legacy", "missing_initialized"] {
        let (mut f, target, query) = candidate()?;
        let attempt = f.core.engine.owner_policy_welcome_attempt(&query)?;
        assert!(f
            .core
            .engine
            .request_owner_policy_welcome(&query, &attempt, &target)?);
        let key = format!("link.asked:{SPACE}:{target}");
        match fault {
            "missing" => f.core.engine.store.meta_delete(&key)?,
            "corrupt" => f.core.engine.store.set_meta(&key, "{}")?,
            "legacy" => f.core.engine.store.set_meta(&key, "1")?,
            "missing_initialized" => f
                .core
                .engine
                .store
                .meta_delete(&format!("{key}:owner-policy-initialized"))?,
            _ => unreachable!("fixed fault inventory"),
        }
        let original = f.core.engine.outbox_envelopes();
        assert!(
            f.core
                .engine
                .request_owner_policy_welcome(&query, &attempt, &target)
                .is_err(),
            "{fault}"
        );
        assert_eq!(f.core.engine.outbox_envelopes(), original);
    }
    Ok(())
}

#[test]
fn failed_accepted_reference_write_keeps_original_outbox_and_exact_echo_retryable() -> TestResult {
    let (mut f, target, query) = candidate()?;
    let attempt = f.core.engine.owner_policy_welcome_attempt(&query)?;
    assert!(f
        .core
        .engine
        .request_owner_policy_welcome(&query, &attempt, &target)?);
    let envelope = f
        .core
        .engine
        .outbox_envelopes()
        .into_iter()
        .next()
        .ok_or("original joining")?;
    let frame = ordered(&f.owner, envelope.clone());
    assert_eq!(f.owner.ingest(frame.clone()), Ingest::Applied);
    f.core.engine.store.conn().execute_batch("CREATE TRIGGER fail_welcome_receipt BEFORE UPDATE ON meta WHEN NEW.key LIKE 'link.asked:%' BEGIN SELECT RAISE(ABORT, 'injected rejoin receipt failure'); END;")?;
    assert!(matches!(
        f.core.engine.ingest(frame.clone()),
        Ingest::Invalid(_)
    ));
    assert_eq!(f.core.engine.outbox_envelopes(), vec![envelope]);
    f.core
        .engine
        .store
        .conn()
        .execute_batch("DROP TRIGGER fail_welcome_receipt")?;
    assert_eq!(f.core.engine.ingest(frame), Ingest::Confirmed);
    assert!(f.core.engine.store.outbox()?.is_empty());
    Ok(())
}
