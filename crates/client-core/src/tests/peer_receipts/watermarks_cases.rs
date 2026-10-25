//! Complete watermarks scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn own_text_and_voice_keep_delivered_then_read_after_offline_cold_reopens(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (db, e, c) = fixture()?;
    assert_delivery(&e, &c, Delivery::Sent);
    assert!(e.lock().receipt(&c.space, &c.peer, c.voice.seq, 0).unwrap());
    assert_delivery(&e, &c, Delivery::Delivered);
    drop(e);
    let e = db.open();
    assert_delivery(&e, &c, Delivery::Delivered);
    assert!(e.lock().receipt(&c.space, &c.peer, 0, c.voice.seq).unwrap());
    assert_delivery(&e, &c, Delivery::Read);
    drop(e);
    let e = db.open();
    assert_delivery(&e, &c, Delivery::Read);
    assert_eq!(
        e.file_bytes(c.voice.voice.as_ref().unwrap().item_id.clone(), None)
            .unwrap(),
        Some(vec![7; 900])
    );
    assert!(e.verify_log(c.space.clone()).valid);

    Ok(())
}

pub(super) fn independent_watermark_maxima_are_exact_u64_and_stale_replay_cannot_lower_them(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (db, e, c) = fixture()?;
    assert!(e.lock().receipt(&c.space, &c.peer, u64::MAX, 1).unwrap());
    assert!(e
        .lock()
        .receipt(&c.space, &c.peer, 2, u64::MAX - 1)
        .unwrap());
    let before = snapshot(&e);
    assert_eq!(
        before.rows,
        vec![(
            c.space.clone(),
            c.peer.clone(),
            format!("{:020}", u64::MAX),
            format!("{:020}", u64::MAX - 1)
        )]
    );
    for (delivered, read) in [
        (0, 0),
        (u64::MAX, 1),
        (1, u64::MAX - 2),
        (u64::MAX, u64::MAX - 1),
    ] {
        assert!(!e
            .lock()
            .receipt(&c.space, &c.peer, delivered, read)
            .unwrap());
        assert_eq!(snapshot(&e), before);
    }
    drop(e);
    let e = db.open();
    assert_eq!(snapshot(&e), before);
    assert!(e.lock().receipt(&c.space, &c.peer, 0, u64::MAX).unwrap());
    assert_eq!(
        e.lock().net.receipts[&c.space][&c.peer],
        (u64::MAX, u64::MAX)
    );
    assert_eq!(snapshot(&e).rows[0].3, format!("{:020}", u64::MAX));

    Ok(())
}

pub(super) fn delivery_aggregates_only_current_members_and_read_implies_delivered(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (_db, e, c) = fixture()?;
    membership(
        &e,
        &c,
        EventBody::MemberAdded {
            identity: c.outsider.clone(),
            role: Role::Member,
        },
    )?;
    assert!(e.lock().receipt(&c.space, &c.peer, 0, c.voice.seq).unwrap());
    assert_delivery(&e, &c, Delivery::Sent);
    assert!(e
        .lock()
        .receipt(&c.space, &c.outsider, c.voice.seq, 0)
        .unwrap());
    assert_delivery(&e, &c, Delivery::Delivered);
    membership(
        &e,
        &c,
        EventBody::MemberRemoved {
            identity: c.outsider.clone(),
        },
    )?;
    assert_delivery(&e, &c, Delivery::Read);
    assert_eq!(
        e.lock().net.receipts[&c.space][&c.peer],
        (c.voice.seq, c.voice.seq)
    );

    Ok(())
}

pub(super) fn receipt_history_does_not_override_pending_or_permanently_failed_sends(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (db, e, c) = fixture()?;
    let pending = e
        .send_message(c.space.clone(), "Still queued".into())
        .unwrap();
    assert!(e
        .lock()
        .receipt(&c.space, &c.peer, u64::MAX, u64::MAX)
        .unwrap());
    assert_eq!(entry(&e, &c, &pending.id).delivery, Delivery::Sending);
    {
        let mut engine = e.lock();
        let client = engine
            .store
            .outbox()
            .unwrap()
            .into_iter()
            .find(|row| row.event.space == c.space)
            .unwrap()
            .event
            .client_id;
        assert!(engine
            .reject(&client, "Synthetic permanent refusal", true, None)
            .user_failed());
    }
    assert_eq!(entry(&e, &c, &pending.id).delivery, Delivery::Failed);
    assert_eq!(entry(&e, &c, &c.voice.id).delivery, Delivery::Read);
    drop(e);
    let e = db.open();
    assert_eq!(entry(&e, &c, &pending.id).delivery, Delivery::Failed);
    assert_eq!(entry(&e, &c, &c.voice.id).delivery, Delivery::Read);

    Ok(())
}
