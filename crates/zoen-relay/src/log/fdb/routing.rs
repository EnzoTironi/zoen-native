//! Ordinary cross-cell routing and its bounded queue admission.
use super::*;

impl FdbLog {
    pub(super) async fn append_routed(
        &self,
        env: &Envelope,
        target_known: bool,
    ) -> Result<Sequencing, Reject> {
        if env
            .device()
            .is_some_and(|device| self.managed_device(device).is_some())
        {
            return Err(Reject::retry("managed publication binding required"));
        }
        if !env.valid_invite() {
            return Err(Reject::no("invalid invite code"));
        }
        if !self.forwarding_available() {
            return Err(Reject::retry("cell forwarding unavailable"));
        }
        let _count = self
            .routing_count
            .clone()
            .try_acquire_owned()
            .map_err(|_| Reject::retry("relay routing capacity reached"))?;
        let _bytes = self
            .routing_bytes
            .clone()
            .try_acquire_many_owned(
                env.retained_len()
                    .saturating_add(1024)
                    .min(u32::MAX as usize) as u32,
            )
            .map_err(|_| Reject::retry("relay routing capacity reached"))?;
        tokio::time::timeout(std::time::Duration::from_secs(6), async {
            // At most one retry after a stale route or uncertain RPC result. The same
            // envelope/client_id reaches durable dedupe on either owner.
            for attempt in 0..2 {
                let fence = self
                    .owner
                    .resolve(env.space(), || self.forwarding_available())
                    .await?;
                let result = if fence.owner == self.owner.node {
                    self.append_fenced(env, target_known, fence).await
                } else if let Some(forwarder) = self.forwarding.get() {
                    forwarder.append(env, &fence).await
                } else {
                    return Err(Reject::retry(
                        "partition owner requires the cell forwarding bus",
                    ));
                };
                match result {
                    Err(r) if !r.permanent && attempt == 0 => continue,
                    result => return result,
                }
            }
            Err(Reject::retry("partition ownership is changing"))
        })
        .await
        .unwrap_or_else(|_| Err(Reject::retry("relay routing deadline exceeded")))
    }
}
