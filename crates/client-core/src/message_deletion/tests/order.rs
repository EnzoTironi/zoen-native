use super::*;

#[test]
fn interleaved_received_and_own_acknowledgements_keep_the_replay_order() -> TestResult {
    let f = fixture(":memory:")?;
    let remote = ordered(
        &f,
        types::EventBody::MessagePosted {
            message: types::new_id("msg")?,
            text: "Received before acknowledgements".into(),
            attaches: None,
            media: None,
            reply: None,
            forwarded: false,
            forwarded_author: None,
        },
    )?;
    let initial = timeline(&f)?;
    let confirmed = initial
        .iter()
        .position(|row| row.id == remote)
        .ok_or("remote")?;
    let pending = initial
        .iter()
        .position(|row| row.id == f.input.targets[0].message_id)
        .ok_or("pending")?;
    assert!(confirmed < pending);
    f.client.lock().reload()?;
    assert_eq!(timeline(&f)?, initial);
    let mut outbox = f.client.lock().store.outbox()?;
    // The relay can accept a later local write first. Preserve its actual sequence.
    outbox.reverse();
    for row in outbox {
        ingest(
            &mut f.client.lock(),
            row.event,
            crate::sync::Ingest::Confirmed,
        )?;
        let before = timeline(&f)?;
        f.client.lock().reload()?;
        assert_eq!(timeline(&f)?, before);
    }
    let final_rows = timeline(&f)?;
    assert!(f.client.lock().state.spaces[&f.input.space_id]
        .entries
        .iter()
        .all(|row| !row.hash.is_empty()));
    assert!(final_rows.windows(2).all(|pair| pair[0].seq <= pair[1].seq));
    Ok(())
}

#[test]
fn confirmation_moves_both_result_rows_and_preserves_other_pending_rows() -> TestResult {
    let f = fixture(":memory:")?;
    let mut state = f.client.lock().state.spaces[&f.input.space_id].clone();
    let source = state.entries.last().ok_or("source")?.clone();
    let entry = |id: &str, seq, hash: &str| crate::engine::Entry {
        client_id: id.into(),
        seq,
        hash: hash.into(),
        ..source.clone()
    };
    state.entries = vec![
        entry("confirmed", 1, "hash1"),
        entry("pending-a", 0, ""),
        entry("approval", 0, ""),
        entry("approval:result", 0, ""),
        entry("pending-b", 0, ""),
    ];
    state.confirm_pending_entries("approval", 2, "hash2");
    assert_eq!(
        state.entries.iter().map(|row| row.id()).collect::<Vec<_>>(),
        [
            "confirmed",
            "approval",
            "approval:result",
            "pending-a",
            "pending-b"
        ]
    );
    assert!(state.entries[1..3]
        .iter()
        .all(|row| row.seq == 2 && row.hash == "hash2"));
    state.entries.push(entry("remote", 3, "hash3"));
    state.entries.push(entry("remote:result", 3, "hash3"));
    state.settle_confirmed_tail(3);
    assert_eq!(
        state.entries.iter().map(|row| row.id()).collect::<Vec<_>>(),
        [
            "confirmed",
            "approval",
            "approval:result",
            "remote",
            "remote:result",
            "pending-a",
            "pending-b"
        ]
    );
    state.confirm_pending_entries("approval", 2, "hash2");
    state.settle_confirmed_tail(4); // An event without a timeline entry changes nothing.
    assert_eq!(state.entries.last().map(|row| row.id()), Some("pending-b"));
    Ok(())
}
