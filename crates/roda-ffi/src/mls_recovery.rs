//! Recoverable public MLS contexts, encrypted to identity agreement keys (ADR 0047).
//! Group state, upload dependencies and signed commits share one SQLite transaction.

use roda_log::content::{Sealed, SealedKind};
use roda_log::recovery::{self, RecoveryRef};
use roda_mls::{recovery::verify_group_info, Commit, SUITE_ID};
use roda_proto::Envelope;
use roda_types::{Event, Role};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};

use crate::engine::{now_ms, Engine, R};
use crate::CoreError;

fn invalid(reason: impl std::fmt::Display) -> CoreError {
    CoreError::Invalid {
        reason: reason.to_string(),
    }
}

fn context_meta(space: &str) -> String {
    // Device-independent: backup keeps this authenticated reference, but no MLS state.
    format!("recovery:{space}")
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ContextRef {
    pub space: String,
    pub epoch: u64,
    pub blob: String,
    pub device: String,
}

impl Engine {
    pub(crate) fn recovery_recipients(&mut self, space: &str) -> R<Option<Vec<(String, String)>>> {
        // Preserve interoperability with relays and members predating agreement keys.
        if !self.net.profiles.supported {
            return Ok(None);
        }
        let Some(agreement) = &self.net.profiles.agreement else {
            return Ok(None);
        };
        let me = self.me.as_deref().ok_or_else(|| invalid("no account"))?;
        let mine = agreement.public_hex();
        let mut recipients = Vec::new();
        let mut waiting = false;
        let mut legacy = false;
        for identity in self.roster(space) {
            if identity == me {
                recipients.push((identity, mine.clone()));
            } else if let Some(key) = self.store.peer_agreement_key(&identity)? {
                recipients.push((identity, key));
            } else if self.net.profiles.no_agreement.contains(&identity) {
                legacy = true;
            } else {
                self.net.profiles.need_agreement.insert(identity);
                waiting = true;
            }
        }
        if waiting {
            return Err(invalid("waiting for member recovery keys"));
        }
        Ok((!legacy).then_some(recipients))
    }

    /// The caller owns the transaction, including the staged cryptographic state.
    pub(crate) fn store_recovery_commit(
        &self,
        space: &str,
        c: Commit,
        recipients: Option<&[(String, String)]>,
    ) -> R<Vec<Event>> {
        let mut sealed = Sealed::new(SealedKind::Commit, SUITE_ID, c.commit);
        if let Some(recipients) = recipients {
            let author = self
                .net
                .author
                .as_ref()
                .ok_or_else(|| invalid("no account"))?;
            let device = author.key.id();
            let summary =
                verify_group_info(space, &c.group_info, Some(&device)).map_err(invalid)?;
            if !summary.identities.is_subset(&self.roster(space)) {
                return Err(invalid("recovery context includes a removed member"));
            }
            let keys: Vec<(&str, &str)> = recipients
                .iter()
                .map(|(id, key)| (id.as_str(), key.as_str()))
                .collect();
            let bytes = recovery::seal(space, summary.epoch, &device, &c.group_info, &keys)
                .ok_or_else(|| invalid("cannot encrypt recovery context"))?;
            let blob = hex::encode(Sha256::digest(&bytes));
            self.store.queue_recovery_upload(&blob, &bytes, now_ms())?;
            sealed.recovery = RecoveryRef {
                version: 1,
                epoch: summary.epoch,
                blob,
            }
            .encode();
        }
        let mut events = vec![self.handshake_event(space, sealed)?];
        if let Some(welcome) = c.welcome {
            events.push(
                self.handshake_event(space, Sealed::new(SealedKind::Welcome, SUITE_ID, welcome))?,
            );
        }
        for event in &events {
            self.store.outbox_put(event)?;
        }
        Ok(events)
    }

    pub(crate) fn remember_recovery_context(&self, env: &Envelope) -> R<()> {
        if env.legacy_pruned_hash().is_some() {
            // Even the apparent Commit kind is unauthenticated on an old stub.
            return Ok(());
        }
        if env.sealed_kind() != Some(SealedKind::Commit) {
            if env.recovery().is_some() {
                return Err(invalid("only commits carry recovery contexts"));
            }
            return Ok(());
        }
        let Some(bytes) = env.recovery() else {
            self.store.meta_delete(&context_meta(env.space()))?;
            self.store.invalidate_recovery_blob(env.space(), None)?;
            return Ok(());
        };
        let r = RecoveryRef::parse(bytes).ok_or_else(|| invalid("invalid recovery reference"))?;
        if !env.is_pruned()
            && env
                .sealed_data()
                .and_then(|(_, data)| roda_mls::commit_epoch(data))
                .and_then(|e| e.checked_add(1))
                != Some(r.epoch)
        {
            return Err(invalid(
                "recovery reference does not match its commit epoch",
            ));
        }
        let reference = ContextRef {
            space: env.space().into(),
            epoch: r.epoch,
            blob: r.blob,
            device: env.device().unwrap_or(env.author()).into(),
        };
        self.store.set_meta(
            &context_meta(env.space()),
            &serde_json::to_string(&reference).map_err(invalid)?,
        )?;
        self.store
            .invalidate_recovery_blob(env.space(), Some(&reference.blob))?;
        Ok(())
    }

    fn recovery_context(&self, space: &str) -> Option<ContextRef> {
        serde_json::from_str(&self.store.meta(&context_meta(space)).ok()??).ok()
    }

    fn can_recover(&self, space: &str) -> bool {
        self.is_e2e(space)
            && self.state.spaces.get(space).is_some_and(|s| {
                s.members.iter().any(|(id, role)| {
                    Some(id.as_str()) == self.me.as_deref() && *role != Role::Reader
                })
            })
    }

    /// Downloads are bounded and requested one at a time, only for missing groups.
    pub(crate) fn wanted_recovery_contexts(&self) -> Vec<ContextRef> {
        if self.net.profiles.agreement.is_none() {
            return Vec::new();
        }
        let Ok(device) = self.device() else {
            return Vec::new();
        };
        self.net
            .synced
            .iter()
            .filter(|s| self.can_recover(s) && !device.has_group(s))
            .filter_map(|s| self.recovery_context(s))
            .filter(|r| {
                self.store
                    .recovery_blob(&r.space, &r.blob)
                    .ok()
                    .flatten()
                    .is_none()
            })
            .collect()
    }

    pub(crate) fn recovery_context_arrived(&self, reference: &ContextRef, bytes: &[u8]) -> bool {
        if bytes.len() > recovery::MAX_RECOVERY_BLOB_BYTES
            || hex::encode(Sha256::digest(bytes)) != reference.blob
        {
            return false;
        }
        if !self.recovery_context(&reference.space).is_some_and(|r| {
            r.blob == reference.blob && r.epoch == reference.epoch && r.device == reference.device
        }) {
            return false;
        }
        let ok = self
            .store
            .cache_recovery_blob(&reference.space, &reference.blob, bytes)
            .is_ok();
        if ok {
            self.net.wake();
        }
        ok
    }

    /// Runs after ordered catch-up. A restart resends the exact staged commit; messages
    /// stay held by the persisted recovery marker until the log confirms that commit.
    pub(crate) fn mls_recovery_pass(&mut self) -> R<bool> {
        let spaces: Vec<String> = self
            .net
            .synced
            .iter()
            .filter(|s| self.can_recover(s))
            .cloned()
            .collect();
        let mut error = None;
        for space in spaces {
            if self
                .net
                .mls
                .recovery_retry_at
                .get(&space)
                .is_some_and(|at| *at > Instant::now())
            {
                continue;
            }
            match self.try_mls_recovery(&space) {
                Ok(true) => {
                    self.net.mls.recovery_retry_at.remove(&space);
                    return Ok(true);
                }
                Ok(false) => {}
                Err(err) => {
                    self.net
                        .mls
                        .recovery_retry_at
                        .insert(space, Instant::now() + Duration::from_secs(5));
                    error = Some(err);
                }
            }
        }
        error.map_or(Ok(false), Err)
    }

    fn try_mls_recovery(&mut self, space: &str) -> R<bool> {
        if self
            .store
            .outbox_handshakes()?
            .iter()
            .any(|(_, s, _)| s == space)
        {
            return Ok(false);
        }
        let device = self.device()?;
        if device.pending(space) {
            return Ok(false);
        }
        if device.has_group(space) {
            // New and upgraded groups need a context even if nobody is being added.
            if self.recovery_context(space).is_none()
                && self.ready_epoch(space).is_some()
                && self.recovery_recipients(space)?.is_some()
            {
                self.commit_now(space, &[], &Default::default())?;
                return Ok(true);
            }
            return Ok(false);
        }
        let Some(reference) = self.recovery_context(space) else {
            return Ok(false);
        };
        let Some(blob) = self.store.recovery_blob(space, &reference.blob)? else {
            return Ok(false);
        };
        let agreement = self
            .net
            .profiles
            .agreement
            .as_ref()
            .ok_or_else(|| invalid("no recovery key"))?;
        let me = self.me.as_deref().ok_or_else(|| invalid("no account"))?;
        let info = recovery::open(
            space,
            reference.epoch,
            &reference.device,
            me,
            agreement,
            &blob,
        )
        .ok_or_else(|| invalid("cannot open recovery context"))?;
        let summary = verify_group_info(space, &info, Some(&reference.device)).map_err(invalid)?;
        if summary.epoch != reference.epoch || !summary.identities.is_subset(&self.roster(space)) {
            return Err(invalid(
                "recovery context does not match the current roster or epoch",
            ));
        }
        let Some(recipients) = self.recovery_recipients(space)? else {
            return Ok(false);
        };
        let tx = self.store.conn().unchecked_transaction().map_err(invalid)?;
        let c = self
            .device_on(&tx)?
            .recover(space, &info, &self.roster(space))
            .map_err(invalid)?;
        let events = self.store_recovery_commit(space, c, Some(&recipients))?;
        tx.commit().map_err(invalid)?;
        self.queued_recovery_commit(events);
        Ok(true)
    }
}

#[cfg(test)]
mod tests;
