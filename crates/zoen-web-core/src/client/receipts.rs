//! Receipt scope comes only from already verified, confirmed signed logs.
//! The optimistic outbox is absent from this fold. The connection-local queue
//! keeps only unresolved pairs; a confirmed removal discards new arrivals and
//! queued pairs without deleting previously authorized historical saved marks.
use super::*;
const MAX_DEFERRED: usize = 1024;
impl<S: Signer> Client<S> {
    fn receipt_removed(&self, space: &str, who: &str, roster: &BTreeMap<String, Role>) -> bool {
        !roster.contains_key(who)
            && self.logs.get(space).is_some_and(|log| {
                log.events().iter().rev().any(|event|
            matches!(&event.body,EventBody::MemberRemoved{identity}if identity==who))
            })
    }
    fn store_receipt(&mut self, space: &str, from: &str, delivered: u64, read: u64) {
        let mark = self
            .saved
            .receipts
            .entry(space.into())
            .or_default()
            .entry(from.into())
            .or_default();
        *mark = (mark.0.max(delivered).max(read), mark.1.max(read));
    }
    pub(super) fn receive_receipt(
        &mut self,
        space: &str,
        from: &str,
        delivered: u64,
        read: u64,
    ) -> Result<(), String> {
        let roster = self.roster(space);
        let key = (space.to_string(), from.to_string());
        if roster.contains_key(&self.account.identity) && roster.contains_key(from) {
            self.store_receipt(space, from, delivered, read);
            return Ok(());
        }
        if self.receipt_removed(space, &self.account.identity, &roster)
            || self.receipt_removed(space, from, &roster)
        {
            self.deferred_receipts.remove(&key);
            return Ok(());
        }
        if delivered == 0 && read == 0 {
            return Ok(());
        }
        if space.is_empty()
            || space.len() > 128
            || from.is_empty()
            || from.len() > 128
            || (self.deferred_receipts.len() >= MAX_DEFERRED
                && !self.deferred_receipts.contains_key(&key))
        {
            return Err("receipt catch-up buffer exceeded".into());
        }
        let mark = self.deferred_receipts.entry(key).or_default();
        *mark = (mark.0.max(delivered).max(read), mark.1.max(read));
        Ok(())
    }
    pub(super) fn flush_receipts(&mut self) {
        // Move, never copy, the bounded queue. Keep at most one derived roster at
        // a time; no per-Space table snapshots or parallel membership authority.
        let pending = std::mem::take(&mut self.deferred_receipts);
        let mut scope = String::new();
        let mut roster = BTreeMap::new();
        for ((space, from), (delivered, read)) in pending {
            if space != scope {
                roster = self.roster(&space);
                scope = space.clone();
            }
            if roster.contains_key(&self.account.identity) && roster.contains_key(&from) {
                self.store_receipt(&space, &from, delivered, read);
            } else if !self.receipt_removed(&space, &self.account.identity, &roster)
                && !self.receipt_removed(&space, &from, &roster)
            {
                self.deferred_receipts
                    .insert((space, from), (delivered, read));
            }
        }
    }
}
