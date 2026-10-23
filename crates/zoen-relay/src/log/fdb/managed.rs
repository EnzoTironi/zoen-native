//! Single-host managed append. Its queue owns the generation through commit.
use super::*;

impl FdbLog {
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
