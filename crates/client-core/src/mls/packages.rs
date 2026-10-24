//! Key-package publication and refused-handshake recovery.
use super::{
    identity_set, join_set, mls_err, rewelcome_meta, storage, tracing_like, welcome_meta, Engine,
    Envelope, EventBody, KeyPackageBatch, SealedKind, KEY_PACKAGES, META_PUBLISHED, R,
};

impl Engine {
    // ── work for the network task ──

    /// This device's key packages, until the relay has them: the first full set with a
    /// last-resort one, then top-ups when the relay says the stock is low.
    pub fn mls_key_packages_to_publish(&mut self) -> Option<KeyPackageBatch> {
        let device_id = self.net.account.as_ref()?.device.clone();
        if self.net.mls.publishing.is_none() {
            let first = self.store.meta(META_PUBLISHED).ok().flatten() != Some(device_id);
            let device = self.device().ok()?;
            if first {
                let mut packages = device.key_packages(KEY_PACKAGES, true).ok()?;
                let last_resort = packages.pop()?;
                self.net.mls.publishing = Some((packages, Some(last_resort)));
            } else if self.net.mls.top_up > 0 {
                let packages = device.key_packages(self.net.mls.top_up, false).ok()?;
                tracing_like(&format!("key packages generated count={}", packages.len()));
                self.net.mls.top_up = 0;
                self.net.mls.publishing = Some((packages, None));
            }
        }
        self.net.mls.publishing.clone()
    }

    /// The relay says this device has `remaining` single-use packages left: refill to
    /// [`KEY_PACKAGES`]. Notices for the identity's other devices are theirs.
    pub fn mls_key_packages_low(&mut self, device: &str, remaining: u32) {
        let mine = self
            .net
            .account
            .as_ref()
            .is_some_and(|a| a.device == device);
        tracing_like(&format!(
            "key packages low remaining={remaining} mine={mine} publishing={}",
            self.net.mls.publishing.is_some()
        ));
        if mine && self.net.mls.publishing.is_none() {
            self.net.mls.top_up = KEY_PACKAGES.saturating_sub(remaining as usize);
        }
    }

    pub fn mls_key_packages_published(&mut self, result: Result<(), String>) {
        tracing_like(&format!("key packages published result={result:?}"));
        match result {
            Ok(()) => {
                if let Some(a) = &self.net.account {
                    let _ = self.store.set_meta(META_PUBLISHED, &a.device);
                }
                self.net.mls.publishing = None;
            }
            Err(e) => tracing_like(&format!("key packages refused: {e}")),
        }
    }

    /// One of this device's handshakes was refused for good: a commit that lost its epoch
    /// (`STALE_COMMIT`), or a Welcome whose commit isn't the last one any more. Neither is
    /// anything the person wrote, so it isn't shown as failed: a lost commit is dropped with
    /// its Welcome, the winner arrives from the log, and the group is checked again.
    /// `false` when `client_id` isn't a handshake.
    pub(crate) fn mls_handshake_refused(&mut self, client_id: &str, reason: &str) -> R<bool> {
        let Some(e) = self
            .store
            .outbox()?
            .into_iter()
            .map(|p| p.event)
            .find(|e| e.client_id == client_id)
        else {
            return Ok(false);
        };
        let EventBody::Sealed { kind } = &e.body else {
            return Ok(false);
        };
        if kind != SealedKind::Commit.name() && kind != SealedKind::Welcome.name() {
            return Ok(false);
        }
        tracing_like(&format!("{kind} in {} refused: {reason}", e.space));
        let dropped = (|| -> R<Vec<String>> {
            let tx = self.store.conn().unchecked_transaction().map_err(storage)?;
            let mut dropped = vec![client_id.to_string()];
            if kind == SealedKind::Commit.name() {
                // Its Welcome, held behind it, has no commit to follow any more.
                let orphans: Vec<String> = self
                .store
                .outbox()
                .map_err(storage)?
                .into_iter()
                .map(|p| p.event)
                .filter(|o| {
                    o.space == e.space
                        && matches!(&o.body, EventBody::Sealed { kind } if kind == SealedKind::Welcome.name())
                })
                .map(|o| o.client_id)
                .collect();
                for id in orphans {
                    self.store.outbox_remove(&id)?;
                    self.store.meta_delete(&welcome_meta(&id))?;
                    dropped.push(id);
                }
                let device = self.device_on(&tx)?;
                if device.has_group(&e.space) {
                    device.abandon(&e.space).map_err(mls_err)?;
                } else {
                    device.forget(&e.space).map_err(mls_err)?;
                }
                if let Some(reference) = Envelope::plain(&e)
                    .recovery()
                    .and_then(event_log::recovery::RecoveryRef::parse)
                {
                    self.store.clear_recovery_upload(&reference.blob)?;
                }
            } else if kind == SealedKind::Welcome.name() {
                // Its commit landed but another got in before the Welcome: the newcomers' leaves
                // are in our group with no way in, so they get taken out and added afresh.
                let key = welcome_meta(client_id);
                let stranded = identity_set(self.store.meta(&key)?);
                self.store.meta_delete(&key)?;
                if !stranded.is_empty() {
                    let key = rewelcome_meta(&e.space);
                    let mut all = identity_set(self.store.meta(&key)?);
                    all.extend(stranded);
                    self.store.set_meta(&key, &join_set(&all))?;
                }
            }
            self.store.outbox_remove(client_id)?;
            tx.commit().map_err(storage)?;
            Ok(dropped)
        })();
        let dropped = match dropped {
            Ok(ids) => ids,
            Err(err) => {
                // This is still a handshake. Preserve its whole transaction for retry;
                // treating it as a failed user event would strand the pending group.
                tracing_like(&format!("dropping the handshake in {}: {err}", e.space));
                return Err(err);
            }
        };
        for id in dropped {
            self.net.pending.remove(&id);
        }
        self.net.mls.mark_group_dirty(e.space);
        Ok(true)
    }
}
