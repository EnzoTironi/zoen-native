//! Pending-prefix admission, durable fork marking and settlement.
use super::{
    digest_meta, fold_roster, fork_meta, fork_reason, pending_prefix_meta, pending_refusal_meta,
    seen_against, signing_device, storage, tracing_like, BTreeMap, Engine, Event, Ingest,
    PendingPrefix, Seen, SeenAgainst, PENDING_PREFIX_CEILING, R,
};

impl Engine {
    /// One decision for a failed `accept` on the next sequence, whether the
    /// body arrived in the clear, sealed, or pruned. The caller already
    /// returned a gap when the event's own sequence is still ahead.
    pub(crate) fn refuse_unaccepted(
        &mut self,
        space: &str,
        err: &event_log::LogError,
        event: &Event,
    ) -> Ingest {
        let (seen, empty) = {
            let log = self.logs.get(space).expect("accept ran on this log");
            (seen_against(log, event), log.is_empty())
        };
        if empty {
            self.logs.remove(space);
        }
        let causal = matches!(err, event_log::LogError::BrokenCausalLink { .. });
        self.refused_member_history(space, causal, seen, &event.author, &err.to_string())
    }

    /// `causal` means the signature and the event's own chain link already
    /// passed, and the signed `seen` did not. The event is this device's next
    /// sequence. A member whose `seen` disagrees with a held position, or names
    /// a position this chain does not hold, is fork evidence. Anything else is
    /// a refusal. Neither result asks the relay for this event again.
    pub(crate) fn refused_member_history(
        &mut self,
        space: &str,
        causal: bool,
        seen: SeenAgainst,
        author: &str,
        err: &str,
    ) -> Ingest {
        if !causal {
            return Ingest::Invalid(err.to_string());
        }
        let fork = matches!(seen, SeenAgainst::Ahead | SeenAgainst::Diverged)
            && self.author_in_roster(space, author);
        if fork {
            if let Err(error) = self.mark_checkpoint_fork(space) {
                return Ingest::Invalid(error.to_string());
            }
        }
        Ingest::Invalid(err.to_string())
    }

    pub(super) fn observe_checkpoint(
        &mut self,
        e: &Event,
        epoch: u64,
        digest: &str,
        upto: &Seen,
    ) -> R<()> {
        if !self.author_in_roster(&e.space, &e.author) {
            return Ok(());
        }
        if self.digest_forks(&e.space, epoch, digest) || self.prefix_forks(&e.space, upto) {
            return self.mark_checkpoint_fork(&e.space);
        }
        if self.chain_hash_at(&e.space, upto.seq).is_none() {
            self.remember_pending_prefix(
                &e.space,
                signing_device(e),
                PendingPrefix {
                    seq: upto.seq,
                    hash: upto.hash.clone(),
                    epoch,
                    digest: digest.to_string(),
                },
            );
        }
        Ok(())
    }

    pub(super) fn settle_pending_prefixes(&mut self, space: &str) -> R<()> {
        let pending = self.pending_prefixes(space);
        if pending.is_empty() {
            return Ok(());
        }
        let started = pending.len();
        let mut left = BTreeMap::new();
        let mut fork = false;
        for (device, item) in pending {
            let Some(mine) = self.chain_hash_at(space, item.seq) else {
                left.insert(device, item);
                continue;
            };
            if mine != item.hash || self.digest_forks(space, item.epoch, &item.digest) {
                fork = true;
            }
        }
        if !fork && left.len() == started {
            return Ok(());
        }
        self.persist_settlement(space, &left, fork)?;
        if fork {
            self.show_fork(space, fork_reason());
        }
        Ok(())
    }

    pub(super) fn persist_settlement(
        &self,
        space: &str,
        pending: &BTreeMap<String, PendingPrefix>,
        fork: bool,
    ) -> R<()> {
        let tx = self.store.conn().unchecked_transaction().map_err(storage)?;
        if fork {
            self.store
                .set_meta(&fork_meta(space), &fork_reason())
                .map_err(storage)?;
            #[cfg(test)]
            if crash_after_fork_meta() {
                tx.commit().map_err(storage)?;
                return Err(storage("fork meta committed before pending removal"));
            }
        }
        let key = pending_prefix_meta(space);
        if pending.is_empty() {
            self.store.meta_delete(&key).map_err(storage)?;
        } else {
            let json = serde_json::to_string(pending).map_err(storage)?;
            self.store.set_meta(&key, &json).map_err(storage)?;
        }
        tx.commit().map_err(storage)?;
        Ok(())
    }

