//! Compare authorization to the log head and cancellation in the append's
//! actual conflict-reading transaction. No plaintext grant enters the relay.
use super::*;
impl Cell {
    pub(super) async fn check_source_fence(
        &self,
        transaction: &Transaction,
        pending: &Pending,
        head: &Option<(u64, String)>,
    ) -> Result<Result<(), Reject>, FdbBindingError> {
        let Some(permit) = &pending.managed else {
            return Ok(Ok(()));
        };
        let Some(fence) = permit.source_fence() else {
            return Ok(Ok(()));
        };
        if !fence.valid()
            || fence.expires_at_ms.is_some_and(|expiry| expiry <= now_ms())
            || fence.space != pending.env.space()
            || head
                .as_ref()
                .is_none_or(|(seq, hash)| *seq != fence.head.seq || *hash != fence.head.hash)
        {
            return Ok(Err(Reject::retry("managed authorization source changed")));
        }
        let root =
            Subspace::all().subspace(&("zoen-runtime-v1", permit.condition().namespace.as_str()));
        for key in [
            root.pack(&("run", fence.run.as_str(), "cancel")),
            root.pack(&("attempt", fence.attempt.as_str(), "closed")),
        ] {
            if transaction.get(&key, false).await?.is_some() {
                return Ok(Err(Reject::retry("managed run authorization ended")));
            }
        }
        Ok(Ok(()))
    }
}
