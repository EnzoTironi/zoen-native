//! Terminal consent failure retains the human request without wedging the lane.
use super::*;

#[test]
fn terminal_share_failure_survives_cold_and_needs_a_fresh_explicit_request() -> R<()> {
    let dir = std::env::temp_dir().join(types::new_id("share-request-failure"));
    std::fs::create_dir_all(&dir).map_err(storage)?;
    let op = dir.join("owner.sqlite").to_string_lossy().into_owned();
    let ap = dir.join("agent.sqlite").to_string_lossy().into_owned();
    let mut f = late_word(&op, &ap)?;
    let command = composition::input(&f, "old_consent_request", true)?;
    let first = f.owner.request_app_edit(command.clone())?;
    let share = f
        .owner
        .store
        .outbox()?
        .into_iter()
        .find(|row| matches!(row.event.body, EventBody::AppCurrentShared { .. }))
        .ok_or_else(|| storage("normal composed Share"))?
        .event;
    let ordinary = f
        .owner
        .send_message(SPACE, "An unrelated message must still be sent")?;
    f.owner.refuse_stale_app_share(&share)?;
    let rows = f.owner.store.outbox()?;
    assert!(rows
        .iter()
        .any(|row| row.event.client_id == share.client_id && row.failed));
    assert!(rows
        .iter()
        .any(|row| row.event.client_id == first.id && row.failed));
    assert!(rows
        .iter()
        .any(|row| row.event.client_id == ordinary.id && !row.failed));
    let failed = f
        .owner
        .timeline(SPACE)?
        .into_iter()
        .find(|entry| entry.id == first.id)
        .ok_or_else(|| storage("failed human prompt remains visible"))?;
    assert_eq!(failed.delivery, Delivery::Failed);
    assert!(matches!(
        f.owner.request_app_edit(command.clone()),
        Err(CoreError::Stale { .. })
    ));
    let secret = f.owner_secret;
    drop(f.owner);
    f.owner = Engine::open(&op)?;
    assert!(f.owner.unlock(Some(secret.to_vec()))?);
    assert!(matches!(
        f.owner.request_app_edit(command),
        Err(CoreError::Stale { .. })
    ));
    f.owner.mls_seal_outbox()?;
    let offered = f.owner.outbox_envelopes();
    assert!(offered.iter().any(|entry| entry.client_id() == ordinary.id));
    assert!(offered
        .iter()
        .all(|entry| entry.client_id() != first.id && entry.client_id() != share.client_id));
    publish(&mut f.owner, &mut f.agent, &mut f.blobs);
    assert!(f.agent.logs[SPACE]
        .events()
        .iter()
        .all(|entry| entry.client_id != first.id));
    let fresh = composition::input(&f, "fresh_reviewed_consent", true)?;
    let next = f.owner.request_app_edit(fresh)?;
    assert_ne!(next.id, first.id);
    publish(&mut f.owner, &mut f.agent, &mut f.blobs);
    receive_wanted(&mut f.agent, &f.blobs)?;
    let request = f
        .agent
        .artifact_request(SPACE, &next.id)?
        .ok_or_else(|| storage("fresh reviewed request"))?;
    assert_eq!(request.item_id, f.original.item_id);
    assert_eq!(
        f.agent
            .app_binary_read(
                &request.item_id,
                Some(request.version),
                Some(&request.sha256)
            )?
            .data_base64,
        f.original.data_base64
    );
    assert_eq!(
        f.owner.logs[SPACE]
            .events()
            .iter()
            .filter(|entry| matches!(entry.body, EventBody::AppCurrentShared { .. }))
            .count(),
        1
    );
    drop(f);
    std::fs::remove_dir_all(dir).map_err(storage)?;
    Ok(())
}

#[test]
fn terminal_failed_sent_heads_do_not_consume_the_next_application_window() -> R<()> {
    let mut f = late_word(":memory:", ":memory:")?;
    for n in 0..Engine::APPLICATION_IN_FLIGHT {
        f.owner
            .send_message(SPACE, &format!("Actual queued message {n}"))?;
    }
    f.owner.mls_seal_outbox()?;
    let rows = f.owner.store.outbox()?;
    let sent: HashMap<_, _> = rows
        .iter()
        .map(|row| {
            (
                row.event.client_id.clone(),
                f.owner.sealed_epoch(&row.event),
            )
        })
        .collect();
    assert_eq!(sent.len(), Engine::APPLICATION_IN_FLIGHT);
    assert!(sent.values().all(Option::is_some));
    for row in rows {
        assert!(matches!(
            f.owner.reject(
                &row.event.client_id,
                "Terminal send refusal",
                true,
                sent[&row.event.client_id]
            ),
            crate::sync::RejectionOutcome::UserFailed
        ));
    }
    let next = f
        .owner
        .send_message(SPACE, "Fresh message after terminal refusals")?;
    f.owner.mls_seal_outbox()?;
    let offered = f.owner.outbox_envelopes_except(&sent);
    assert_eq!(offered.len(), 1);
    assert_eq!(offered[0].client_id(), next.id);
    Ok(())
}
