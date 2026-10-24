//! Complete receipt prefix scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn conflicting_sealed_seen_blocks_cold_restore_with_valid_pending_credentials(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut fixture = Fixture::new()?;
    fixture.grant("conflicting-prefix-trust", None);
    let trigger = fixture.message("Original retained input")?;
    let original = fixture
        .core
        .reply(SPACE, &trigger, 400)
        .unwrap()
        .intent()
        .clone();
    let prepared = fixture
        .core
        .prepare_reply(&original, "Original retained output", 500)
        .unwrap();
    let before = fixture.core.retained_head(SPACE).unwrap().unwrap();
    let pending = fixture.core.snapshot_outbox().unwrap();
    let source = receipt_source_message(&mut fixture, 0)?;
    assert_eq!(source.env.seen(), prepared.envelope().seen());
    let diverged = Seen {
        seq: before.seq,
        hash: "00".repeat(32),
    };
    assert_ne!(diverged.hash, before.hash);
    let (kind, bytes) = source.env.sealed_data().unwrap();
    let author = fixture.owner.net.author.as_ref().unwrap();
    let conflicting = Envelope::sealed(
        author,
        SPACE,
        source.env.client_id(),
        source.env.at_ms(),
        Some(&diverged),
        Sealed::new(kind, ::mls::SUITE_ID, bytes.to_vec()),
    );
    conflicting.verify().unwrap();
    let frame = ordered(&fixture.core.engine, conflicting);
    let got = fixture.core.engine.ingest(frame);
    assert!(matches!(got, Ingest::Invalid(_)), "{got:?}");
    assert_eq!(fixture.core.retained_head(SPACE).unwrap(), Some(before));
    assert!(fixture.core.snapshot_outbox().unwrap() == pending);
    assert!(fixture.core.engine.fork_recorded(SPACE));
    assert!(fixture.core.engine.outbox_envelopes().is_empty());
    assert!(matches!(
        DeviceCore::restore(fixture.core.image().unwrap(), fixture.credential()),
        Err(NativeError::Credential)
    ));
    println!("JOURNEY a member's real signature over a conflicting held-prefix hash preserves the pending reply but parks publication and rejects cold restore with the matching certified device");

    Ok(())
}

pub(super) fn receipt_prefix_keeps_exact_pending_bytes_and_resumes_beyond_64_real_positions(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    use sha2::{Digest, Sha256};
    let mut fixture = Fixture::new()?;
    fixture.grant("prefix-trust", Some(1000));
    let trigger = fixture.message("Original prefix input")?;
    let original = fixture
        .core
        .reply(SPACE, &trigger, 400)
        .unwrap()
        .intent()
        .clone();
    let prepared = fixture
        .core
        .prepare_reply(&original, "Paid original output", 500)
        .unwrap();
    let cid = prepared.envelope().client_id().to_string();
    let key = super::reference::reference_key(&cid);
    let json = fixture.core.engine.store.meta(&key).unwrap().unwrap();
    assert_eq!(
        prepared.reference_digest(),
        hex::encode(Sha256::digest(json.as_bytes()))
    );
    assert_eq!(
        prepared.publish_digest(),
        hex::encode(Sha256::digest(
            ClientFrame::Publish {
                env: prepared.envelope().clone()
            }
            .encode()
        ))
    );
    fixture
        .core
        .engine
        .store
        .set_meta("mls.sealed:preserved-orphan", "original opaque metadata")
        .unwrap();
    let author = fixture.core.engine.net.author.clone().unwrap();
    fixture
        .core
        .engine
        .append_synced(
            SPACE,
            &author,
            550,
            EventBody::DeviceJoining {
                device: fixture.agent_device.id(),
            },
        )
        .unwrap();
    let pending = fixture.core.snapshot_outbox().unwrap();
    let before = fixture.core.retained_head(SPACE).unwrap().unwrap();
    let mut batch = Vec::new();
    for index in 0..65 {
        batch.push(receipt_source_message(&mut fixture, index)?);
    }
    let accepted = ordered(&fixture.owner, prepared.envelope().clone());
    assert_eq!(fixture.owner.ingest(accepted.clone()), Ingest::Applied);
    let first = batch.drain(..64).collect::<Vec<_>>();
    let frontier = first.last().unwrap().clone();
    let ReplyReceipt::Prefix(prefix) = fixture
        .core
        .receive_reply_progress(&original, &cid, first)
        .unwrap()
    else {
        panic!("target was beyond the real first batch");
    };
    assert_eq!(prefix.before(), &before);
    assert_eq!(
        prefix.frontier(),
        &Seen {
            seq: frontier.seq,
            hash: frontier.hash
        }
    );
    assert_eq!(prefix.reference_digest(), prepared.reference_digest());
    assert_eq!(prefix.publish_digest(), prepared.publish_digest());
    assert!(fixture.core.snapshot_outbox().unwrap() == pending);
    assert_eq!(fixture.core.engine.store.meta(&key).unwrap().unwrap(), json);
    let mut cold = DeviceCore::restore(
        NativeImage::decode_bounded(prefix.image().as_bytes().to_vec()).unwrap(),
        fixture.credential(),
    )
    .unwrap();
    batch.push(accepted.clone());
    let ReplyReceipt::Accepted(receipt) =
        cold.receive_reply_progress(&original, &cid, batch).unwrap()
    else {
        panic!("actual native acceptance must produce AcceptedReply");
    };
    assert_eq!(
        receipt.accepted(),
        &Seen {
            seq: accepted.seq,
            hash: accepted.hash
        }
    );
    assert_eq!(receipt.reference_digest(), prepared.reference_digest());
    assert_eq!(receipt.publish_digest(), prepared.publish_digest());
    assert_eq!(cold.engine.store.outbox_len().unwrap(), 1);
    assert_eq!(
        cold.engine
            .store
            .meta("mls.sealed:preserved-orphan")
            .unwrap()
            .as_deref(),
        Some("original opaque metadata")
    );
    println!("JOURNEY 65 real certified encrypted source positions resume across cold 64-position prefix; pending retry rows, other command, orphan sealing and original reference remain exact; only the actual target produces acceptance");

    Ok(())
}

