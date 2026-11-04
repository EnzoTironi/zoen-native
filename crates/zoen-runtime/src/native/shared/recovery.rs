//! Recover only the authenticated original capsule; no model or send is entered.
use super::*;

pub(crate) async fn recover_shared_native(
    runtime: &RuntimeAuthority,
    managed: &mut ManagedRuntime,
    prepared: &Prepared,
) -> Result<(), RuntimeError> {
    let encoded = prepared
        .intent
        .as_deref()
        .and_then(|value| value.strip_prefix(PREFIX))
        .ok_or(RuntimeError::RestoreUnreconciled)?;
    if encoded.len() > 16_384 {
        return Err(RuntimeError::EvidenceTooLarge);
    }
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| RuntimeError::InvalidBinding)?;
    let intent: Intent = runtime
        .custody
        .open("runtime-native-maintenance/1", None, &bytes)?;
    if intent.format != 1
        || prepared.retirement
        || prepared.effect_digest != intent.effect()?
        || prepared.subject_updates != intent.updates()?
    {
        return Err(RuntimeError::RestoreUnreconciled);
    }
    let custody = runtime
        .native
        .as_ref()
        .ok_or(RuntimeError::CoreAuthorityUnavailable)?;
    let principal = &intent.target.context.principal;
    let credential = custody.credential(&principal.agent, &principal.device)?;
    if credential.principal() != *principal {
        return Err(RuntimeError::InvalidBinding);
    }
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
    if sql_cp == prepared.next && fdb_cp == prepared.next {
        let key = crate::continuity::history(&runtime.execution, prepared.next.sequence);
        let bytes = tx
            .get(&key, false)
            .await?
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        if serde_json::from_slice::<Prepared>(&bytes).map_err(|_| RuntimeError::InvalidBinding)?
            != *prepared
            || actual != expected
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let capsule = runtime.execution.read_native(principal, None).await?;
        if capsule.root != intent.target {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        custody.open_image(principal, capsule)?;
        sql.commit().await?;
        if managed
            .anchor
            .reconcile(&sql_cp, &fdb_cp)
            .map_err(crate::managed_images::error)?
            != Recovery::BothAfter
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        return Ok(());
    }
    let mut prior = expected.clone();
    for update in &prepared.subject_updates {
        match &update.previous {
            Some(value) => {
                prior.insert(update.subject.clone(), value.clone());
            }
            None => {
                prior.remove(&update.subject);
            }
        }
    }
    if (sql_cp != prepared.previous && sql_cp != prepared.next)
        || fdb_cp != prepared.previous
        || actual != prior
    {
        return Err(RuntimeError::RestoreUnreconciled);
    }
    drop(sql);
    drop(tx);
    let fence = runtime.execution.acquire_native_device(principal).await?;
    let result = async {
        let tx = runtime.execution.transaction().await?;
        let pin = runtime.execution.image_activation_pin(principal);
        let sealed = prepared
            .intent
            .as_deref()
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        if tx
            .get(&pin, false)
            .await?
            .as_deref()
            .is_some_and(|value| value != sealed.as_bytes())
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let staged = intent.staged();
        let image = runtime
            .execution
            .recover_image_stage(&tx, &staged, &fence)
            .await?;
        let core = DeviceCore::restore(
            custody.open_image(principal, image)?,
            credential.unlocked()?,
        )
        .map_err(|_| RuntimeError::InvalidBinding)?;
        if let Some(source) = &intent.source {
            if core
                .retained_head(&source.space)
                .map_err(|_| RuntimeError::InvalidBinding)?
                .as_ref()
                != Some(&source.through)
            {
                return Err(RuntimeError::InvalidBinding);
            }
        }
        verify_source(runtime, &tx, intent.source.as_ref(), false).await?;
        let scope = intent.scope(runtime, &fence)?;
        let admission = runtime
            .finance
            .native_directory(
                &credential.agent,
                &credential.owner,
                &principal.device,
                &credential.certificate,
            )
            .await?
            .shared_native_checkpoint(managed, prepared, &scope, true)
            .await?
            .authorize(scope.clone())
            .await?;
        runtime
            .execution
            .native_activation(&tx, &staged, &fence)
            .await?;
        if let Some(discovery) = &intent.discovery {
            discovery.apply(runtime, &tx, &intent.target, false).await?;
        }
        managed.write_fdb(runtime, &tx, prepared).await?;
        tx.clear(&pin);
        admission.consume(&scope)?;
        tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
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
    .await;
    let released = runtime
        .execution
        .release_native_device(principal, &fence)
        .await;
    result?;
    released
}
