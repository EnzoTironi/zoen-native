//! Existing capsule/Publication transaction owns upload and acceptance progress.
use super::*;
use crate::execution::Execution;

impl Delivery<'_> {
    pub(super) async fn snapshot(&self) -> Result<foundationdb::Transaction, RuntimeError> {
        self.runtime.image_continuity()?;
        let execution = &self.runtime.execution;
        let trx = execution.transaction().await?;
        self.capability.check_in(execution, &trx).await?;
        let principal = &self.record.original.context.principal;
        Execution::lease(
            &trx,
            &execution.native_lease(principal),
            self.fence,
            trx.get_read_version().await?,
        )
        .await?;
        if execution.native_root(&trx, principal).await?.as_ref() != Some(&self.current) {
            return Err(RuntimeError::Denied);
        }
        let prior = trx
            .get(&execution.run(self.run, "image-output/1"), false)
            .await?
            .ok_or(RuntimeError::InvalidBinding)?;
        if hash(&prior) != self.publication_digest {
            return Err(RuntimeError::InvalidBinding);
        }
        let head = execution
            .reply_source()?
            .head_in(&trx, self.envelope.space())
            .await
            .map_err(|_| RuntimeError::Denied)?
            .ok_or(RuntimeError::Denied)?;
        if head.seq() != self.source_head.seq || head.hash() != self.source_head.hash {
            return Err(RuntimeError::Denied);
        }
        Ok(trx)
    }

    pub(super) async fn authorize(&self) -> Result<(), RuntimeError> {
        self.core
            .image_output_file(
                &self.record.task.request_id,
                &self.record.task.approval_hash,
                &self.envelope,
                self.runtime.finance.reply_clock().await?,
            )
            .map_err(|_| RuntimeError::Denied)?;
        self.runtime
            .finance
            .native_directory(
                &self.credential.agent,
                &self.credential.owner,
                &self.record.original.context.principal.device,
                &self.credential.certificate,
            )
            .await?
            .commit()
            .await?;
        drop(self.snapshot().await?);
        Ok(())
    }

    pub(super) async fn activate(&mut self) -> Result<(), RuntimeError> {
        let principal = &self.record.original.context.principal;
        let image = self
            .core
            .image()
            .map_err(|_| RuntimeError::InvalidBinding)?;
        let next = self
            .custody
            .seal_image(principal, Some(&self.current), &image)?;
        let staged = self
            .runtime
            .execution
            .stage_native(principal, Some(self.current.clone()), self.fence, next)
            .await?;
        let root = staged.root.clone();
        let (_, bytes) = self.runtime.custody.seal(
            &format!("native-image-output/1/{}", self.run),
            &self.publication,
        )?;
        let mut scope = staged.scope(&self.runtime.execution, self.fence);
        scope.effects = hash(&bytes);
        let admission = self
            .runtime
            .finance
            .native_directory(
                &self.credential.agent,
                &self.credential.owner,
                &principal.device,
                &self.credential.certificate,
            )
            .await?
            .authorize(scope.clone())
            .await?;
        admission.check(&scope)?;
        let trx = self.snapshot().await?;
        if let Some(accepted) = &self.publication.accepted {
            let position = self
                .runtime
                .execution
                .reply_source()?
                .position_in(&trx, self.envelope.space(), accepted.seq)
                .await
                .map_err(|_| RuntimeError::InvalidBinding)?;
            if position.env != self.envelope || position.hash != accepted.hash {
                return Err(RuntimeError::InvalidBinding);
            }
        }
        self.runtime
            .execution
            .native_activation(&trx, &staged, self.fence)
            .await?;
        trx.set(
            &self.runtime.execution.run(self.run, "image-output/1"),
            &bytes,
        );
        admission.consume(&scope)?;
        self.runtime.image_continuity()?;
        trx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        self.current = root;
        self.publication_digest = hash(&bytes);
        Ok(())
    }
}