    pub(super) fn remember_pending_prefix(
        &mut self,
        space: &str,
        device: &str,
        item: PendingPrefix,
    ) {
        let mut pending = self.pending_prefixes(space);
        if let Some(existing) = pending.get(device) {
            if item.seq <= existing.seq {
                return;
            }
            pending.insert(device.to_string(), item);
            self.store_pending_prefixes(space, &pending);
            return;
        }
        if pending.len() >= PENDING_PREFIX_CEILING {
            self.note_prefix_refusal(space, device, item.seq);
            return;
        }
        pending.insert(device.to_string(), item);
        self.store_pending_prefixes(space, &pending);
    }

    pub(super) fn note_prefix_refusal(&self, space: &str, device: &str, seq: u64) {
        let count = self.pending_prefix_refusals(space).saturating_add(1);
        let _ = self
            .store
            .set_meta(&pending_refusal_meta(space), &count.to_string());
        tracing_like(&format!(
            "pending checkpoint prefix refused space={space} device={device} seq={seq} ceiling={PENDING_PREFIX_CEILING} count={count}"
        ));
    }

    /// How many new pending claims this Space has refused at the defensive ceiling.
    /// Stored at `mls.pending_prefix_refused:{space}`.
    pub(crate) fn pending_prefix_refusals(&self, space: &str) -> u64 {
        self.store
            .meta(&pending_refusal_meta(space))
            .ok()
            .flatten()
            .and_then(|raw| raw.parse().ok())
            .unwrap_or(0)
    }

    pub(super) fn author_in_roster(&self, space: &str, identity: &str) -> bool {
        self.logs
            .get(space)
            .is_some_and(|log| fold_roster(log.events().iter()).contains_key(identity))
    }

    pub(super) fn digest_forks(&self, space: &str, epoch: u64, digest: &str) -> bool {
        self.store
            .meta(&digest_meta(space, epoch))
            .ok()
            .flatten()
            .is_some_and(|mine| mine != digest)
    }

    pub(super) fn prefix_forks(&self, space: &str, upto: &Seen) -> bool {
        self.chain_hash_at(space, upto.seq)
            .is_some_and(|mine| mine != upto.hash)
    }

    pub(super) fn chain_hash_at(&self, space: &str, seq: u64) -> Option<String> {
        let event = self.logs.get(space)?.events().get(seq as usize)?;
        (event.seq == seq).then(|| event.hash.clone())
    }

    pub(super) fn pending_prefixes(&self, space: &str) -> BTreeMap<String, PendingPrefix> {
        self.store
            .meta(&pending_prefix_meta(space))
            .ok()
            .flatten()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    pub(super) fn store_pending_prefixes(
        &self,
        space: &str,
        pending: &BTreeMap<String, PendingPrefix>,
    ) {
        let key = pending_prefix_meta(space);
        let _ = if pending.is_empty() {
            self.store.meta_delete(&key)
        } else {
            self.store
                .set_meta(&key, &serde_json::to_string(pending).unwrap_or_default())
        };
    }

    pub(crate) fn recorded_fork(&self, space: &str) -> Option<String> {
        self.store.meta(&fork_meta(space)).ok().flatten()
    }

    pub(crate) fn fork_recorded(&self, space: &str) -> bool {
        self.recorded_fork(space).is_some()
    }

    pub(crate) fn restore_recorded_forks(&mut self) {
        let spaces = self.space_order.clone();
        for space in spaces {
            if let Some(reason) = self.recorded_fork(&space) {
                self.show_fork(&space, reason);
            }
        }
    }

    pub(super) fn mark_checkpoint_fork(&mut self, space: &str) -> R<()> {
        let reason = fork_reason();
        self.store
            .set_meta(&fork_meta(space), &reason)
            .map_err(storage)?;
        self.show_fork(space, reason);
        Ok(())
    }

    /// The phrase has to live on the projection. The next confirmed event
    /// rebuilds `state` from there and would otherwise drop it.
    pub(super) fn show_fork(&mut self, space: &str, reason: String) {
        if let Some(scoped) = self.space_projections.get_mut(space) {
            if let Some(s) = scoped.spaces.get_mut(space) {
                s.integrity_error = Some(reason.clone());
            }
        }
        if let Some(s) = self.state.spaces.get_mut(space) {
            s.integrity_error = Some(reason);
        }
    }
}
