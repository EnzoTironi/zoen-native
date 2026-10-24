//! Bounded unvalidated receipts on the current socket.

use super::{DeferredReceipts, Engine, HashSet, SinkExt};

impl DeferredReceipts {
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
        // SyncDone also acknowledges partial gap/join requests, so it cannot
        // prove unrelated membership complete. Keep unresolved pairs bounded
        // until canonical history admits them or this socket is discarded.
        Ok(())
    }
}
