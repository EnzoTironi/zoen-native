//! Unreadable jobs cannot advance MLS before the publication fence gets a turn.
use super::fault_tests::{original_json, raw_queue, set_json, Fixture, OwnedDatabase};
use super::*;
use rusqlite::types::Value;

type ProviderRow = (Vec<u8>, String, Vec<u8>, i64);
fn provider_rows(engine: &Engine) -> Vec<ProviderRow> {
    let mut query = engine.store.conn().prepare(
        "SELECT group_id,data_type,group_data,provider_version FROM openmls_group_data ORDER BY group_id,data_type"
    ).unwrap();
    let rows = query
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap();
    rows.collect::<rusqlite::Result<_>>().unwrap()
}

fn exercise_fault(fault: &str) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let database = OwnedDatabase(std::env::temp_dir().join(types::new_id("outbox-seal-repair")?));
    let mut f = Fixture::with_privacies(
        database.0.to_str().unwrap(),
        Privacy::EndToEnd,
        Privacy::EndToEnd,
    )?;
    f.engine.create_mls_group("sp_fault_a").unwrap();
    let original_a = provider_rows(&f.engine);
    assert!(!original_a.is_empty());
    let a_ids: HashSet<_> = original_a.iter().map(|r| r.0.clone()).collect();
    f.engine.create_mls_group("sp_fault_b").unwrap();
    assert!(f.engine.ready_epoch("sp_fault_a").is_some());
    assert!(f.engine.ready_epoch("sp_fault_b").is_some());
    let first = f.message("sp_fault_a", 0);
    let second = f.message("sp_fault_a", 1);
    let independent = f.message("sp_fault_b", 2);
    let original = original_json(&f.engine, &first);
    let original_queue = raw_queue(&f.engine);
    let first_key = format!("mls.sealed:{}", first.client_id);
    let next_key = format!("mls.sealed:{}", second.client_id);
    let other_key = format!("mls.sealed:{}", independent.client_id);
    let head = f.engine.store.outbox_heads()?.remove(0);
    assert_eq!(
        f.engine
            .read_outbox_head(&head)
            .expect("authentic original"),
        first
    );
    let damaged = match fault {
        "json" => "{not JSON".to_string(),
        "signature" => {
            let mut event = first.clone();
            event.sig.clear();
            serde_json::to_string(&event).unwrap()
        }
        "empty-content" => {
            let mut event = first.clone();
            event.content.clear();
            serde_json::to_string(&event).unwrap()
        }
        _ => unreachable!("explicit owned faults"),
    };
    set_json(&f.engine, &first, Value::Text(damaged));
    let broken = raw_queue(&f.engine);
    let sealed = f.engine.mls_seal_outbox();
    let a_after: Vec<_> = provider_rows(&f.engine)
        .into_iter()
        .filter(|r| a_ids.contains(&r.0))
        .collect();
    assert!(
        a_after == original_a,
        "invalid unsealed intent advanced the real sender ratchet: {fault}"
    );
    assert!(
        f.engine.store.meta(&first_key).unwrap().is_none(),
        "{fault}"
    );
    assert!(
        f.engine.store.meta(&next_key).unwrap().is_none(),
        "same-chat successors must not seal beyond an invalid head: {fault}"
    );
    assert!(
        sealed.unwrap(),
        "another properly scoped chat must seal normally: {fault}"
    );
    let other_copy = f
        .engine
        .store
        .meta(&other_key)
        .unwrap()
        .expect("actual independent ciphertext");
    let offered = f.engine.outbox_envelopes();
    assert_eq!(
        offered.iter().map(Envelope::client_id).collect::<Vec<_>>(),
        [independent.client_id.as_str()],
        "{fault}"
    );
    assert_eq!(raw_queue(&f.engine), broken);

    set_json(&f.engine, &first, Value::Text(original));
    assert!(f.engine.mls_seal_outbox().unwrap());
    assert_eq!(f.engine.store.meta(&other_key).unwrap(), Some(other_copy));
    let repaired = f.engine.outbox_envelopes();
    assert_eq!(
        repaired
            .iter()
            .filter(|e| e.space() == "sp_fault_a")
            .map(Envelope::client_id)
            .collect::<Vec<_>>(),
        [first.client_id.as_str(), second.client_id.as_str()]
    );
    assert_eq!(raw_queue(&f.engine), original_queue);
    let provider = provider_rows(&f.engine);
    let device = f.device;
    drop(f);
    let mut cold = Engine::open(database.0.to_str().unwrap()).unwrap();
    assert!(cold.unlock(Some(device.to_vec())).unwrap());
    assert_eq!(raw_queue(&cold), original_queue);
    assert_eq!(cold.outbox_envelopes(), repaired);
    assert!(
        !cold.mls_seal_outbox().unwrap(),
        "cold replay must not seal or advance a generation again"
    );
    assert!(
        provider_rows(&cold) == provider,
        "cold exact replay changed MLS provider state"
    );
    drop(cold);
    drop(database);
    Ok(())
}

