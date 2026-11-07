//! Apply actual relay history without discovering/scheduling another chat reply.
use super::*;
use types::Seen;

pub(crate) struct NativeSourceRefresh {
    pub(crate) through: Seen,
    pub(crate) caught_up: bool,
}

impl NativeCustody {
    pub(crate) async fn refresh_source(
        &self,
        runtime: &RuntimeAuthority,
        agent: &str,
        device: &str,
        space: &str,
    ) -> Result<NativeSourceRefresh, RuntimeError> {
        runtime.check_shared_runtime().await?;
        if !id(space) {
            return Err(RuntimeError::InvalidBinding);
        }
        let credential = self.credential(agent, device)?;
        let principal = credential.principal();
        runtime
            .finance
            .native_directory(
                &credential.agent,
                &credential.owner,
                device,
                &credential.certificate,
            )
            .await?
            .commit()
            .await?;
        runtime.execution.collect_native_stage(&principal).await?;
        runtime.collect_reply_retired(&principal).await?;
        let fence = runtime.execution.acquire_native_device(&principal).await?;
        let result = Box::pin(async {
            let capsule = runtime
                .execution
                .read_native(&principal, Some(&fence))
                .await?;
            let previous = capsule.root.clone();
            let mut core = Box::new(
                DeviceCore::restore(
                    self.open_image(&principal, capsule)?,
                    credential.unlocked()?,
                )
                .map_err(|_| RuntimeError::InvalidBinding)?,
            );
            let retained = core
                .retained_head(space)
                .map_err(|_| RuntimeError::InvalidBinding)?;
            let source = runtime.execution.reply_source()?;
            let batch = source
                .read_in(
                    &runtime.execution.transaction().await?,
                    space,
                    retained.as_ref().map(|h| (h.seq, h.hash.as_str())),
                )
                .await
                .map_err(|_| RuntimeError::Denied)?;
            let expected = Seen {
                seq: batch.head.seq(),
                hash: batch.head.hash().into(),
            };
            let changed = !batch.entries.is_empty();
            core.ingest(batch.entries)
                .map_err(|_| RuntimeError::InvalidBinding)?;
            let through = core
                .retained_head(space)
                .map_err(|_| RuntimeError::InvalidBinding)?
                .ok_or(RuntimeError::Denied)?;
            let progress = NativeSourceRefresh {
                caught_up: through == expected,
                through,
            };
            if !changed {
                runtime
                    .finance
                    .native_directory(
                        &credential.agent,
                        &credential.owner,
                        device,
                        &credential.certificate,
                    )
                    .await?
                    .commit()
                    .await?;
                return Ok(progress);
            }
            let image = core.image().map_err(|_| RuntimeError::InvalidBinding)?;
            drop(core);
            let next = self.seal_image(&principal, Some(&previous), &image)?;
            let staged = runtime
                .execution
                .stage_native(&principal, Some(previous), &fence, next)
                .await?;
            self.activate_maintenance(
                runtime,
                &credential,
                staged,
                &fence,
                Some(shared::SharedSource {
                    cell: source.cell().into(),
                    space: space.into(),
                    expected,
                    through: progress.through.clone(),
                }),
            )
            .await?;
            Ok(progress)
        })
        .await;
        let released = runtime
            .execution
            .release_native_device(&principal, &fence)
            .await;
        match result {
            Err(error) => Err(error),
            Ok(progress) => {
                released?;
                Ok(progress)
            }
        }
    }
}
