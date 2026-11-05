//! Replay the authenticated original receipt; no grant, provider or network send.
use super::*;
use relay_admission::managed_images::Recovery;

pub(crate) async fn recover_shared_receipt(
    runtime: &RuntimeAuthority,
    managed: &mut ManagedRuntime,
    prepared: &Prepared,
) -> Result<(), RuntimeError> {
    let intent = Intent::decode(runtime, prepared)?;
    let mut sql = runtime.finance.continuity_transaction().await?;
    let sql_cp = managed.sql_checkpoint(&mut sql).await?;
    let tx = runtime.execution.transaction().await?;
    let fdb_cp = managed.fdb_checkpoint(runtime, &tx).await?;
    let actual = inventory(runtime, &mut sql, &tx).await?;
    let expected = managed
        .anchor
        .reader()
        .pending_subject_commitments(prepared)
        .map_err(crate::managed_images::error)?;
    let mut before = expected.clone();
    for update in &intent.updates {
        before.insert(
            update.subject.clone(),
            update
                .previous
                .clone()
                .ok_or(RuntimeError::InvalidBinding)?,
        );
    }
    intent.validate_updates(&before, &expected)?;
    let custody = runtime
        .native
        .as_ref()
        .ok_or(RuntimeError::CoreAuthorityUnavailable)?;
    let principal = &intent.target.context.principal;
    let credential = custody.credential(&principal.agent, &principal.device)?;
    if credential.principal() != *principal {
        return Err(RuntimeError::InvalidBinding);
    }
    if sql_cp == prepared.next && fdb_cp == prepared.next {
        if actual != expected {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let history = tx
            .get(
                &crate::continuity::history(&runtime.execution, prepared.next.sequence),
                false,
            )
            .await?
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        if serde_json::from_slice::<Prepared>(&history).map_err(|_| RuntimeError::InvalidBinding)?
            != *prepared
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let (record, publication, position) = intent.original(runtime, &tx).await?;
        let image = runtime
            .execution
            .read_native_in(&tx, principal, None)
            .await?;
        if image.root != intent.target {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        {
            let mut core = DeviceCore::restore(
                custody.open_image(principal, image)?,
                credential.unlocked()?,
            )
            .map_err(native_error)?;
            intent.verify_native(&mut core, &record, &publication, &position)?;
        }
        intent.apply_record(runtime, &tx, &publication).await?;
        sql.commit().await?;
    } else if fdb_cp == prepared.previous
        && (sql_cp == prepared.previous || sql_cp == prepared.next)
        && actual == before
    {
        drop(tx);
        drop(sql);
        let fence = runtime.execution.acquire_native_device(principal).await?;
        let result = async {
            let tx = runtime.execution.transaction().await?;
            let (record, publication, position) = intent.original(runtime, &tx).await?;
            let staged = intent.staged();
            let image = if let Some(staged) = &staged {
                let pin = tx
                    .get(&runtime.execution.image_activation_pin(principal), false)
                    .await?;
                // The independent original may precede the pin commit. Never
                // recreate a missing image; recover only its exact Ready stage.
                if pin.as_deref().is_some_and(|value| {
                    Some(value) != prepared.intent.as_deref().map(str::as_bytes)
                }) {
                    return Err(RuntimeError::RestoreUnreconciled);
                }
                runtime
                    .execution
                    .recover_image_stage(&tx, staged, &fence)
                    .await?
            } else {
                runtime
                    .execution
                    .read_native_in(&tx, principal, Some(&fence))
                    .await?
            };
            if image.root != intent.target {
                return Err(RuntimeError::InvalidBinding);
            }
            {
                let mut core = DeviceCore::restore(
                    custody.open_image(principal, image)?,
                    credential.unlocked()?,
                )
                .map_err(native_error)?;
                intent.verify_native(&mut core, &record, &publication, &position)?;
            }
            if let Some(staged) = &staged {
                runtime
                    .execution
                    .native_activation(&tx, staged, &fence)
                    .await?;
            }
            intent.apply_record(runtime, &tx, &publication).await?;
            let mut sql = runtime.finance.continuity_transaction().await?;
            if inventory(runtime, &mut sql, &tx).await? != expected {
                return Err(RuntimeError::RestoreUnreconciled);
            }
            if sql_cp == prepared.previous {
                managed.write_sql(&mut sql, prepared).await?;
            }
            sql.commit().await?;
            managed.write_fdb(runtime, &tx, prepared).await?;
            tx.clear(&runtime.execution.image_activation_pin(principal));
            tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
            Ok(())
        }
        .await;
        let released = runtime
            .execution
            .release_native_device(principal, &fence)
            .await;
        result?;
        released?;
    } else {
        return Err(RuntimeError::RestoreUnreconciled);
    }
    if managed
        .anchor
        .reconcile(&prepared.next, &prepared.next)
        .map_err(crate::managed_images::error)?
        != Recovery::BothAfter
    {
        return Err(RuntimeError::RestoreUnreconciled);
    }
    Ok(())
}
