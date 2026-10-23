//! The existing native activation owns the review and its exact MLS capsule.
use super::*;
use crate::execution::Execution;

impl Staging<'_> {
    pub(super) async fn snapshot(&self) -> Result<foundationdb::Transaction, RuntimeError> {
        self.runtime.image_continuity()?;
        let execution = &self.runtime.execution;
        let trx = execution.transaction().await?;
        self.capability.check_in(execution, &trx).await?;
        let principal = self.credential.principal();
        Execution::lease(
            &trx,
            &execution.native_lease(&principal),
            self.fence,
            trx.get_read_version().await?,
        )
        .await?;
        if execution.native_root(&trx, &principal).await?.as_ref() != Some(&self.root) {
            return Err(RuntimeError::Denied);
        }
        let prior = trx.get(&self.key, false).await?;
        if prior.as_ref().map(|v| hash(v)) != self.prior {
            return Err(RuntimeError::InvalidBinding);
        }
        let head = execution
            .reply_source()?
            .head_in(&trx, self.space)
            .await
            .map_err(|_| RuntimeError::Denied)?
            .ok_or(RuntimeError::Denied)?;
        if head.seq() != self.head.seq || head.hash() != self.head.hash {
            return Err(RuntimeError::Denied);
        }
        Ok(trx)
    }
    pub(super) async fn activate(&mut self) -> Result<(), RuntimeError> {
        let transition = if self
            .review
            .as_ref()
            .is_some_and(|review| review.checkpoint.is_none() && review.accepted.is_none())
        {
            let envelope = self.envelope.as_ref().ok_or(RuntimeError::InvalidBinding)?;
            self.runtime
                .begin_image_publish_checkpoint(envelope)
                .await?
        } else {
            None
        };
        if let Some(transition) = &transition {
            self.review
                .as_mut()
                .ok_or(RuntimeError::InvalidBinding)?
                .checkpoint = Some(transition.checkpoint());
        }
        let principal = self.credential.principal();
        let image = self
            .core
            .image()
            .map_err(|_| RuntimeError::InvalidBinding)?;
        let next = self
            .custody
            .seal_image(&principal, Some(&self.root), &image)?;
        let staged = self
            .runtime
            .execution
            .stage_native(&principal, Some(self.root.clone()), self.fence, next)
            .await?;
        let root = staged.root.clone();
        let bytes = self
            .review
            .as_ref()
            .map(|review| {
                self.runtime
                    .custody
                    .seal(&subject(&self.key), review)
                    .map(|(_, bytes)| bytes)
            })
            .transpose()?;
        let mut scope = staged.scope(&self.runtime.execution, self.fence);
        scope.effects = match &bytes {
            Some(bytes) => hash(bytes),
            None => hash(
                &serde_json::to_vec(&(self.space, &self.head))
                    .map_err(|_| RuntimeError::InvalidBinding)?,
            ),
        };
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
            .image_checkpoint(transition.as_ref().map(|value| value.annotation()))
            .await?
            .authorize(scope.clone())
            .await?;
        admission.check(&scope)?;
        let trx = self.snapshot().await?;
        if let Some(accepted) = self
            .review
            .as_ref()
            .and_then(|review| review.accepted.as_ref())
        {
            let position = self
                .runtime
                .execution
                .reply_source()?
                .position_in(&trx, self.space, accepted.seq)
                .await
                .map_err(|_| RuntimeError::InvalidBinding)?;
            if Some(&position.env) != self.envelope.as_ref() || position.hash != accepted.hash {
                return Err(RuntimeError::InvalidBinding);
            }
        }
        self.runtime
            .execution
            .native_activation(&trx, &staged, self.fence)
            .await?;
        if let Some(bytes) = &bytes {
            trx.set(&self.key, bytes);
        }
        if let Some(transition) = &transition {
            transition
                .annotation()
                .write_fdb(&self.runtime.execution, &trx)
                .await?;
        }
        admission.consume(&scope)?;
        self.runtime.image_continuity()?;
        trx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
        if let Some(transition) = transition {
            transition.finish(self.runtime).await?;
        }
        self.root = root;
        self.prior = bytes.as_ref().map(|bytes| hash(bytes));
        Ok(())
    }
}
