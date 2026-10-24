//! Complete receipt recovery scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn lost_sync_marker_or_scope_cannot_downgrade_a_retained_sealed_reply(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    for case in ["sync-marker", "scope", "both"] {
        let mut fixture = Fixture::new()?;
        fixture.grant("marker-original-trust", None);
        let trigger = fixture.message("Original marker request")?;
        let original = fixture
            .core
            .reply(SPACE, &trigger, 400)
            .unwrap()
            .intent()
            .clone();
        let prepared = fixture
            .core
            .prepare_reply(&original, "Exact sealed output", 500)
            .unwrap();
        let key = super::reference::reference_key(prepared.envelope().client_id());
        let reference = fixture.core.engine.store.meta(&key).unwrap().unwrap();
        if case != "scope" {
            fixture
                .core
                .engine
                .store
                .conn()
                .execute("DELETE FROM synced_spaces WHERE space = ?1", [SPACE])
                .unwrap();
            fixture.core =
                DeviceCore::restore(fixture.core.image().unwrap(), fixture.credential()).unwrap();
            assert!(!fixture.core.engine.net.synced.contains(SPACE));
        }
        if case != "sync-marker" {
            fixture.core.engine.state.spaces.remove(SPACE);
        }
        assert!(!fixture.core.engine.is_e2e(SPACE));
        let pending = fixture
            .core
            .engine
            .store
            .outbox_get(prepared.envelope().client_id())
            .unwrap()
            .unwrap();
        let plain = Envelope::plain(&pending);
        plain.verify().unwrap();
        let before = fixture.core.engine.logs[SPACE].head();
        assert!(
            matches!(
                fixture.core.ingest(vec![ordered(&fixture.owner, plain)]),
                Err(NativeError::History)
            ),
            "{case}"
        );
        assert!(fixture.core.image().is_err());
        assert_eq!(fixture.core.engine.logs[SPACE].head(), before);
        assert_eq!(fixture.core.engine.store.outbox_len().unwrap(), 1);
        assert_eq!(
            fixture.core.engine.store.meta(&key).unwrap().as_deref(),
            Some(reference.as_str())
        );
    }
    println!("JOURNEY actual persisted sync-marker loss and missing current scope projection cannot turn the original sealed Application into an accepted signed plaintext echo; original reference survives");

    Ok(())
}

pub(super) fn native_receipt_survives_revocation_expiry_and_cold_replay_without_new_effects(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut fixture = Fixture::new()?;
    fixture.grant("receipt-original-trust", Some(1000));
    let trigger = fixture.message("Original receipt source")?;
    let original = fixture
        .core
        .reply(SPACE, &trigger, 400)
        .unwrap()
        .intent()
        .clone();
    let prepared = fixture
        .core
        .prepare_reply(&original, "Original accepted native result", 500)
        .unwrap();
    let client_id = prepared.envelope().client_id().to_string();
    let key = super::reference::reference_key(&client_id);
    let reference = fixture.core.engine.store.meta(&key).unwrap().unwrap();
    let accepted = ordered(&fixture.owner, prepared.envelope().clone());
    assert_eq!(fixture.owner.ingest(accepted.clone()), Ingest::Applied);
    let first = fixture
        .core
        .receive_reply(&original, &client_id, vec![accepted.clone()])
        .unwrap();
    assert_eq!(first.accepted().hash, accepted.hash);
    assert_eq!(fixture.core.engine.store.outbox_len().unwrap(), 0);
    assert!(fixture
        .core
        .engine
        .store
        .meta(&format!("mls.sealed:{client_id}"))
        .unwrap()
        .is_none());
    fixture.encrypted(
        EventBody::GrantRevoked {
            grant: "receipt-original-trust".into(),
        },
        1100,
    );
    assert!(matches!(
        fixture.core.refresh(&original, 1200),
        Err(NativeError::Permission)
    ));
    let before = fixture.core.image().unwrap();
    let mut cold = DeviceCore::restore(
        NativeImage::decode_bounded(before.as_bytes().to_vec()).unwrap(),
        fixture.credential(),
    )
    .unwrap();
    let receipt = cold
        .receive_reply(&original, &client_id, vec![accepted.clone()])
        .unwrap();
    assert_eq!(receipt.original(), &original);
    assert_eq!(receipt.message()?, prepared.message()?);
    assert_eq!(receipt.event_digest(), prepared.event_digest());
    assert_eq!(receipt.accepted(), first.accepted());
    assert_eq!(receipt.image().as_bytes(), before.as_bytes());
    assert_eq!(
        cold.engine.store.meta(&key).unwrap().as_deref(),
        Some(reference.as_str())
    );
    assert_eq!(cold.engine.store.outbox_len().unwrap(), 0);
    assert!(cold.engine.logs[SPACE]
        .events()
        .iter()
        .all(|e| !matches!(e.body, EventBody::UsageRecorded { .. })));
    let mut changed = original.clone();
    changed.grant_digest = "00".repeat(32);
    assert!(matches!(
        cold.receive_reply(&changed, &client_id, vec![]),
        Err(NativeError::History)
    ));
    assert!(cold.image().is_err());
    println!("JOURNEY exact retained receipt survives accepted native grant revoke/expiry and cold replay without provider/image changes, new outbox or usage; a different original refuses");

    Ok(())
}

