//! Single-host managed append. Its queue owns the generation through commit.
use super::*;

impl FdbLog {
    pub(super) async fn append_consumed_local(
        &self,
        env: &Envelope,
        target_known: bool,
        permit: relay_admission::managed_images::ManagedPublishPermit,
    ) -> Result<Sequencing, Reject> {
        permit
            .check(env)
            .map_err(|_| Reject::retry("managed publication refused"))?;
        if !env.valid_invite() {
            return Err(Reject::no("invalid invite code"));
        }
        let result = async {
            let fence = self
                .owner
                .resolve(env.space(), || self.forwarding_available())
                .await?;
            if fence.owner != self.owner.node {
                return Err(Reject::retry(
                    "managed publication requires the local owner",
                ));
            }
            let (result, stats) = self
                .sequencer
                .append_managed_publish(env.clone(), target_known, fence, permit)
                .await;
            let span = tracing::Span::current();
            span.record("batch", stats.size);
            span.record("attempts", stats.attempts);
            result
        };
        tokio::time::timeout(std::time::Duration::from_secs(6), result)
            .await
            .unwrap_or_else(|_| Err(Reject::retry("managed publication deadline exceeded")))
    }

    pub(super) async fn append_managed_local(
        &self,
        env: &Envelope,
        target_known: bool,
        generation: relay_admission::managed_images::GenerationGuard,
    ) -> Result<Sequencing, Reject> {
        // Initial managed custody is explicitly single-host. Never forward a
        // process-local file-lock capability through an ordinary cell RPC.
        let result = async {
            let fence = self
                .owner
                .resolve(env.space(), || self.forwarding_available())
                .await?;
            if fence.owner != self.owner.node {
                return Err(Reject::retry(
                    "managed publication requires the local owner",
                ));
            }
            self.append_fenced_guarded(env, target_known, fence, Some(Arc::new(generation)))
                .await
        };
        tokio::time::timeout(std::time::Duration::from_secs(6), result)
            .await
            .unwrap_or_else(|_| Err(Reject::retry("managed publication deadline exceeded")))
    }
}
