//! Complete receipts scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn failed_delivery_send_does_not_consume_the_watermark() {
    let e = seeded();
    let s = e.spaces().into_iter().find(|s| s.unread > 0).unwrap();
    let first = e.lock().delivered_to_announce(&s.id).unwrap();
    // Merely preparing a frame is not a successful send. Retrying must retain it.
    assert_eq!(e.lock().delivered_to_announce(&s.id), Some(first));
}

pub(super) fn receipt_replay_survives_offline_reads_and_restarts() {
    let path = std::env::temp_dir().join(types::new_id("receipt-replay"));
    let e = Client::open(path.to_string_lossy().into(), "en".into()).unwrap();
    e.seed_demo_if_empty().unwrap();
    let spaces: Vec<_> = e.spaces().into_iter().filter(|s| s.unread > 0).collect();
    assert!(spaces.iter().any(|s| s.members.len() == 2));
    assert!(spaces.iter().any(|s| s.members.len() > 2));
    for s in &spaces {
        e.lock().net.synced.insert(s.id.clone());
        let delivered = e.lock().delivered_to_announce(&s.id).unwrap();
        e.lock().delivered_announced(&s.id, delivered);
        assert_eq!(e.lock().delivered_to_announce(&s.id), None);
        // No network task: only the durable local mark can carry this read forward.
        e.mark_read(s.id.clone()).unwrap();
        e.mark_read(s.id.clone()).unwrap();
    }
    for _ in 0..2 {
        let frames = e.lock().receipts_to_replay();
        for s in &spaces {
            assert!(frames.iter().any(|(id, k)| id == &s.id
                && matches!(k, protocol::EphemeralKind::Delivered { seq } if *seq > 0)));
            assert!(frames.iter().any(|(id, k)| id == &s.id
                && matches!(k, protocol::EphemeralKind::Read { seq } if *seq > 0)));
        }
    }
    drop(e);
    let e = Client::open(path.to_string_lossy().into(), "en".into()).unwrap();
    for s in &spaces {
        e.lock().net.synced.insert(s.id.clone());
    }
    let frames = e.lock().receipts_to_replay();
    for s in &spaces {
        assert!(frames
            .iter()
            .any(|(id, k)| id == &s.id
                && matches!(k, protocol::EphemeralKind::Read { seq } if *seq > 0)));
    }
    drop(e);
    std::fs::remove_file(path).unwrap();
}

pub(super) fn ticks_follow_the_lowest_read_up_to_watermark() {
    let dir = std::env::temp_dir().join(format!("client-test-{}", types::new_id("t")));
    std::fs::create_dir_all(&dir).unwrap();
    let e = Client::open(
        dir.join("r.sqlite").to_string_lossy().into(),
        "pt-BR".into(),
    )
    .unwrap();
    e.seed_demo_if_empty().unwrap();
    let s = e
        .spaces()
        .into_iter()
        .find(|s| s.members.len() >= 3)
        .expect("a group");
    e.send_message(s.id.clone(), "oi".into()).unwrap();
    let me = e.lock().me.clone().unwrap();
    let others: Vec<String> = s
        .members
        .iter()
        .map(|m| m.id.clone())
        .filter(|m| *m != me)
        .collect();
    e.lock().net.synced.insert(s.id.clone());
    let last = |e: &Client| e.timeline(s.id.clone()).unwrap().last().unwrap().clone();
    let seq = last(&e).seq;
    assert_eq!(last(&e).delivery, Delivery::Sent);
    for o in &others {
        assert!(e.lock().receipt(&s.id, o, seq, 0).unwrap());
    }
    assert_eq!(last(&e).delivery, Delivery::Delivered);
    assert_eq!(
        e.lock().space_summary(&s.id).unwrap().last_delivery,
        Some(Delivery::Delivered)
    );
    e.lock().receipt(&s.id, &others[0], 0, seq).unwrap();
    assert_eq!(
        last(&e).delivery,
        Delivery::Delivered,
        "a group is read only when everyone read"
    );
    for o in &others[1..] {
        e.lock().receipt(&s.id, o, 0, seq).unwrap();
    }
    assert_eq!(last(&e).delivery, Delivery::Read);
    for o in &others {
        assert!(!e.lock().receipt(&s.id, o, seq, seq).unwrap(), "duplicate");
        assert!(
            !e.lock()
                .receipt(&s.id, o, seq.saturating_sub(1), 0)
                .unwrap(),
            "stale"
        );
    }
    assert_eq!(last(&e).delivery, Delivery::Read);
    // A read on another of my devices clears the unread count here.
    let other_space = e
        .spaces()
        .into_iter()
        .find(|x| x.unread > 0)
        .expect("unread chat");
    let unread = other_space.unread;
    assert!(!e
        .lock()
        .receipt(&other_space.id, &me, u64::MAX / 2, 0)
        .unwrap());
    assert_eq!(
        e.lock().space_summary(&other_space.id).unwrap().unread,
        unread,
        "own-device delivered is not read"
    );
    assert!(e
        .lock()
        .receipt(&other_space.id, &me, 0, u64::MAX / 2)
        .unwrap());
    assert!(!e.lock().receipt(&other_space.id, &me, 0, 1).unwrap());
    assert_eq!(e.lock().space_summary(&other_space.id).unwrap().unread, 0);
}
