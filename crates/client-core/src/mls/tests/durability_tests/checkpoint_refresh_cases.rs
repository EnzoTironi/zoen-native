//! A recent checkpoint must not consume a cold inspection's budget reloading MLS.
//! The probe delays the real load; it does not mock group membership or readiness.
use super::*;
use crate::mls::checkpoints::refresh_probe::Probe;
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn fresh_checkpoint_after_cold_open_does_not_reload_group_or_change_work() -> TestResult {
    let database = Database::new();
    let space = "fresh-checkpoint-cold-budget";
    let (mut owner, secret, mut peer, _peer_secret) = encrypted_pair(&database, space);
    let message = owner.send_message(space, "Retain the cold checkpoint ciphertext")?;
    assert!(owner.mls_seal_outbox()?);
    let before_intent = serde_json::to_value(owner.store.outbox_get(&message.id)?)?;
    let pending = owner
        .store
        .outbox_get(&message.id)?
        .ok_or("missing pending message")?;
    let ciphertext = owner
        .sealed_copy(&pending)
        .ok_or("missing original ciphertext")?;
    let timestamp = crate::engine::now_ms().to_string();
    owner
        .store
        .set_meta(&checkpointed_ms_meta(space), &timestamp)?;
    let before_provider = provider_rows(&owner);
    let before_events = owner.store.event_count()?;
    drop(owner);
    let mut owner = reopen(&database.path("owner.sqlite"), &secret);
    let before_due = owner.net.mls.checkpoint_due.clone();
    let before_dirty = owner.net.mls.dirty.clone();
    let before_retry = owner.net.mls.retry_at.clone();
    let before_recovery = owner.net.mls.has_recovery_work();
    let probe = Probe::start(Duration::from_millis(100));
    owner.mls_refresh_checkpoints();
    assert_eq!(
        probe.reads(),
        0,
        "a still-fresh checkpoint must not block on the real group load"
    );
    assert_eq!(
        owner.store.meta(&checkpointed_ms_meta(space))?,
        Some(timestamp)
    );
    assert_eq!(provider_rows(&owner), before_provider);
    assert_eq!(owner.store.event_count()?, before_events);
    assert_eq!(
        serde_json::to_value(owner.store.outbox_get(&message.id)?)?,
        before_intent
    );
    let retained = owner.sealed_copy(&pending).ok_or("lost cold ciphertext")?;
    assert_eq!(retained.0, ciphertext.0);
    assert_eq!(retained.1.content(), ciphertext.1.content());
    assert_eq!(retained.1.sig, ciphertext.1.sig);
    assert_eq!(owner.net.mls.checkpoint_due, before_due);
    assert_eq!(owner.net.mls.dirty, before_dirty);
    assert_eq!(owner.net.mls.retry_at, before_retry);
    assert_eq!(owner.net.mls.has_recovery_work(), before_recovery);
    drop(probe);
    relay_ready_to_peer(&mut owner, &mut peer);
    assert_eq!(
        peer.timeline(space)?
            .iter()
            .filter(|entry| entry.id == message.id)
            .count(),
        1
    );
    assert!(owner.store.outbox_get(&message.id)?.is_none());
    Ok(())
}

#[test]
fn due_and_unknown_checkpoints_still_require_the_actual_group() -> TestResult {
    let database = Database::new();
    let space = "checkpoint-needs-real-group";
    let (mut owner, _secret, _peer, _peer_secret) = encrypted_pair(&database, space);
    let key = checkpointed_ms_meta(space);
    let before_provider = provider_rows(&owner);
    let before_events = owner.store.event_count()?;
    let expired = crate::engine::now_ms() - checkpoint_refresh_ms() - 1;
    owner.net.mls.checkpoint_due.remove(space);
    owner.store.set_meta(&key, &expired.to_string())?;
    {
        let probe = Probe::start(Duration::from_millis(10));
        owner.mls_refresh_checkpoints();
        assert_eq!(probe.reads(), 1);
        assert!(owner.net.mls.checkpoint_due.contains(space));
        assert_eq!(owner.store.meta(&key)?, Some(expired.to_string()));
    }
    for previous in [None, Some("malformed")] {
        owner.net.mls.checkpoint_due.remove(space);
        if let Some(previous) = previous {
            owner.store.set_meta(&key, previous)?;
        } else {
            owner.store.meta_delete(&key)?;
        }
        let probe = Probe::start(Duration::from_millis(10));
        owner.mls_refresh_checkpoints();
        assert_eq!(probe.reads(), 1);
        assert!(!owner.net.mls.checkpoint_due.contains(space));
        let started: i64 = owner
            .store
            .meta(&key)?
            .ok_or("missing checkpoint clock")?
            .parse()?;
        assert!(started >= expired);
    }
    owner.device()?.forget(space)?;
    owner.net.mls.checkpoint_due.remove(space);
    owner.store.set_meta(&key, &expired.to_string())?;
    {
        let probe = Probe::start(Duration::from_millis(10));
        owner.mls_refresh_checkpoints();
        assert_eq!(probe.reads(), 1);
        assert!(
            !owner.net.mls.checkpoint_due.contains(space),
            "a due timestamp cannot manufacture a missing group"
        );
        assert_eq!(owner.store.meta(&key)?, Some(expired.to_string()));
    }
    assert_eq!(owner.store.event_count()?, before_events);
    assert_ne!(
        provider_rows(&owner),
        before_provider,
        "the test genuinely forgot the real MLS group"
    );
    Ok(())
}