pub(super) fn receipt_refuses_different_duplicate_opening_and_missing_original_reference(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    for case in [
        "duplicate-ciphertext",
        "pruned-duplicate",
        "changed-opening",
        "missing-reference",
        "changed-message",
        "changed-digest",
        "future-reference",
    ] {
        let mut fixture = Fixture::new()?;
        fixture.grant("duplicate-original-trust", None);
        let trigger = fixture.message("Original duplicate source")?;
        let original = fixture
            .core
            .reply(SPACE, &trigger, 400)
            .unwrap()
            .intent()
            .clone();
        let prepared = fixture
            .core
            .prepare_reply(&original, "Exact native original output", 500)
            .unwrap();
        let client_id = prepared.envelope().client_id().to_string();
        let key = super::reference::reference_key(&client_id);
        let different = colliding_native_reply(&fixture, &prepared)?;
        let accepted = ordered(&fixture.owner, prepared.envelope().clone());
        assert_eq!(fixture.owner.ingest(accepted.clone()), Ingest::Applied);
        fixture
            .core
            .receive_reply(&original, &client_id, vec![accepted.clone()])
            .unwrap();
        let committed = fixture.core.image().unwrap();
        let mut replay = accepted.clone();
        match case {
            "duplicate-ciphertext" => {
                replay.env = different;
            }
            "pruned-duplicate" => {
                replay.env = replay.env.pruned().unwrap();
            }
            "changed-opening" => {
                let opening_key = format!("runtime.mls-opening.v1/{}", accepted.hash);
                let opening = fixture
                    .core
                    .engine
                    .store
                    .meta(&opening_key)
                    .unwrap()
                    .unwrap();
                fixture
                    .core
                    .engine
                    .store
                    .set_meta(&opening_key, &opening.replace("OwnRetainedEcho", "Opaque"))
                    .unwrap();
            }
            "missing-reference" => fixture.core.engine.store.meta_delete(&key).unwrap(),
            "changed-message" | "changed-digest" | "future-reference" => {
                let json = fixture.core.engine.store.meta(&key).unwrap().unwrap();
                let changed = match case {
                    "changed-message" => json.replace(prepared.message()?, "different-message"),
                    "changed-digest" => json.replace(prepared.event_digest(), &"00".repeat(32)),
                    "future-reference" => json.replace("\"version\":1", "\"version\":2"),
                    _ => unreachable!(),
                };
                assert_ne!(json, changed);
                fixture.core.engine.store.set_meta(&key, &changed).unwrap();
            }
            _ => unreachable!(),
        }
        fixture.core =
            DeviceCore::restore(fixture.core.image().unwrap(), fixture.credential()).unwrap();
        assert!(
            matches!(
                fixture
                    .core
                    .receive_reply(&original, &client_id, vec![replay]),
                Err(NativeError::History)
            ),
            "{case}"
        );
        assert!(fixture.core.image().is_err());
        let mut good = DeviceCore::restore(committed, fixture.credential()).unwrap();
        assert_eq!(
            good.receive_reply(&original, &client_id, vec![accepted])
                .unwrap()
                .event_digest(),
            prepared.event_digest()
        );
    }
    println!("JOURNEY receipt replay refuses another signed ciphertext or pruned duplicate, opaque opening, missing/future reference and changed original message/digest after actual cold store roundtrip");

    Ok(())
}

