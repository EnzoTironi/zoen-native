//! Durable peer receipt admission, projection and replay.

use super::*;

impl Engine {
    pub(crate) fn delivery(&self, e: &Entry) -> Delivery {
        let source = e.client_id.strip_suffix(":result").unwrap_or(&e.client_id);
        // Confirmed remote aliases retain their own receipt; only unsequenced
        // optimistic rows inherit the local outbox status.
        if e.hash.is_empty() && self.net.failed.contains_key(source) {
            Delivery::Failed
        } else if e.hash.is_empty() && self.net.pending.get(source) == Some(&e.space) {
            Delivery::Sending
        } else if self.net.synced.contains(&e.space) || self.net.receipts.contains_key(&e.space) {
            self.receipt_for(e)
        } else {
            Delivery::Local
        }
    }

    /// ✓, ✓✓ or read for one of my messages: the lowest watermark across everyone else in
    /// the Space (a group shows read only when all have read, like WhatsApp).
    pub(super) fn receipt_for(&self, e: &Entry) -> Delivery {
        let me = self.me.as_deref().unwrap_or_default();
        let Some(space) = self.state.spaces.get(&e.space) else {
            return Delivery::Sent;
        };
        if e.author != me {
            return Delivery::Sent;
        }
        let marks = self.net.receipts.get(&e.space);
        let mut others = space
            .members
            .iter()
            .map(|(m, _)| m)
            .filter(|m| *m != me)
            .peekable();
        if others.peek().is_none() {
            return Delivery::Sent;
        }
        let (mut delivered, mut read) = (true, true);
        for m in others {
            let (d, r) = marks.and_then(|x| x.get(m)).copied().unwrap_or((0, 0));
            read &= r >= e.seq;
            delivered &= d.max(r) >= e.seq;
        }
        match (read, delivered) {
            (true, _) => Delivery::Read,
            (_, true) => Delivery::Delivered,
            _ => Delivery::Sent,
        }
    }

