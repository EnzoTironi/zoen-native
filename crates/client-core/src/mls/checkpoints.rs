//! Checkpoint refresh, observation and durable outbound positions.
use super::*;

impl Engine {
    /// Checkpoints this device owes for being alive: in every group it is in, once a
    /// refresh interval has passed since its last one (the relay's ceiling counts from it).
    pub fn mls_refresh_checkpoints(&mut self) {
        let now = crate::engine::now_ms();
        let spaces: Vec<SpaceId> = self
            .net
            .synced
            .iter()
            .filter(|s| self.is_e2e(s))
            .cloned()
            .collect();
        for space in spaces {
            if !self.device().is_ok_and(|d| d.has_group(&space)) {
                continue;
            }
            let last: Option<i64> = self
                .store
                .meta(&checkpointed_ms_meta(&space))
                .ok()
                .flatten()
                .and_then(|v| v.parse().ok());
            // A group this device never checkpointed starts its clock now: its hold (from
            // when it was added) is fresh, and a device in many new groups must not
            // checkpoint them all at its next launch.
            let Some(last) = last else {
                let _ = self
                    .store
                    .set_meta(&checkpointed_ms_meta(&space), &now.to_string());
                continue;
            };
            if now - last >= checkpoint_refresh_ms() {
                self.net.mls.checkpoint_due.insert(space);
            }
        }
    }

    /// Another member's checkpoint. Two checks share one fork path.
    ///
    /// The epoch digest is compared with this device's `digest_meta` for that epoch,
    /// as before. `upto.hash` is compared with the chain hash at `upto.seq` once that
    /// position is in the log. Either mismatch stores `fork_meta` and the Space's
    /// `integrity_error`. A checkpoint whose author is not a current member of the
    /// Space log is ignored.
    ///
    /// When `upto.seq` is still ahead of the chain, the checkpoint is kept under
    /// `mls.pending_prefix:{space}` and checked again from here when a later event in
    /// that Space reaches it, including after this device reopens. One signing device
    /// keeps only its highest unreached `upto.seq`. A later claim from that device
    /// with a higher seq replaces it. A lower or equal seq is ignored. This relies on
    /// the hash chain: the hash at the higher position commits to every earlier one,
    /// so a divergence at or before the lower position still fails the higher check.
    /// A device that leaves the roster keeps the claim it already stored until that
    /// check runs. The list is bounded by the Space's member devices. Past
    /// [`PENDING_PREFIX_CEILING`] entries a new device is refused. The refusal count
    /// is `mls.pending_prefix_refused:{space}`, read by [`Engine::pending_prefix_refusals`].
    /// Nothing already stored is dropped. Settling writes `fork_meta` and then the
    /// pending list in one store transaction, so a crash keeps the fork or the claim.
    /// Once the chain includes the seq, the entry is removed whether the prefix
    /// matches or forks. A match with no stored digest for the epoch is removed too.
    /// The digest check does not wait for a later epoch record.
    ///
    /// A checkpoint that never reaches this function can still be fork evidence.
    /// `seen` is protobuf field 7 of the signed content (`seq` and `hash`). The
    /// author signature covers those bytes. The relay's `seq`, `prev` and chain hash
    /// do not. When that signed `seen` names a position this device holds with a
    /// different hash, ingest marks the fork and leaves the event out of the log.
    /// A member event that is already the next sequence is the same fork when
    /// that `seen` is not held: the relay assigns the sequence after the author
    /// signed, so an honest `seen` on this chain is below the event. An event
    /// whose own sequence is still ahead of the local head stays a gap.
    pub(crate) fn compare_checkpoint(&mut self, e: &Event) -> R<()> {
        if let EventBody::Checkpoint {
            epoch,
            digest,
            upto,
        } = &e.body
        {
            if self.me.as_deref() != Some(e.author.as_str()) {
                self.observe_checkpoint(e, *epoch, digest, upto)?;
            }
        }
        self.settle_pending_prefixes(&e.space)
    }

    pub(super) fn checkpointed_at(&mut self, space: &str) -> u64 {
        if let Some(at) = self.net.mls.checkpointed_at.get(space) {
            return *at;
        }
        let at = self
            .store
            .meta(&checkpointed_meta(space))
            .ok()
            .flatten()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        self.net.mls.checkpointed_at.insert(space.to_string(), at);
        at
    }

    /// Posts the checkpoints this device owes. `true` when it queued any.
    pub fn mls_checkpoints(&mut self) -> R<bool> {
        let due: Vec<SpaceId> = self.net.mls.checkpoint_due.drain().collect();
        let mut queued = false;
        for space in due {
            let Some(upto) = self.logs.get(&space).and_then(|l| l.head()) else {
                continue;
            };
            let (epoch, digest) = match self
                .device()
                .and_then(|d| d.checkpoint(&space).map_err(mls_err))
            {
                Ok(c) => c,
                Err(_) => continue,
            };
            let me = self.me_id()?;
            let reached = upto.seq;
            self.append(
                &space,
                &me,
                EventBody::Checkpoint {
                    upto,
                    epoch,
                    digest,
                },
            )?;
            let _ = self
                .store
                .set_meta(&checkpointed_meta(&space), &reached.to_string());
            let _ = self.store.set_meta(
                &checkpointed_ms_meta(&space),
                &crate::engine::now_ms().to_string(),
            );
            self.net.mls.checkpointed_at.insert(space, reached);
            queued = true;
        }
        Ok(queued)
    }
}