#[test]
fn unreadable_unsealed_head_preserves_ratchet_and_other_chat_progress_through_exact_repair(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    exercise_fault("json")?;
    Ok(())
}

#[test]
fn invalid_signed_head_cannot_consume_ratchet_or_ciphertext_before_dispatch_validation(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    for fault in ["signature", "empty-content"] {
        exercise_fault(fault)?;
    }
    Ok(())
}

#[test]
fn corrupt_cached_head_still_fences_unsealed_successor_without_reencrypting_it(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::with_privacies(":memory:", Privacy::EndToEnd, Privacy::EndToEnd)?;
    f.engine.create_mls_group("sp_fault_a").unwrap();
    let first = f.message("sp_fault_a", 0);
    assert!(f.engine.mls_seal_outbox().unwrap());
    let before = provider_rows(&f.engine);
    let retained = f
        .engine
        .store
        .meta(&format!("mls.sealed:{}", first.client_id))
        .unwrap();
    let next = f.message("sp_fault_a", 1);
    let original = original_json(&f.engine, &first);
    set_json(&f.engine, &first, Value::Text("{broken".into()));
    assert!(!f.engine.mls_seal_outbox().unwrap());
    assert!(provider_rows(&f.engine) == before);
    assert!(f
        .engine
        .store
        .meta(&format!("mls.sealed:{}", next.client_id))
        .unwrap()
        .is_none());
    assert_eq!(
        f.engine
            .store
            .meta(&format!("mls.sealed:{}", first.client_id))
            .unwrap(),
        retained
    );
    set_json(&f.engine, &first, Value::Text(original));
    assert!(f.engine.mls_seal_outbox().unwrap());
    assert_eq!(
        f.engine
            .store
            .meta(&format!("mls.sealed:{}", first.client_id))
            .unwrap(),
        retained
    );
    Ok(())
}

#[test]
fn unknown_original_lane_stops_before_crypto_and_exact_association_repair_recovers(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::with_privacies(":memory:", Privacy::EndToEnd, Privacy::EndToEnd)?;
    f.engine.create_mls_group("sp_fault_a").unwrap();
    f.engine.create_mls_group("sp_fault_b").unwrap();
    let first = f.message("sp_fault_a", 0);
    let independent = f.message("sp_fault_b", 1);
    let original = original_json(&f.engine, &first);
    let scope = f.engine.net.pending.remove(&first.client_id).unwrap();
    set_json(&f.engine, &first, Value::Text("{broken".into()));
    let queue = raw_queue(&f.engine);
    let before = provider_rows(&f.engine);
    assert!(matches!(
        f.engine.mls_seal_outbox(),
        Err(CoreError::Storage { .. })
    ));
    assert!(provider_rows(&f.engine) == before);
    assert_eq!(raw_queue(&f.engine), queue);
    assert!(f
        .engine
        .store
        .meta(&format!("mls.sealed:{}", independent.client_id))
        .unwrap()
        .is_none());
    set_json(&f.engine, &first, Value::Text(original));
    f.engine.net.pending.insert(first.client_id, scope);
    assert!(f.engine.mls_seal_outbox().unwrap());
    assert_eq!(f.engine.outbox_envelopes().len(), 2);
    Ok(())
}

