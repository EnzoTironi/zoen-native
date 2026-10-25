//! Characterize the synchronous upstream save API before adapting asynchronous ACK.
use crate::fixture::{Result, open, unknown_data};
use pdfcraft_engine::Edit;
use serde_json::{Value, json};
use std::sync::Arc;

pub fn delayed_ack(original: Arc<Vec<u8>>) -> Result<(Arc<Vec<u8>>, Value)> {
    let (mut session, id) = open(original.clone())?;
    session.apply(
        id,
        Edit::SetInfo {
            key: "Subject".into(),
            value: "Human review".into(),
        },
    )?;
    let checkpoint_generation = session.get(id).ok_or("missing_document")?.edit_generation();
    let accepted = session.save_bytes(id)?;
    session.check_save(id, &accepted)?;
    // This edit occurs AFTER the snapshot but BEFORE the simulated host ACK.
    session.apply(
        id,
        Edit::RotatePages {
            pages: vec![1],
            degrees: 90,
        },
    )?;
    let newer = session.get(id).ok_or("missing_document")?.edit_generation();
    if newer == checkpoint_generation {
        return Err("actual_edit_generation_did_not_advance".into());
    }
    let live_before_ack = session.save_bytes(id)?;
    // Upstream mark_saved is intentionally synchronous: applying it to an older
    // asynchronous snapshot replaces current COS. Observe that seam directly.
    let (mut synchronous, sync_id) = open(live_before_ack.clone())?;
    synchronous.mark_saved(sync_id, accepted.clone(), None)?;
    let sync_rotation = synchronous
        .get(sync_id)
        .ok_or("missing_sync_document")?
        .info
        .pages[1]
        .rotation;
    if sync_rotation != 0 {
        return Err("synchronous_baseline_observation_changed".into());
    }
    // Managed ACK must not rebase/replace the live Session on a newer generation.
    // The accepted bytes/head advance in the bridge; this Session remains intact.
    let live_after_ack = session.save_bytes(id)?;
    if live_before_ack != live_after_ack {
        return Err("newer_change_lost_during_ack".into());
    }
    session.undo(id)?;
    if session.get(id).ok_or("missing_document")?.info.pages[1].rotation != 0 {
        return Err("undo_after_delayed_ack_failed".into());
    }
    if session
        .get(id)
        .ok_or("missing_document")?
        .info
        .subject
        .as_deref()
        != Some("Human review")
    {
        return Err("undo_removed_preceding_human_edit".into());
    }
    session.redo(id)?;
    let final_bytes = session.save_bytes(id)?;
    session.check_save(id, &final_bytes)?;
    session.mark_saved(id, final_bytes.clone(), None)?;
    session.undo(id)?;
    if session.get(id).ok_or("missing_document")?.info.pages[1].rotation != 0 {
        return Err("upstream_undo_lost_on_stable_ack".into());
    }
    unknown_data(final_bytes.clone())?;
    let (cold, cold_id) = open(final_bytes.clone())?;
    if cold.get(cold_id).ok_or("missing_cold_document")?.info.pages[1].rotation != 90 {
        return Err("cold_recovery_lost_rotation".into());
    }
    if cold.save_bytes(cold_id)? != final_bytes {
        return Err("unmodified_reopen_normalized_bytes".into());
    }
    Ok((
        final_bytes,
        json!({"snapshotGeneration":checkpoint_generation,
        "newerGeneration":newer,"syncSaveWouldReplaceNewer":true,
        "managedAckPreservedNewer":true,"undoRedoAfterLateAck":true,
        "undoAfterStableAck":true,"coldRecoveryExact":true,"unknownCatalogValuePreserved":true}),
    ))
}