pub(super) fn receipt_prefix_validates_original_pending_before_reading_unrelated_history(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    for case in ["pending-inner", "missing-sealed-copy", "pending-map"] {
        let mut fixture = Fixture::new()?;
        fixture.grant("prefix-pending-trust", None);
        let trigger = fixture.message("Original pending input")?;
        let original = fixture
            .core
            .reply(SPACE, &trigger, 400)
            .unwrap()
            .intent()
            .clone();
        let prepared = fixture
            .core
            .prepare_reply(&original, "Original pending output", 500)
            .unwrap();
        let cid = prepared.envelope().client_id().to_string();
        let next = receipt_source_message(&mut fixture, 0)?;
        let initial = fixture.core.retained_head(SPACE).unwrap().unwrap();
        match case {
            "pending-inner" => {
                let author = fixture.core.engine.net.author.clone().unwrap();
                let event = author.sign_event(
                    SPACE,
                    &cid,
                    500,
                    prepared.envelope().seen(),
                    EventBody::MessagePosted {
                        message: prepared.message()?.into(),
                        text: "Different certified output".into(),
                        attaches: None,
                        reply: None,
                        media: None,
                        forwarded: false,
                    },
                );
                fixture.core.engine.store.outbox_remove(&cid).unwrap();
                fixture.core.engine.store.outbox_put(&event).unwrap();
            }
            "missing-sealed-copy" => fixture
                .core
                .engine
                .store
                .conn()
                .execute(
                    "DELETE FROM meta WHERE key=?1",
                    [format!("mls.sealed:{cid}")],
                )
                .map(|_| ())
                .unwrap(),
            "pending-map" => {
                fixture.core.engine.net.pending.remove(&cid);
            }
            _ => unreachable!(),
        }
        assert!(
            matches!(
                fixture
                    .core
                    .receive_reply_progress(&original, &cid, vec![next]),
                Err(NativeError::History)
            ),
            "{case}"
        );
        assert!(fixture.core.image().is_err());
        assert_eq!(fixture.core.engine.logs[SPACE].head().unwrap(), initial);
    }
    println!("JOURNEY tampered certified inner, missing original sealed copy and pending-map mismatch refuse before applying unrelated prefix history");

    Ok(())
}

pub(super) fn receipt_prefix_refuses_skipped_duplicate_oversized_and_partially_invalid_batches(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    for case in [
        "skip",
        "duplicate",
        "oversized",
        "late-invalid",
        "other-pending-acceptance",
    ] {
        let mut fixture = Fixture::new()?;
        fixture.grant("prefix-batch-trust", None);
        let trigger = fixture.message("Original batch input")?;
        let original = fixture
            .core
            .reply(SPACE, &trigger, 400)
            .unwrap()
            .intent()
            .clone();
        let prepared = fixture
            .core
            .prepare_reply(&original, "Original batch output", 500)
            .unwrap();
        let cid = prepared.envelope().client_id().to_string();
        let prior = fixture.core.image().unwrap().as_bytes().to_vec();
        let first = receipt_source_message(&mut fixture, 0)?;
        let mut batch = vec![first.clone()];
        match case {
            "skip" => {
                batch = vec![receipt_source_message(&mut fixture, 1)?];
            }
            "duplicate" => {
                fixture.core.ingest(vec![first.clone()]).unwrap();
            }
            "oversized" => {
                for index in 1..65 {
                    batch.push(receipt_source_message(&mut fixture, index)?);
                }
            }
            "late-invalid" => {
                let mut last = receipt_source_message(&mut fixture, 1)?;
                last.hash = "00".repeat(32);
                batch.push(last);
            }
            "other-pending-acceptance" => {
                let author = fixture.core.engine.net.author.clone().unwrap();
                let event = fixture
                    .core
                    .engine
                    .append_synced(
                        SPACE,
                        &author,
                        550,
                        EventBody::DeviceJoining {
                            device: fixture.agent_device.id(),
                        },
                    )
                    .unwrap();
                let last = ordered(&fixture.owner, Envelope::plain(&event));
                assert_eq!(fixture.owner.ingest(last.clone()), Ingest::Applied);
                batch.push(last);
            }
            _ => unreachable!(),
        }
        assert!(
            fixture
                .core
                .receive_reply_progress(&original, &cid, batch)
                .is_err(),
            "{case}"
        );
        assert!(fixture.core.image().is_err());
        let cold = DeviceCore::restore(
            NativeImage::decode_bounded(prior).unwrap(),
            fixture.credential(),
        )
        .unwrap();
        assert_eq!(
            cold.engine.outbox_envelopes(),
            vec![prepared.envelope().clone()]
        );
    }
    println!("JOURNEY source gap, replay-only prefix, 65-position input, invalid later chain and another pending acceptance refuse whole scratch; original committed publication stays recoverable");

    Ok(())
}