pub(super) fn actual_maximum_text_reference_is_bounded_and_oversized_fields_refuse(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let extreme_space = "\0".repeat(256);
    let mut fixture = Fixture::new_in(&extreme_space)?;
    fixture.grant(&"\0".repeat(256), None);
    let trigger = fixture.message("Bounded original source")?;
    let original = fixture
        .core
        .reply(&extreme_space, &trigger, 400)
        .unwrap()
        .intent()
        .clone();
    let text = "🧬".repeat(super::publication::MAX_REPLY_BYTES / 4);
    let prepared = fixture.core.prepare_reply(&original, &text, 500).unwrap();
    let client_id = prepared.envelope().client_id().to_string();
    let key = super::reference::reference_key(&client_id);
    let json = fixture.core.engine.store.meta(&key).unwrap().unwrap();
    let wire = ClientFrame::Publish {
        env: prepared.envelope().clone(),
    }
    .encode();
    assert!(json.len() <= super::reference::MAX_REFERENCE_BYTES);
    assert!(wire.len() <= super::publication::MAX_PUBLISH_BYTES);
    let mut cold = DeviceCore::restore(
        NativeImage::decode_bounded(prepared.image().as_bytes().to_vec()).unwrap(),
        fixture.credential(),
    )
    .unwrap();
    let accepted = ordered(&fixture.owner, prepared.envelope().clone());
    assert_eq!(fixture.owner.ingest(accepted.clone()), Ingest::Applied);
    let receipt = cold
        .receive_reply(&original, &client_id, vec![accepted])
        .unwrap();
    assert_eq!(receipt.event_digest(), prepared.event_digest());
    assert!(
        matches!(&cold.engine.logs[&extreme_space].events().last().unwrap().body, EventBody::MessagePosted { text: actual, .. } if actual == &text)
    );
    cold.engine
        .store
        .set_meta(&key, &" ".repeat(super::reference::MAX_REFERENCE_BYTES + 1))
        .unwrap();
    assert!(matches!(
        cold.receive_reply(&original, &client_id, vec![]),
        Err(NativeError::Bounds)
    ));
    assert!(cold.image().is_err());
    let mut oversized = Fixture::new()?;
    oversized.grant(&"g".repeat(257), None);
    let trigger = oversized.message("Oversized original field")?;
    let original = oversized
        .core
        .reply(SPACE, &trigger, 400)
        .unwrap()
        .intent()
        .clone();
    let before = oversized.core.image().unwrap();
    assert!(matches!(
        oversized
            .core
            .prepare_reply(&original, "Unpublished output", 500),
        Err(NativeError::Bounds)
    ));
    assert!(oversized.core.image().is_err());
    assert_eq!(
        DeviceCore::restore(before, oversized.credential())
            .unwrap()
            .engine
            .store
            .outbox_len()
            .unwrap(),
        0
    );
    println!("JOURNEY actual 32768-byte UTF-8 reply with 256-byte JSON-escaped Space/grant fields produces reference_json={} publish={} image={} bytes; 135169-byte metadata and 257-byte field refuse without adopting or truncating", json.len(), wire.len(), prepared.image().as_bytes().len());

    Ok(())
}

pub(super) fn receipt_delta_preserves_other_pending_and_refuses_real_new_join_or_row_changes(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut fixture = Fixture::new()?;
    fixture.grant("delta-original-trust", None);
    let trigger = fixture.message("Original delta source")?;
    let original = fixture
        .core
        .reply(SPACE, &trigger, 400)
        .unwrap()
        .intent()
        .clone();
    let prepared = fixture
        .core
        .prepare_reply(&original, "Original delta output", 500)
        .unwrap();
    let client_id = prepared.envelope().client_id().to_string();
    let before = fixture.core.snapshot_outbox().unwrap();
    let author = fixture.core.engine.net.author.clone().unwrap();
    let joining = fixture
        .core
        .engine
        .append_synced(
            SPACE,
            &author,
            600,
            EventBody::DeviceJoining {
                device: fixture.agent_device.id(),
            },
        )
        .unwrap();
    event_log::verify_author(&joining).unwrap();
    assert!(matches!(
        fixture.core.verify_receipt_delta(&before, &client_id),
        Err(NativeError::History)
    ));
    let surviving = fixture
        .core
        .engine
        .store
        .outbox_get(&joining.client_id)
        .unwrap()
        .unwrap();
    let accepted = ordered(&fixture.owner, prepared.envelope().clone());
    assert_eq!(fixture.owner.ingest(accepted.clone()), Ingest::Applied);
    fixture
        .core
        .receive_reply(&original, &client_id, vec![accepted])
        .unwrap();
    assert_eq!(
        fixture
            .core
            .engine
            .store
            .outbox_get(&joining.client_id)
            .unwrap(),
        Some(surviving.clone())
    );
    let before = fixture.core.snapshot_outbox().unwrap();
    fixture
        .core
        .engine
        .store
        .outbox_note(&joining.client_id, "changed retry state", false)
        .unwrap();
    assert!(matches!(
        fixture.core.verify_receipt_delta(&before, &client_id),
        Err(NativeError::History)
    ));
    fixture
        .core
        .engine
        .store
        .outbox_remove(&joining.client_id)
        .unwrap();
    fixture.core.engine.net.pending.remove(&joining.client_id);
    assert!(matches!(
        fixture.core.verify_receipt_delta(&before, &client_id),
        Err(NativeError::History)
    ));
    let before = fixture.core.snapshot_outbox().unwrap();
    fixture
        .core
        .engine
        .store
        .set_meta("mls.sealed:orphan-metadata", "1:00:00")
        .unwrap();
    assert!(matches!(
        fixture.core.verify_receipt_delta(&before, &client_id),
        Err(NativeError::History)
    ));
    println!("JOURNEY explicit delta-check refuses a separately generated genuinely signed joining event, retry-row change, unrelated removal and new orphan sealing metadata; actual target receipt preserves an existing other pending exactly");

    Ok(())
}