    pub(super) fn receipt_members(&self, space: &str) -> R<Cow<'_, [(IdentityId, Role)]>> {
        let pending_membership = self.net.pending.values().any(|s| s == space)
            && self.store.outbox()?.iter().any(|pending| {
                !pending.failed
                    && pending.event.space == space
                    && matches!(
                        pending.event.body,
                        EventBody::SpaceCreated { .. }
                            | EventBody::MemberAdded { .. }
                            | EventBody::MemberRemoved { .. }
                    )
                    && !self.logs.get(space).is_some_and(|log| {
                        log.events()
                            .iter()
                            .any(|e| e.client_id == pending.event.client_id)
                    })
            });
        if !pending_membership {
            return Ok(Cow::Borrowed(
                self.state
                    .spaces
                    .get(space)
                    .map(|s| s.members.as_slice())
                    .unwrap_or_default(),
            ));
        }
        // Optimistic membership is for UI feedback, not admission of relay marks.
        // Reuse the confirmed authority projection only while such an overlay exists.
        let mut confirmed = State::default();
        let mut plans = PlanReplay::new(&self.store, &self.logs);
        if let Some(log) = self.logs.get(space) {
            for (end, event) in log.events().iter().enumerate() {
                let context = plans.before(&confirmed, &self.identities, event, end)?;
                confirmed.apply_checked(event, &self.identities, &context);
            }
        }
        Ok(Cow::Owned(
            confirmed
                .spaces
                .remove(space)
                .map(|s| s.members)
                .unwrap_or_default(),
        ))
    }

    pub(crate) fn receipt_scope(&self, space: &str, from: &str) -> R<bool> {
        let Some(me) = self.me.as_deref() else {
            return Ok(false);
        };
        let members = self.receipt_members(space)?;
        Ok(members.iter().any(|(member, _)| member == me)
            && members.iter().any(|(member, _)| member == from))
    }

    /// A signed, checked canonical membership transition has removed the reader
    /// or subject. Outbox/UI optimism cannot erase this cut or resurrect a mark.
    /// This is only evaluated for unresolved/unauthorized arrivals, not hot valid
    /// receipts. It reuses the existing checked projection and PlanReplay.
    pub(crate) fn receipt_confirmed_removed(&self, space: &str, from: &str) -> R<bool> {
        let Some(me) = self.me.as_deref() else {
            return Ok(false);
        };
        let Some(log) = self.logs.get(space) else {
            return Ok(false);
        };
        let mut confirmed = State::default();
        let mut plans = PlanReplay::new(&self.store, &self.logs);
        let mut removed = [false, false];
        for (end, event) in log.events().iter().enumerate() {
            let present = |state: &State, who: &str| {
                state
                    .spaces
                    .get(space)
                    .is_some_and(|room| room.members.iter().any(|(id, _)| id == who))
            };
            let before = [present(&confirmed, me), present(&confirmed, from)];
            let context = plans.before(&confirmed, &self.identities, event, end)?;
            confirmed.apply_checked(event, &self.identities, &context);
            for (index, who) in [me, from].into_iter().enumerate() {
                let after = present(&confirmed, who);
                if after {
                    removed[index] = false;
                } else if before[index] {
                    removed[index] = true;
                }
            }
        }
        Ok(removed.into_iter().any(|value| value))
    }

    pub(super) fn restore_peer_receipts(&mut self) -> R<()> {
        // Older demo databases kept sample ticks in one special metadata blob.
        // Migrate only canonical members, atomically, then use the ordinary cache.
        if let Some(legacy) = self.store.meta("seed_receipts")? {
            let storage_error = |error: rusqlite::Error| CoreError::Storage {
                message: error.to_string(),
            };
            let tx = self
                .store
                .conn()
                .unchecked_transaction()
                .map_err(storage_error)?;
            if let Ok(legacy) =
                serde_json::from_str::<HashMap<SpaceId, HashMap<IdentityId, (u64, u64)>>>(&legacy)
            {
                for (space, peers) in legacy {
                    for (peer, (delivered, read)) in peers {
                        if self.me.as_deref() != Some(&peer)
                            && self.receipt_scope(&space, &peer)?
                            && delivered.max(read) > 0
                        {
                            self.store
                                .raise_peer_receipt(&space, &peer, delivered, read)?;
                        }
                    }
                }
            }
            self.store.meta_delete("seed_receipts")?;
            tx.commit().map_err(storage_error)?;
        }
        let saved = self.store.peer_receipts()?;
        let mut receipts: HashMap<SpaceId, HashMap<IdentityId, (u64, u64)>> = HashMap::new();
        for (space, peers) in &self.net.receipts {
            for (peer, mark) in peers {
                if self.me.as_deref() != Some(peer) && self.receipt_scope(space, peer)? {
                    receipts
                        .entry(space.clone())
                        .or_default()
                        .insert(peer.clone(), *mark);
                }
            }
        }
        for row in saved {
            if self.me.as_deref() != Some(&row.identity)
                && self.receipt_scope(&row.space, &row.identity)?
            {
                let mark = receipts
                    .entry(row.space)
                    .or_default()
                    .entry(row.identity)
                    .or_insert((0, 0));
                *mark = (
                    mark.0.max(row.delivered).max(row.read),
                    mark.1.max(row.read),
                );
            }
        }
        self.net.receipts = receipts;
        Ok(())
    }

    /// A current member's receipt arrived (live or replayed). Persist before publishing
    /// a higher mark so reconnect or cold/offline reopen cannot lower observed ticks.
    /// From my own identity, a read on another device is a read here too.
    pub fn receipt(&mut self, space: &str, from: &str, delivered: u64, read: u64) -> R<bool> {
        if !self.receipt_scope(space, from)? {
            return Ok(false);
        }
        if self.me.as_deref() == Some(from) {
            if read == 0 {
                return Ok(false);
            }
            let key = format!("read:{space}");
            let previous = self.store.meta(&key)?.and_then(|v| v.parse::<u64>().ok());
            if previous.is_some_and(|seq| seq >= read) {
                return Ok(false);
            }
            self.store.set_meta(&key, &read.to_string())?;
            return Ok(true);
        }
        let previous = self
            .net
            .receipts
            .get(space)
            .and_then(|peers| peers.get(from))
            .copied()
            .unwrap_or((0, 0));
        let next = (previous.0.max(delivered).max(read), previous.1.max(read));
        if next == previous {
            return Ok(false);
        }
        let saved = self.store.raise_peer_receipt(space, from, next.0, next.1)?;
        self.net
            .receipts
            .entry(space.to_string())
            .or_default()
            .insert(from.to_string(), saved);
        Ok(saved != previous)
    }

    /// The `Delivered` watermark to announce for `space`, when its head moved past the last
    /// one announced and the head is someone else's message.
    pub fn delivered_to_announce(&self, space: &str) -> Option<u64> {
        let me = self.me.clone().unwrap_or_default();
        let s = self.state.spaces.get(space)?;
        let head = s
            .entries
            .iter()
            .filter(|e| e.author != me)
            .map(|e| e.seq)
            .max()?;
        let last = self.net.delivered_sent.get(space).copied().unwrap_or(0);
        (head > last).then_some(head)
    }

    /// Advance only after the socket accepted the frame. Reconnect still replays it:
    /// a successful write alone cannot prove the relay received it.
    pub fn delivered_announced(&mut self, space: &str, seq: u64) {
        let last = self
            .net
            .delivered_sent
            .entry(space.to_string())
            .or_default();
        *last = (*last).max(seq);
    }

    /// Reconstruct ephemeral receipts from local history after authenticated catch-up.
    /// Read marks are durable, including reads made while the network task was stopped.
    pub fn receipts_to_replay(&mut self) -> Vec<(String, protocol::EphemeralKind)> {
        self.net.delivered_sent.clear();
        let mut frames = Vec::new();
        for space in &self.net.synced {
            if !self.state.spaces.contains_key(space) {
                continue;
            }
            if let Some(seq) = self.delivered_to_announce(space) {
                frames.push((space.clone(), protocol::EphemeralKind::Delivered { seq }));
            }
            if let Some(seq) = self.read_up_to(space).filter(|seq| *seq > 0) {
                frames.push((space.clone(), protocol::EphemeralKind::Read { seq }));
            }
        }
        frames
    }
}
