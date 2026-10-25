use super::*;

pub(super) fn chain(space: &str, events: &[Sequenced]) {
    let mut previous = types::GENESIS_PREV.to_owned();
    for (index, event) in events.iter().enumerate() {
        event.env.verify().unwrap();
        assert_eq!(event.seq, index as u64, "gapless canonical sequence");
        assert_eq!(event.prev, previous);
        assert_eq!(
            event.hash,
            event_log::chain_hash(space, event.seq, &event.prev, &event.env.wire_hash()).unwrap()
        );
        previous.clone_from(&event.hash);
    }
}

pub(super) fn healthy_socket(records: &[Record], connection: u64) {
    assert_eq!(
        records
            .iter()
            .filter(|r| matches!(r.kind, Kind::Open))
            .count(),
        1
    );
    for record in records {
        assert_eq!(record.connection, connection);
        match &record.kind {
            Kind::Close => {
                panic!("receiver socket changed or failed: {records:#?}");
            }
            Kind::TransportError(error) => panic!("receiver transport failed: {error}"),
            Kind::RelayError(error) => panic!("receiver relay refused a frame: {error}"),
            _ => {}
        }
    }
}

pub(super) fn recovered_wire(
    records: &[Record],
    checkpoint: usize,
    connection: u64,
    initial: std::time::Instant,
    space: &str,
    refused: &Sequenced,
) {
    healthy_socket(records, connection);
    let after = &records[checkpoint..];
    let repairs: Vec<_> = after
        .iter()
        .filter(|r| matches!(r.kind, Kind::Sync { .. }))
        .collect();
    assert_eq!(repairs.len(), 1, "one autonomous full repair");
    assert!(repairs[0].at.duration_since(initial) >= Duration::from_millis(59_500));
    let Kind::Sync { cursors, all: true } = &repairs[0].kind else {
        panic!("repair must use normal all-space durable cursor Sync")
    };
    assert_eq!(
        cursors
            .iter()
            .filter(|c| c.space == space)
            .map(|c| c.next_seq)
            .collect::<Vec<_>>(),
        vec![refused.seq]
    );
    let received: Vec<_> = after
        .iter()
        .filter(|r| matches!(r.kind, Kind::Event { .. }))
        .collect();
    assert_eq!(
        received.len(),
        1,
        "target never reached live NATS; one direct replay only"
    );
    assert!(received[0].at >= repairs[0].at);
    assert!(
        matches!(&received[0].kind, Kind::Event { space: s, id, seq, hash, dropped: false }
        if s == space && id == refused.env.client_id() && *seq == refused.seq && hash == &refused.hash)
    );
    let recovery_triggers: Vec<_> = after
        .iter()
        .filter_map(|record| match &record.kind {
            Kind::Publish => Some("receiver Publish".to_owned()),
            Kind::Joined(space) => Some(format!("receiver Joined {space}")),
            _ => None,
        })
        .collect();
    assert!(
        recovery_triggers.is_empty(),
        "recovery must be autonomous: {recovery_triggers:?}"
    );
}

pub(super) fn cold(
    path: &std::path::Path,
    vault: Arc<fixture::Vault>,
    space: &str,
    id: &str,
    timeline: &[client_core::TimelineEntry],
    refused: &Sequenced,
) {
    {
        let store = store::Store::open(path.to_str().unwrap()).unwrap();
        let events = store.events(space).unwrap();
        let selected: Vec<_> = events
            .iter()
            .filter(|event| event.client_id == id)
            .collect();
        assert_eq!(selected.len(), 1);
        let event = selected[0];
        assert_eq!(event.seq, refused.seq);
        assert_eq!(event.hash, refused.hash);
        assert_eq!(event.prev, refused.prev);
        assert_eq!(
            event.sealed_wire,
            Some(refused.env.wire_hash()),
            "accepted plaintext retains the hash of these exact signed ciphertext bytes"
        );
        assert_eq!(
            store
                .cursors()
                .unwrap()
                .iter()
                .filter(|(s, _)| s == space)
                .map(|(_, n)| *n)
                .collect::<Vec<_>>(),
            vec![refused.seq + 1]
        );
        assert!(
            store.outbox().unwrap().is_empty(),
            "cold open has no resend intent"
        );
    }
    let cold =
        client_core::Client::open(path.to_string_lossy().into_owned(), "en-US".into()).unwrap();
    assert!(cold.unlock(vault).unwrap());
    let current = cold.timeline(space.into()).unwrap();
    // Delivery is connection-local presentation; the stopped client has no
    // current socket. Every other DTO field must survive exactly, in order.
    let durable_projection = |entries: &[client_core::TimelineEntry]| {
        entries
            .iter()
            .map(|entry| {
                let mut value = serde_json::to_value(entry).unwrap();
                value.as_object_mut().unwrap().remove("delivery");
                value
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(durable_projection(&current), durable_projection(timeline));
    assert_eq!(current.iter().filter(|e| e.id == id).count(), 1);
}
