//! Single-host ownership and bounded queue admission precede consumption.
use super::*;
use crate::log::{sequencer::ManagedAdmission, ManagedPublication};
use relay_admission::managed_images::Checkpoint;

impl FdbLog {
    pub(super) async fn append_registered_local(
        &self,
        env: &Envelope,
        target_known: bool,
        checkpoint: &Checkpoint,
    ) -> Result<ManagedPublication, Reject> {
        let configured = self
            .managed_device(env.device().unwrap_or_default())
            .ok_or_else(|| Reject::retry("unconfigured managed device"))?;
        if configured.deployment().agent != env.author() || !env.valid_invite() {
            return Err(Reject::no("invalid managed publication"));
        }
        let result = async {
            // Failed routes and full queues cannot burn original consumption.
            let fence = self
                .owner
                .resolve(env.space(), || self.forwarding_available())
                .await?;
            if fence.owner != self.owner.node {
                return Err(Reject::retry(
                    "managed publication requires the local owner",
                ));
            }
            match self.sequencer.admit_managed(
                env.clone(),
                target_known,
                fence,
                configured.anchor(),
                checkpoint,
            )? {
                ManagedAdmission::Queued(queued) => {
                    let (result, stats) = queued.append().await;
                    let span = tracing::Span::current();
                    span.record("batch", stats.size);
                    span.record("attempts", stats.attempts);
                    result.map(ManagedPublication::Sequenced)
                }
                ManagedAdmission::Consumed(condition) => self
                    .observed_managed_publication(&condition, env)
                    .await
                    .map_err(|_| Reject::retry("original managed publication is uncertain"))?
                    .map(ManagedPublication::Observed)
                    .ok_or_else(|| Reject::retry("original managed publication is uncertain")),
            }
        };
        tokio::time::timeout(std::time::Duration::from_secs(6), result)
            .await
            .unwrap_or_else(|_| Err(Reject::retry("managed publication deadline exceeded")))
    }
}
