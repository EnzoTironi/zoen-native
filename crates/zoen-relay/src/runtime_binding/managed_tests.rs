//! Real independent anchors: bootstrap proves pairing, never an effect permit.
use super::*;
use crate::managed_devices::{tests::fixture, ManagedDevices};

#[test]
fn bootstrap_binding_requires_exact_independent_genesis_and_matching_stores(
) -> Result<(), Box<dyn std::error::Error>> {
    let (home, deployment, writer) = fixture()?;
    let registry = ManagedDevices::open([(deployment.clone(), home.0.clone())])?;
    let configured = registry
        .get(&deployment.device)
        .ok_or("missing fixture managed device")?;
    let genesis = configured.anchor().checkpoint()?;
    assert_eq!((genesis.generation, genesis.sequence), (1, 0));
    assert!(configured.anchor().retained_checkpoint(&genesis).is_err());
    validate_managed_checkpoints(&configured, &genesis, &genesis, &genesis)?;

    let replaced = relay_admission::managed_images::Checkpoint {
        digest: "f".repeat(64),
        ..genesis.clone()
    };
    assert!(validate_managed_checkpoints(&configured, &replaced, &genesis, &genesis).is_err());
    assert!(validate_managed_checkpoints(&configured, &genesis, &genesis, &replaced).is_err());
    assert!(validate_managed_checkpoints(&configured, &replaced, &replaced, &replaced).is_err());
    let later = relay_admission::managed_images::Checkpoint {
        generation: 2,
        ..genesis.clone()
    };
    assert!(validate_managed_checkpoints(&configured, &later, &later, &later).is_err());
    // Losing the external authority cannot be repaired by matching store input.
    std::fs::rename(home.0.join("checkpoint.json"), home.0.join("retired.json"))?;
    assert!(validate_managed_checkpoints(&configured, &genesis, &genesis, &genesis).is_err());
    drop(writer);
    Ok(())
}

#[test]
fn bootstrap_binding_cannot_authorize_http_or_ws_effects() -> Result<(), Box<dyn std::error::Error>>
{
    let (home, deployment, writer) = fixture()?;
    let registry = ManagedDevices::open([(deployment.clone(), home.0.clone())])?;
    let configured = registry
        .get(&deployment.device)
        .ok_or("missing fixture managed device")?;
    let genesis = configured.anchor().checkpoint()?;
    let request = Request {
        namespace: deployment.namespace,
        nonce: vec![1; 32],
        managed: Some(protocol::runtime_binding::ManagedCheckpoint {
            generation: genesis.generation,
            sequence: genesis.sequence,
            digest: genesis.digest,
        }),
    };
    // The protocol permits this request only as an exact read-only proof.
    assert!(request.valid());
    assert_eq!(
        effect_checkpoint(&request),
        Err("managed bootstrap binding is read-only")
    );
    let omitted = Request {
        managed: None,
        ..request.clone()
    };
    assert_eq!(
        effect_checkpoint(&omitted),
        Err("managed runtime binding required")
    );
    let mut effect = request;
    effect
        .managed
        .as_mut()
        .ok_or("missing checkpoint")?
        .sequence = 1;
    assert_eq!(effect_checkpoint(&effect)?.sequence, 1);
    drop(writer);
    Ok(())
}

#[test]
fn original_upload_survives_unrelated_progress_but_not_rollback_or_retirement(
) -> Result<(), Box<dyn std::error::Error>> {
    let (home, deployment, mut writer) = fixture()?;
    let registry = ManagedDevices::open([(deployment.clone(), home.0.clone())])?;
    let configured = registry
        .get(&deployment.device)
        .ok_or("missing fixture managed device")?;
    let before = writer.reader().checkpoint()?;
    let original = writer.prepare(
        &event_log::sha256_hex(b"original publication"),
        &before,
        &before,
        false,
    )?;
    writer.reconcile(&original.next, &original.next)?;
    let later = writer.prepare(
        &event_log::sha256_hex(b"unrelated presentation"),
        &original.next,
        &original.next,
        false,
    )?;
    writer.reconcile(&later.next, &later.next)?;
    validate_managed_checkpoints(&configured, &original.next, &later.next, &later.next)?;
    let guard = configured.upload_guard(&original.next)?;
    let retirement = event_log::sha256_hex(b"retire managed generation");
    assert_eq!(
        writer.prepare(&retirement, &later.next, &later.next, true),
        Err(relay_admission::managed_images::AnchorError::Busy)
    );
    // Rolling both stores back to an original checkpoint still conflicts with
    // the independent external tip; retaining history is not restore authority.
    assert!(validate_managed_checkpoints(
        &configured,
        &original.next,
        &original.next,
        &original.next,
    )
    .is_err());
    assert!(
        validate_managed_checkpoints(&configured, &original.next, &later.next, &original.next,)
            .is_err()
    );
    drop(guard);
    let retired = writer.prepare(&retirement, &later.next, &later.next, true)?;
    writer.reconcile(&retired.next, &retired.next)?;
    assert!(validate_managed_checkpoints(
        &configured,
        &original.next,
        &retired.next,
        &retired.next,
    )
    .is_err());
    assert!(configured.upload_guard(&original.next).is_err());
    Ok(())
}
