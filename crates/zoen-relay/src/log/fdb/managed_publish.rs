//! A consumed external effect is a condition of the actual log transaction.
//! Neither a restored dedupe table nor a later checkpoint grants another send.
use super::*;
use relay_admission::managed_images::{Checkpoint, Prepared, PublishCondition};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Committed {
    condition: PublishCondition,
    space: String,
    client_id: String,
    sequence: u64,
    hash: String,
}

pub(super) struct ManagedAppend {
    key: Vec<u8>,
    condition: PublishCondition,
    committed: Option<Committed>,
}

impl ManagedAppend {
    pub(super) fn allows_new(&self) -> bool {
        self.committed.is_none()
    }

    /// Dedupe is observational only: its original event must still be present
    /// and byte-identical. A consumed effect with no event stays uncertain.
    pub(super) fn record(
        &self,
        transaction: &Transaction,
        event: &Sequenced,
        original: &Envelope,
    ) -> Result<(), Reject> {
        let same_envelope = event.env.wire_hash() == original.wire_hash()
            && event.env.author() == original.author()
            && event.env.device() == original.device()
            && event.env.space() == original.space()
            && event.env.client_id() == original.client_id();
        if !same_envelope
            || self.committed.as_ref().is_some_and(|old| {
                old.space != event.env.space()
                    || old.client_id != event.env.client_id()
                    || old.sequence != event.seq
                    || old.hash != event.hash
            })
        {
            return Err(Reject::retry("original managed publication is uncertain"));
        }
        let value = Committed {
            condition: self.condition.clone(),
            space: event.env.space().into(),
            client_id: event.env.client_id().into(),
            sequence: event.seq,
            hash: event.hash.clone(),
        };
        let bytes = serde_json::to_vec(&value).map_err(|_| Reject::unavailable())?;
        transaction.set(&self.key, &bytes);
        Ok(())
    }
}

impl Cell {
    pub(super) async fn managed_publication(
        &self,
        transaction: &Transaction,
        pending: &Pending,
    ) -> Result<Result<Option<ManagedAppend>, Reject>, FdbBindingError> {
        let Some(permit) = &pending.managed else {
            return Ok(Ok(None));
        };
        let refused = || Ok(Err(Reject::retry("managed publication refused")));
        if permit.check(&pending.env).is_err() {
            return refused();
        }
        let condition = permit.condition();
        let (prefix, cell): (String, String) =
            unpack(self.root.bytes()).map_err(|_| custom("invalid relay cell"))?;
        if prefix != "zoen"
            || cell != condition.source
            || condition.agent != pending.env.author()
            || Some(condition.device.as_str()) != pending.env.device()
            || !protocol::runtime_binding::valid_namespace(&condition.namespace)
        {
            return refused();
        }
        let root = Subspace::all().subspace(&("zoen-runtime-v1", condition.namespace.as_str()));
        // These are conflict reads in the SAME transaction as log/dedupe and
        // consumption writes. Restoring or advancing the root invalidates it.
        let current = transaction
            .get(&root.pack(&("managed-images/checkpoint/1",)), false)
            .await?;
        let history = transaction
            .get(
                &root.pack(&("managed-images/effect/1", condition.checkpoint.sequence)),
                false,
            )
            .await?;
        let (Some(current), Some(history)) = (current, history) else {
            return refused();
        };
        if current.len() > 4096
            || history.len() > 4096
            || event_log::sha256_hex(&history) != condition.checkpoint_record_digest
        {
            return refused();
        }
        let (Ok(current), Ok(history)) = (
            serde_json::from_slice::<Checkpoint>(&current),
            serde_json::from_slice::<Prepared>(&history),
        ) else {
            return refused();
        };
        if history.next != condition.checkpoint
            || history.retirement
            || history.effect_digest != condition.effect_digest
            || current.generation != condition.checkpoint.generation
            || current.sequence < condition.checkpoint.sequence
            || current.sequence == condition.checkpoint.sequence
                && current.digest != condition.checkpoint.digest
        {
            return refused();
        }
        if permit.check_store_checkpoint(&current).is_err() {
            return refused();
        }
        let key = root.pack(&(
            "managed-images/published/1",
            condition.effect_digest.as_str(),
        ));
        let committed = transaction.get(&key, false).await?;
        let committed = match committed {
            Some(bytes) if bytes.len() <= 16_384 => {
                match serde_json::from_slice::<Committed>(&bytes) {
                    Ok(value) if value.condition == *condition => Some(value),
                    _ => return refused(),
                }
            }
            Some(_) => return refused(),
            None => None,
        };
        Ok(Ok(Some(ManagedAppend {
            key,
            condition: condition.clone(),
            committed,
        })))
    }
}
