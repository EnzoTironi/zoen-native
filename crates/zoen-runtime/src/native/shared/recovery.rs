//! Recover only the authenticated original capsule; no model or send is entered.
use super::*;

pub(crate) async fn recover_shared_native(
    runtime: &RuntimeAuthority,
    managed: &mut ManagedRuntime,
    prepared: &Prepared,
) -> Result<(), RuntimeError> {
    let intent = open_intent(&runtime.custody, prepared)?;
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
        let mut core = DeviceCore::restore(
            custody.open_image(principal, capsule)?,
            credential.unlocked()?,
        )
        .map_err(|_| RuntimeError::InvalidBinding)?;
        if let Some(publication) = &intent.publication {
            publication
                .apply(
                    runtime,
                    &tx,
                    &intent.target,
                    publication.retained(&runtime.custody, &core)?,
                    &managed.deployment.namespace,
                    prepared,
                )
                .await?;
        }
        if let Some(tool) = &intent.tool {
            tool.apply(
                runtime,
                &tx,
                &intent.target,
                &mut core,
                &managed.deployment.namespace,
                prepared,
            )
            .await?;
        }
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
        let mut core = DeviceCore::restore(
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
            .authorize_before(
                scope.clone(),
                intent
                    .publication
                    .as_ref()
                    .and_then(|value| value.expires()),
            )
            .await?;
        runtime
            .execution
            .native_activation(&tx, &staged, &fence)
            .await?;
        if let Some(discovery) = &intent.discovery {
            discovery.apply(runtime, &tx, &intent.target, false).await?;
        }
        if let Some(publication) = &intent.publication {
            publication
                .apply(
                    runtime,
                    &tx,
                    &intent.target,
                    publication.retained(&runtime.custody, &core)?,
                    &managed.deployment.namespace,
                    prepared,
                )
                .await?;
        }
        if let Some(tool) = &intent.tool {
            tool.apply(
                runtime,
                &tx,
                &intent.target,
                &mut core,
                &managed.deployment.namespace,
                prepared,
            )
            .await?;
        }
        managed.write_fdb(runtime, &tx, prepared).await?;
        if intent.publication.is_some() || intent.tool.is_some() {
            let mut sql = runtime.finance.continuity_transaction().await?;
            if inventory(runtime, &mut sql, &tx).await? != expected {
                return Err(RuntimeError::RestoreUnreconciled);
            }
            sql.commit().await?;
        }
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
