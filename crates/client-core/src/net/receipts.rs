//! Bounded unvalidated receipts on the current socket.

use super::{DeferredReceipts, Engine, HashSet};

impl DeferredReceipts {
    pub(crate) fn receive_time(
        &mut self,
        engine: &mut Engine,
        space: &str,
        from: &str,
        report: &protocol::read_time::ReadTime,
    ) -> Result<bool, crate::CoreError> {
        if !report.valid() {
            return Ok(false);
        }
        if engine.receipt_scope(space, from)? {
            return engine.receive_read_time(space, from, report, super::super::engine::now_ms());
        }
        if engine.receipt_confirmed_removed(space, from)? {
            self.1
                .retain(|(s, peer, _, _), _| s != space || peer != from);
            return Ok(false);
        }
        let key = (
            space.to_owned(),
            from.to_owned(),
            report.after_seq,
            report.through_seq,
        );
        if space.is_empty()
            || from.is_empty()
            || space.len() > 128
            || from.len() > 128
            || (self.1.len() >= 1024 && !self.1.contains_key(&key))
        {
            return Err(crate::CoreError::Invalid {
                reason: "read time catch-up buffer exceeded".into(),
            });
        }
        self.1
            .entry(key)
            .or_insert_with(|| (report.clone(), super::super::engine::now_ms()));
        Ok(false)
    }
    pub(crate) fn device_message(
        &mut self,
        engine: &mut Engine,
        from: &str,
        to: &str,
        sealed: &[u8],
        dirty: &mut HashSet<String>,
    ) -> Result<(), crate::CoreError> {
        engine.device_message(from, to, sealed);
        // Opened history can release membership held behind a waiting approval.
        self.flush(engine, dirty)
    }

    pub(crate) fn receive(
        &mut self,
        engine: &mut Engine,
        space: &str,
        from: &str,
        delivered: u64,
        read: u64,
    ) -> Result<bool, crate::CoreError> {
        if engine.receipt_scope(space, from)? {
            return engine.receipt(space, from, delivered, read);
        }
        // Explicit native/Web correction: a known signed removal is a cut,
        // not unresolved membership that can authorize this arrival on rejoin.
        if engine.receipt_confirmed_removed(space, from)? {
            self.0.remove(&(space.into(), from.into()));
            return Ok(false);
        }
        if delivered == 0 && read == 0 {
            return Ok(false);
        }
        // Space IDs are bounded to 128 bytes by the push protocol; identity IDs
        // are 64-byte hex. Preserve that upper bound for deferred frame strings.
        let key = (space.to_string(), from.to_string());
        if space.is_empty()
            || space.len() > 128
            || from.is_empty()
            || from.len() > 128
            || (self.0.len() >= 1024 && !self.0.contains_key(&key))
        {
            // The caller reconnects and catches up: overflow never silently
            // discards a potentially legitimate latest watermark.
            return Err(crate::CoreError::Invalid {
                reason: "receipt catch-up buffer exceeded".into(),
            });
        }
        let mark = self.0.entry(key).or_default();
        *mark = (mark.0.max(delivered).max(read), mark.1.max(read));
        Ok(false)
    }

    pub(crate) fn flush(
        &mut self,
        engine: &mut Engine,
        dirty: &mut HashSet<String>,
    ) -> Result<(), crate::CoreError> {
        let mut ready = Vec::new();
        let mut discarded = Vec::new();
        for (space, from) in self.0.keys() {
            if engine.receipt_scope(space, from)? {
                ready.push((space.clone(), from.clone()));
            } else if engine.receipt_confirmed_removed(space, from)? {
                discarded.push((space.clone(), from.clone()));
            }
        }
        for key in discarded {
            self.0.remove(&key);
        }
        for (space, from) in ready {
            let (delivered, read) = self.0[&(space.clone(), from.clone())];
            if engine.receipt(&space, &from, delivered, read)? {
                dirty.insert(space.clone());
            }
            self.0.remove(&(space, from));
        }
        let keys: Vec<_> = self.1.keys().cloned().collect();
        for key in keys {
            if engine.receipt_scope(&key.0, &key.1)? {
                let (report, observed) = &self.1[&key];
                if engine.receive_read_time(&key.0, &key.1, report, *observed)? {
                    dirty.insert(key.0.clone());
                }
                self.1.remove(&key);
            } else if engine.receipt_confirmed_removed(&key.0, &key.1)? {
                self.1.remove(&key);
            }
        }
        // SyncDone also acknowledges partial gap/join requests, so it cannot
        // prove unrelated membership complete. Keep unresolved pairs bounded
        // until canonical history admits them or this socket is discarded.
        Ok(())
    }
}