#[test]
fn changed_sql_space_cannot_hide_invalid_head_from_precrypto_validation(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::with_privacies(":memory:", Privacy::EndToEnd, Privacy::Closed)?;
    f.engine.create_mls_group("sp_fault_a").unwrap();
    let first = f.message("sp_fault_a", 0);
    let second = f.message("sp_fault_a", 1);
    let before = provider_rows(&f.engine);
    f.engine
        .store
        .conn()
        .execute(
            "UPDATE outbox SET space=?1 WHERE client_id=?2",
            ("sp_fault_b", &first.client_id),
        )
        .unwrap();
    let damaged = raw_queue(&f.engine);
    assert!(matches!(
        f.engine.mls_seal_outbox(),
        Err(CoreError::Storage { .. })
    ));
    assert!(
        provider_rows(&f.engine) == before,
        "untrusted SQL Space hid an unresolved head before crypto"
    );
    assert!(f
        .engine
        .store
        .meta(&format!("mls.sealed:{}", second.client_id))
        .unwrap()
        .is_none());
    assert_eq!(raw_queue(&f.engine), damaged);
    f.engine
        .store
        .conn()
        .execute(
            "UPDATE outbox SET space=?1 WHERE client_id=?2",
            ("sp_fault_a", &first.client_id),
        )
        .unwrap();
    assert!(f.engine.mls_seal_outbox().unwrap());
    assert_eq!(
        f.engine
            .outbox_envelopes()
            .iter()
            .map(Envelope::client_id)
            .collect::<Vec<_>>(),
        [first.client_id.as_str(), second.client_id.as_str()]
    );
    Ok(())
}

#[test]
fn unreadable_cached_ciphertext_fences_successor_until_exact_repair_and_cold_reopen(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let database = OwnedDatabase(std::env::temp_dir().join(types::new_id("outbox-cache-repair")?));
    let mut f = Fixture::with_privacies(
        database.0.to_str().ok_or("database path")?,
        Privacy::EndToEnd,
        Privacy::Closed,
    )?;
    f.engine.create_mls_group("sp_fault_a")?;
    let first = f.message("sp_fault_a", 0);
    assert!(f.engine.mls_seal_outbox()?);
    let key = format!("mls.sealed:{}", first.client_id);
    let original = f.engine.store.meta(&key)?.ok_or("missing sealed copy")?;
    let epoch = original.split(':').next().ok_or("missing epoch")?;
    f.engine
        .store
        .set_meta(&key, &format!("{epoch}:not-hex:signature"))?;
    let second = f.message("sp_fault_a", 1);
    let independent = f.message("sp_fault_b", 2);
    let provider = provider_rows(&f.engine);
    let queue = raw_queue(&f.engine);
    assert!(!f.engine.mls_seal_outbox()?);
    assert!(
        provider_rows(&f.engine) == provider,
        "unreadable cached head advanced sender ratchet"
    );
    assert!(f
        .engine
        .store
        .meta(&format!("mls.sealed:{}", second.client_id))?
        .is_none());
    assert_eq!(
        f.engine
            .outbox_envelopes()
            .iter()
            .map(Envelope::client_id)
            .collect::<Vec<_>>(),
        [independent.client_id.as_str()]
    );
    assert_eq!(raw_queue(&f.engine), queue);
    f.engine.store.set_meta(&key, &original)?;
    assert!(f.engine.mls_seal_outbox()?);
    let repaired = f.engine.outbox_envelopes();
    assert_eq!(
        repaired.iter().map(Envelope::client_id).collect::<Vec<_>>(),
        [
            first.client_id.as_str(),
            second.client_id.as_str(),
            independent.client_id.as_str()
        ]
    );
    let provider = provider_rows(&f.engine);
    // Exercise publication independently once the successor already has a copy.
    f.engine
        .store
        .set_meta(&key, &format!("{epoch}:not-hex:signature"))?;
    assert_eq!(
        f.engine
            .outbox_envelopes()
            .iter()
            .map(Envelope::client_id)
            .collect::<Vec<_>>(),
        [independent.client_id.as_str()]
    );
    assert!(provider_rows(&f.engine) == provider);
    f.engine.store.set_meta(&key, &original)?;
    let device = f.device;
    drop(f);
    let mut cold = Engine::open(database.0.to_str().ok_or("database path")?)?;
    assert!(cold.unlock(Some(device.to_vec()))?);
    assert_eq!(cold.outbox_envelopes(), repaired);
    assert!(!cold.mls_seal_outbox()?);
    assert!(provider_rows(&cold) == provider);
    Ok(())
}
