use super::*;
enum NativeEffect<'a> {
    Maintenance(Option<SharedSource>),
    Discovery(Batch),
    Publication(&'a runs::output::LivePublication<'a>),
    Tool(&'a runs::reviews::LiveToolDecision<'a>),
}
impl NativeCustody {
    pub(in crate::native) async fn activate_tool_decision(
        &self,
        runtime: &RuntimeAuthority,
        credential: &Credential,
        staged: storage::Staged,
        fence: &crate::execution::Fence,
        live: &runs::reviews::LiveToolDecision<'_>,
    ) -> Result<(), RuntimeError> {
        self.activate_native(runtime, credential, staged, fence, NativeEffect::Tool(live))
            .await?;
        Ok(())
    }
    pub(in crate::native) async fn activate_maintenance(
        &self,
        runtime: &RuntimeAuthority,
        credential: &Credential,
        staged: storage::Staged,
        fence: &crate::execution::Fence,
        source: Option<SharedSource>,
    ) -> Result<Root, RuntimeError> {
        self.activate_native(
            runtime,
            credential,
            staged,
            fence,
            NativeEffect::Maintenance(source),
        )
        .await
        .map(|(root, _)| root)
    }
    pub(in crate::native) async fn activate_discovery(
        &self,
        runtime: &RuntimeAuthority,
        credential: &Credential,
        staged: storage::Staged,
        fence: &crate::execution::Fence,
        batch: Batch,
    ) -> Result<(Root, Vec<crate::ReplyRun>), RuntimeError> {
        batch.scope(&runtime.execution, &staged, fence)?;
        self.activate_native(
            runtime,
            credential,
            staged,
            fence,
            NativeEffect::Discovery(batch),
        )
        .await
    }
    pub(in crate::native) async fn activate_shared_reply(
        &self,
        runtime: &RuntimeAuthority,
        credential: &Credential,
        staged: storage::Staged,
        fence: &crate::execution::Fence,
        live: &runs::output::LivePublication<'_>,
    ) -> Result<crate::ReplyOutputInspection, RuntimeError> {
        let inspection = live.inspection(&staged.root);
        self.activate_native(
            runtime,
            credential,
            staged,
            fence,
            NativeEffect::Publication(live),
        )
        .await?;
        Ok(inspection)
    }
    async fn activate_native(
        &self,
        runtime: &RuntimeAuthority,
        credential: &Credential,
        staged: storage::Staged,
        fence: &crate::execution::Fence,
        effect: NativeEffect<'_>,
    ) -> Result<(Root, Vec<crate::ReplyRun>), RuntimeError> {
        let (source, batch, live, tool) = match effect {
            NativeEffect::Maintenance(source) => (source, None, None, None),
            NativeEffect::Discovery(batch) => (
                Some(SharedSource {
                    cell: batch.source.clone(),
                    space: batch.space.clone(),
                    expected: batch.head.clone(),
                    through: batch.through.clone(),
                }),
                Some(batch),
                None,
                None,
            ),
            NativeEffect::Publication(live) => {
                (Some(live.source(runtime)?), None, Some(live), None)
            }
            NativeEffect::Tool(tool) => (Some(tool.source(runtime)?), None, None, Some(tool)),
        };
        let mut intent = Intent {
            format: 1,
            previous: staged.previous.clone(),
            target: staged.root.clone(),
            source,
            discovery: None,
            publication: None,
            tool: None,
        };
        let Some(shared) = &runtime.runtime_continuity else {
            if batch.is_some() || live.is_some() || tool.is_some() {
                return Err(RuntimeError::CoreAuthorityUnavailable);
            }
            let scope = intent.scope(runtime, fence)?;
            let admission = runtime
                .finance
                .native_directory(
                    &credential.agent,
                    &credential.owner,
                    &credential.principal().device,
                    &credential.certificate,
                )
                .await?
                .authorize(scope.clone())
                .await?;
            let tx = runtime.execution.transaction().await?;
            verify_source(runtime, &tx, intent.source.as_ref(), true).await?;
            runtime
                .execution
                .native_activation(&tx, &staged, fence)
                .await?;
            admission.consume(&scope)?;
            tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
            return Ok((staged.root, Vec::new()));
        };
        let mut managed = shared.lock().await;
        Box::pin(managed.reconcile(runtime)).await?;
        let mut sql = runtime.finance.continuity_transaction().await?;
        let previous = managed.sql_checkpoint(&mut sql).await?;
        let tx = runtime.execution.transaction().await?;
        if managed.fdb_checkpoint(runtime, &tx).await? != previous
            || managed
                .anchor
                .reader()
                .checkpoint()
                .map_err(crate::managed_images::error)?
                != previous
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let before = inventory(runtime, &mut sql, &tx).await?;
        if before
            != managed
                .anchor
                .reader()
                .subject_commitments()
                .map_err(crate::managed_images::error)?
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let subject = intent.subject()?;
        if before.get(&subject)
            != intent
                .previous
                .as_ref()
                .map(root_digest)
                .transpose()?
                .as_ref()
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        if let Some(batch) = batch {
            let discovery = Discovery::retain(runtime, &staged.root, batch).await?;
            discovery.require_capacity(runtime, &tx).await?;
            intent.discovery = Some(discovery);
        }
        if let Some(live) = live {
            live.verify(runtime, &tx).await?;
            intent.publication = Some(
                live.retain(
                    runtime,
                    &staged.root,
                    &managed.deployment.namespace,
                    previous
                        .sequence
                        .checked_add(1)
                        .ok_or(RuntimeError::InvalidBinding)?,
                    &before,
                )?,
            );
        }
        if let Some(tool) = tool {
            tool.verify(runtime, &tx).await?;
            intent.tool = Some(
                tool.retain(
                    runtime,
                    &staged.root,
                    &managed.deployment.namespace,
                    previous
                        .sequence
                        .checked_add(1)
                        .ok_or(RuntimeError::InvalidBinding)?,
                    &before,
                )?,
            );
        }
        let scope = intent.scope(runtime, fence)?;
        let updates = intent.updates()?;
        if updates
            .iter()
            .skip(1)
            .any(|update| before.get(&update.subject) != update.previous.as_ref())
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        let sealed = intent.seal(runtime)?;
        let prepared = managed
            .anchor
            .prepare_subjects(
                &intent.effect()?,
                &previous,
                &previous,
                false,
                &updates,
                Some(&sealed),
            )
            .map_err(crate::managed_images::error)?;
        drop(sql);
        runtime
            .execution
            .pin_image_activation(&tx, &staged, fence, sealed.as_bytes())
            .await?;
        tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        #[cfg(test)]
        if runtime.fault.load(std::sync::atomic::Ordering::SeqCst)
            == if tool.is_some() { 181 } else { 90 }
        {
            return Err(RuntimeError::Unavailable);
        }
        let guard = runtime
            .finance
            .native_directory(
                &credential.agent,
                &credential.owner,
                &credential.principal().device,
                &credential.certificate,
            )
            .await?;
        let guard = if live.is_some() || tool.is_some() {
            guard.active_reply_actor().await?
        } else {
            guard
        };
        #[cfg(test)]
        let guard = if live.is_some() {
            match runtime.fault.load(std::sync::atomic::Ordering::SeqCst) {
                45 => guard.suppress_known_ack(),
                46 => guard.delay_known_ack(),
                _ => guard,
            }
        } else {
            guard
        };
        let admission = guard
            .shared_native_checkpoint(&managed, &prepared, &scope, false)
            .await?
            .authorize_before(
                scope.clone(),
                intent
                    .publication
                    .as_ref()
                    .and_then(|value| value.expires())
                    .or_else(|| intent.tool.as_ref().and_then(|value| value.expires())),
            )
            .await?;
        #[cfg(test)]
        if runtime.fault.load(std::sync::atomic::Ordering::SeqCst)
            == if tool.is_some() { 182 } else { 91 }
        {
            return Err(RuntimeError::Unavailable);
        }
        let tx = runtime.execution.transaction().await?;
        verify_source(runtime, &tx, intent.source.as_ref(), true).await?;
        if let Some(live) = live {
            live.verify(runtime, &tx).await?;
        }
        if let Some(tool) = tool {
            tool.verify(runtime, &tx).await?;
        }
        runtime
            .execution
            .native_activation(&tx, &staged, fence)
            .await?;
        let runs = if let Some(discovery) = &intent.discovery {
            discovery
                .apply(runtime, &tx, &staged.root, true)
                .await?
                .runs
        } else {
            Vec::new()
        };
        if intent.publication.is_some() || intent.tool.is_some() {
            let capsule = runtime
                .execution
                .read_native_in(&tx, &credential.principal(), Some(fence))
                .await?;
            let mut core = DeviceCore::restore(
                self.open_image(&credential.principal(), capsule)?,
                credential.unlocked()?,
            )
            .map_err(|_| RuntimeError::InvalidBinding)?;
            if let Some(publication) = &intent.publication {
                publication
                    .apply(
                        runtime,
                        &tx,
                        &staged.root,
                        publication.retained(&runtime.custody, &core)?,
                        &managed.deployment.namespace,
                        &prepared,
                    )
                    .await?;
            }
            if let Some(tool) = &intent.tool {
                tool.apply(
                    runtime,
                    &tx,
                    &staged.root,
                    &mut core,
                    &managed.deployment.namespace,
                    &prepared,
                )
                .await?;
            }
        }
        managed.write_fdb(runtime, &tx, &prepared).await?;
        tx.clear(
            &runtime
                .execution
                .image_activation_pin(&credential.principal()),
        );
        if (live.is_some() || tool.is_some())
            && inventory(
                runtime,
                &mut runtime.finance.continuity_transaction().await?,
                &tx,
            )
            .await?
                != managed
                    .anchor
                    .reader()
                    .pending_subject_commitments(&prepared)
                    .map_err(crate::managed_images::error)?
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        admission.consume(&scope)?;
        tx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        #[cfg(test)]
        if runtime.fault.load(std::sync::atomic::Ordering::SeqCst)
            == if tool.is_some() { 183 } else { 92 }
        {
            return Err(RuntimeError::Unavailable);
        }
        Box::pin(managed.reconcile(runtime)).await?;
        #[cfg(test)]
        if live.is_some() && runtime.fault.load(std::sync::atomic::Ordering::SeqCst) == 150 {
            return Err(RuntimeError::Unavailable);
        }
        Ok((staged.root, runs))
    }
}
