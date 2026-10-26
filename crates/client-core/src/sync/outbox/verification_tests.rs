//! Work counts and retained-original mutations, independent of machine speed.
use super::*;
use crate::sync::outbox::fault_tests::{original_json, set_json, Fixture};
use rusqlite::types::Value;
use types::{EventBody, Privacy};

fn fixture() -> Result<Fixture, Box<dyn std::error::Error>> {
    let mut f = Fixture::with_privacies(":memory:", Privacy::EndToEnd, Privacy::Closed)?;
    f.engine.create_mls_group("sp_fault_a")?;
    Ok(f)
}

fn checks(f: &Fixture) -> usize {
    f.engine
        .net
        .outbox_verification
        .lock()
        .expect("cache")
        .checks
}

#[test]
fn hundred_originals_are_verified_once_across_repeated_maintenance_and_offers(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut f = fixture()?;
    for n in 0..100 {
        f.message("sp_fault_a", n);
    }
    assert!(f.engine.mls_seal_outbox()?);
    assert_eq!(checks(&f), 100);
    for _ in 0..10 {
        assert!(!f.engine.mls_seal_outbox()?);
        assert_eq!(f.engine.outbox_envelopes().len(), 5);
    }
    assert_eq!(checks(&f), 100, "unchanged originals repeated author work");
    f.engine.reload()?;
    assert!(!f.engine.mls_seal_outbox()?);
    assert_eq!(checks(&f), 200, "reload must retire verification receipts");
    Ok(())
}

#[test]
fn every_changed_view_and_proof_is_rechecked_after_a_warm_receipt(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut f = fixture()?;
    let event = f.message("sp_fault_a", 1);
    let original = original_json(&f.engine, &event);
    let head = f.engine.store.outbox_heads()?.remove(0);
    let mut mutations = Vec::new();
    let mut changed = event.clone();
    changed.sig.clear();
    mutations.push(changed);
    let mut changed = event.clone();
    changed.cert = Some(String::new());
    mutations.push(changed);
    let mut changed = event.clone();
    changed.content.clear();
    mutations.push(changed);
    let mut changed = event.clone();
    changed.at_ms += 1;
    mutations.push(changed);
    let mut changed = event.clone();
    changed.device = None;
    mutations.push(changed);
    let mut changed = event.clone();
    if let EventBody::MessagePosted { text, .. } = &mut changed.body {
        text.push_str(" forged");
    }
    mutations.push(changed);
    for changed in mutations {
        set_json(&f.engine, &event, Value::Text(original.clone()));
        assert_eq!(
            f.engine
                .read_outbox_head(&head)
                .expect("authentic original"),
            event
        );
        let before = f
            .engine
            .net
            .outbox_verification
            .lock()
            .expect("cache")
            .checks;
        set_json(
            &f.engine,
            &event,
            Value::Text(serde_json::to_string(&changed)?),
        );
        assert!(f.engine.read_outbox_head(&head).is_err());
        assert_eq!(
            f.engine
                .net
                .outbox_verification
                .lock()
                .expect("cache")
                .checks,
            before + 1
        );
    }
    set_json(&f.engine, &event, Value::Text(original.clone()));
    assert_eq!(
        f.engine
            .read_outbox_head(&head)
            .expect("authentic original"),
        event
    );
    let before = f
        .engine
        .net
        .outbox_verification
        .lock()
        .expect("cache")
        .checks;
    let mut changed = event.clone();
    changed.seq += 1; // Not signed, but still a different complete retained Event.
    set_json(
        &f.engine,
        &event,
        Value::Text(serde_json::to_string(&changed)?),
    );
    assert_eq!(
        f.engine
            .read_outbox_head(&head)
            .expect("unsigned sequence is not author authority"),
        changed
    );
    assert_eq!(
        f.engine
            .net
            .outbox_verification
            .lock()
            .expect("cache")
            .checks,
        before + 1
    );
    let mut wrong_head = head;
    wrong_head.space = "sp_wrong".into();
    assert!(f.engine.read_outbox_head(&wrong_head).is_err());
    Ok(())
}

#[test]
fn count_byte_eviction_and_oversize_fallback_never_reuse_an_unverified_receipt(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut f = fixture()?;
    let mut cache = VerificationCache::default();
    let mut originals = Vec::new();
    for n in 0..=MAX_ENTRIES {
        let event = f.message("sp_fault_a", i64::try_from(n)?);
        let json = original_json(&f.engine, &event);
        assert!(cache.verify(&event, json.clone()));
        originals.push((event, json));
    }
    assert_eq!(cache.receipts.len(), MAX_ENTRIES);
    let before = cache.checks;
    assert!(cache.verify(&originals[0].0, originals[0].1.clone()));
    assert_eq!(cache.checks, before + 1, "evicted originals reverify");
    cache.clear();
    for (event, json) in originals.iter().take(10) {
        let mut allocated = json.clone();
        allocated.reserve_exact(MAX_BYTES / 4);
        assert!(cache.verify(event, allocated));
        assert!(cache.bytes <= MAX_BYTES);
        assert!(cache.receipts.len() <= MAX_ENTRIES);
    }
    assert!(cache.receipts.len() < 10, "byte budget must evict");
    cache.clear();
    for _ in 0..2 {
        let mut oversized = originals[0].1.clone();
        oversized.reserve_exact(MAX_BYTES + 1);
        let before = cache.checks;
        assert!(cache.verify(&originals[0].0, oversized));
        assert_eq!(cache.checks, before + 1);
        assert!(cache.receipts.is_empty());
        assert_eq!(cache.bytes, 0);
    }
    let mut damaged = originals[0].0.clone();
    damaged.sig.clear();
    let mut oversized = serde_json::to_string(&damaged)?;
    oversized.reserve_exact(MAX_BYTES + 1);
    assert!(!cache.verify(&damaged, oversized));
    assert!(
        cache.receipts.is_empty(),
        "oversize fallback cannot bypass proof"
    );
    Ok(())
}
