use std::{collections::BTreeMap, fmt};

use zoen_log::{content::SealedKind, verify_author, verify_event};
use zoen_proto::{Envelope, Sequenced};
use zoen_types::{Event, EventBody, Seen};

use super::reference::ReplyReference;
use super::{
    DeviceCore, NativeError, NativeImage, ReplyIntent, Result, MAX_BATCH, MAX_EVENT_BYTES,
};

/// Exact historical native acceptance facts. These facts cannot activate a
/// runtime root, authorize transport or produce another publication.
pub struct AcceptedReply {
    original: ReplyIntent,
    client_id: String,
    message: String,
    inner_digest: String,
    reference_digest: String,
    publish_digest: String,
    accepted: Seen,
    image: NativeImage,
}

impl fmt::Debug for AcceptedReply {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AcceptedReply").finish_non_exhaustive()
    }
}

impl AcceptedReply {
    pub fn original(&self) -> &ReplyIntent {
        &self.original
    }
    pub fn client_id(&self) -> &str {
        &self.client_id
    }
    pub fn message(&self) -> &str {
        &self.message
    }
    pub fn event_digest(&self) -> &str {
        &self.inner_digest
    }
    pub fn reference_digest(&self) -> &str {
        &self.reference_digest
    }
    pub fn publish_digest(&self) -> &str {
        &self.publish_digest
    }
    pub fn accepted(&self) -> &Seen {
        &self.accepted
    }
    pub fn image(&self) -> &NativeImage {
        &self.image
    }
}

/// A contiguous historical prefix with the entire pending outbox unchanged.
/// It proves no acceptance and cannot produce a new native command.
pub struct NativeReceiptPrefix {
    original: ReplyIntent,
    client_id: String,
    message: String,
    inner_digest: String,
    reference_digest: String,
    publish_digest: String,
    before: Seen,
    frontier: Seen,
    image: NativeImage,
}

impl fmt::Debug for NativeReceiptPrefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeReceiptPrefix")
            .finish_non_exhaustive()
    }
}

impl NativeReceiptPrefix {
    pub fn original(&self) -> &ReplyIntent {
        &self.original
    }
    pub fn client_id(&self) -> &str {
        &self.client_id
    }
    pub fn message(&self) -> &str {
        &self.message
    }
    pub fn event_digest(&self) -> &str {
        &self.inner_digest
    }
    pub fn reference_digest(&self) -> &str {
        &self.reference_digest
    }
    pub fn publish_digest(&self) -> &str {
        &self.publish_digest
    }
    pub fn before(&self) -> &Seen {
        &self.before
    }
    pub fn frontier(&self) -> &Seen {
        &self.frontier
    }
    pub fn image(&self) -> &NativeImage {
        &self.image
    }
}

#[derive(Debug)]
pub enum ReplyReceipt {
    Accepted(AcceptedReply),
    Prefix(NativeReceiptPrefix),
}

#[derive(PartialEq, Eq)]
struct PendingSnapshot {
    event: Event,
    attempts: u32,
    last_error: Option<String>,
    failed: bool,
}

#[derive(PartialEq, Eq)]
pub(super) struct OutboxSnapshot {
    rows: BTreeMap<String, PendingSnapshot>,
    pending: BTreeMap<String, String>,
    references: Vec<(String, String)>,
    sealing: BTreeMap<String, String>,
}

impl DeviceCore {
    pub(super) fn guard_pending_echo(&self, entry: &Sequenced) -> Result<()> {
        let client_id = entry.env.client_id();
        let reference = self.reply_reference(client_id)?;
        let pending = self
            .engine
            .store
            .outbox_get(client_id)
            .map_err(|_| NativeError::Storage)?;
        let Some(pending) = pending else {
            if self.engine.net.pending.contains_key(client_id) {
                return Err(NativeError::History);
            }
            if let Some(reference) = reference {
                let expected = reference.envelope(self)?;
                let accepted = self.accepted_reply_event(&reference, &expected)?;
                if expected != entry.env
                    || accepted.seq != entry.seq
                    || accepted.prev != entry.prev
                    || accepted.hash != entry.hash
                {
                    return Err(NativeError::History);
                }
            }
            return Ok(());
        };
        let expected = self.pending_envelope(&pending, reference.as_ref())?;
        if expected.content().len() > MAX_EVENT_BYTES
            || expected.is_pruned()
            || expected != entry.env
            || expected.verify().is_err()
        {
            return Err(NativeError::History);
        }
        Ok(())
    }

    fn pending_envelope(
        &self,
        pending: &Event,
        reference: Option<&ReplyReference>,
    ) -> Result<Envelope> {
        let client_id = &pending.client_id;
        let account = self
            .engine
            .net
            .account
            .as_ref()
            .ok_or(NativeError::Credential)?;
        if self.engine.net.pending.get(client_id) != Some(&pending.space)
            || pending.author != self.agent
            || pending.author != account.identity
            || pending.device.as_deref() != Some(account.device.as_str())
            || pending.cert.as_deref() != Some(account.cert.as_str())
            || pending.content.len() > MAX_EVENT_BYTES
            || verify_author(pending).is_err()
        {
            return Err(NativeError::History);
        }
        // Mutable synced/projection state cannot change a retained publication.
        // Applications need their producer's immutable original association.
        let expected = if let Some(reference) = reference {
            let expected = reference.envelope(self)?;
            let (epoch, historical) = self
                .engine
                .sealed_copy(pending)
                .ok_or(NativeError::History)?;
            if !reference.matches_inner(pending, &expected)
                || historical != expected
                || historical
                    .sealed_data()
                    .and_then(|(_, bytes)| zoen_mls::application_epoch(bytes))
                    != Some(epoch)
            {
                return Err(NativeError::History);
            }
            expected
        } else if crate::mls::must_seal(&pending.body)
            && !matches!(&pending.body, EventBody::Sealed { kind } if kind == SealedKind::Commit.name() || kind == SealedKind::Welcome.name())
        {
            return Err(NativeError::History);
        } else {
            Envelope::plain(pending)
        };
        Ok(expected)
    }

    /// Ingests historical source only and proves this producer's exact original
    /// output. Current progress permission cannot substitute for receipt proof.
    pub fn receive_reply(
        &mut self,
        original: &ReplyIntent,
        client_id: &str,
        ordered_batch: Vec<Sequenced>,
    ) -> Result<AcceptedReply> {
        match self.receive_reply_progress(original, client_id, ordered_batch)? {
            ReplyReceipt::Accepted(receipt) => Ok(receipt),
            ReplyReceipt::Prefix(_) => {
                self.usable = false;
                Err(NativeError::NotReady)
            }
        }
    }

    /// Reads at most 64 real positions. A prefix keeps every pending row and
    /// sealing/reference byte unchanged; only native acceptance yields a receipt.
    pub fn receive_reply_progress(
        &mut self,
        original: &ReplyIntent,
        client_id: &str,
        ordered_batch: Vec<Sequenced>,
    ) -> Result<ReplyReceipt> {
        let result = (|| {
            if !self.usable {
                return Err(NativeError::History);
            }
            let reference = self
                .reply_reference(client_id)?
                .ok_or(NativeError::History)?;
            if reference.original != *original {
                return Err(NativeError::History);
            }
            let envelope = reference.envelope(self)?;
            let (reference_digest, publish_digest) = reference.digests()?;
            let before = self.snapshot_outbox()?;
            let initial = self
                .retained_head(original.space())?
                .ok_or(NativeError::History)?;
            let pending = self
                .engine
                .store
                .outbox_get(client_id)
                .map_err(|_| NativeError::Storage)?;
            if let Some(pending) = &pending {
                if self.pending_envelope(pending, Some(&reference))? != envelope
                    || self.engine.logs[original.space()]
                        .events()
                        .iter()
                        .any(|event| event.client_id == client_id)
                {
                    return Err(NativeError::History);
                }
            } else {
                self.accepted_reply_event(&reference, &envelope)?;
            }
            if ordered_batch.is_empty() && pending.is_some() {
                return Err(NativeError::NotReady);
            }
            let prefix_frontier =
                contiguous_prefix(&initial, original.space(), client_id, &ordered_batch);
            self.ingest(ordered_batch)?;
            match self.accepted_reply_event(&reference, &envelope) {
                Ok(event) => {
                    let accepted = Seen {
                        seq: event.seq,
                        hash: event.hash.clone(),
                    };
                    self.verify_receipt_delta(&before, client_id)?;
                    Ok(ReplyReceipt::Accepted(AcceptedReply {
                        original: reference.original,
                        client_id: reference.client_id,
                        message: reference.message,
                        inner_digest: reference.inner_digest,
                        reference_digest,
                        publish_digest,
                        accepted,
                        image: self.image()?,
                    }))
                }
                Err(NativeError::NotReady) => {
                    let frontier = prefix_frontier.ok_or(NativeError::History)?;
                    let pending = self
                        .engine
                        .store
                        .outbox_get(client_id)
                        .map_err(|_| NativeError::Storage)?
                        .ok_or(NativeError::History)?;
                    if self.snapshot_outbox()? != before
                        || self.pending_envelope(&pending, Some(&reference))? != envelope
                        || self.retained_head(original.space())?.as_ref() != Some(&frontier)
                        || self.engine.logs[original.space()]
                            .events()
                            .iter()
                            .any(|event| event.client_id == client_id)
                    {
                        return Err(NativeError::History);
                    }
                    Ok(ReplyReceipt::Prefix(NativeReceiptPrefix {
                        original: reference.original,
                        client_id: reference.client_id,
                        message: reference.message,
                        inner_digest: reference.inner_digest,
                        reference_digest,
                        publish_digest,
                        before: initial,
                        frontier,
                        image: self.image()?,
                    }))
                }
                Err(error) => Err(error),
            }
        })();
        if result.is_err() {
            self.usable = false;
        }
        result
    }

    fn accepted_reply_event<'a>(
        &'a self,
        reference: &ReplyReference,
        envelope: &Envelope,
    ) -> Result<&'a Event> {
        if self.engine.net.pending.contains_key(&reference.client_id)
            || self
                .engine
                .store
                .outbox_get(&reference.client_id)
                .map_err(|_| NativeError::Storage)?
                .is_some()
        {
            return Err(NativeError::NotReady);
        }
        let log = self
            .engine
            .logs
            .get(&reference.original.space)
            .ok_or(NativeError::History)?;
        let mut matching = log
            .events()
            .iter()
            .filter(|event| event.client_id == reference.client_id);
        let event = matching.next().ok_or(NativeError::History)?;
        let account = self
            .engine
            .net
            .account
            .as_ref()
            .ok_or(NativeError::Credential)?;
        if matching.next().is_some()
            || self
                .engine
                .store
                .meta(&format!("mls.sealed:{}", reference.client_id))
                .map_err(|_| NativeError::Storage)?
                .is_some()
            || self
                .engine
                .store
                .meta(&format!("mls.welcome:{}", reference.client_id))
                .map_err(|_| NativeError::Storage)?
                .is_some()
            || !reference.matches_inner(event, envelope)
            || verify_event(event).is_err()
            || !crate::mls_opening::exact_own_echo(&self.engine.store, event, account, envelope)
        {
            return Err(NativeError::History);
        }
        Ok(event)
    }

    pub(super) fn snapshot_outbox(&self) -> Result<OutboxSnapshot> {
        let mut rows = BTreeMap::new();
        for row in self
            .engine
            .store
            .outbox()
            .map_err(|_| NativeError::Storage)?
        {
            if rows.len() >= MAX_BATCH {
                return Err(NativeError::Bounds);
            }
            let cid = row.event.client_id.clone();
            rows.insert(
                cid,
                PendingSnapshot {
                    event: row.event,
                    attempts: row.attempts,
                    last_error: row.last_error,
                    failed: row.failed,
                },
            );
        }
        let pending: BTreeMap<_, _> = self
            .engine
            .net
            .pending
            .iter()
            .map(|(id, space)| (id.clone(), space.clone()))
            .collect();
        if pending.len() != rows.len()
            || rows
                .iter()
                .any(|(id, row)| pending.get(id) != Some(&row.event.space))
        {
            return Err(NativeError::History);
        }
        let mut query = self.engine.store.conn().prepare("SELECT key,value FROM meta WHERE key LIKE 'runtime.native-reply.v1/%' ORDER BY key").map_err(|_| NativeError::Storage)?;
        let references = query
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|_| NativeError::Storage)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|_| NativeError::Storage)?;
        let mut query = self.engine.store.conn().prepare("SELECT key,value FROM meta WHERE key LIKE 'mls.sealed:%' OR key LIKE 'mls.welcome:%' ORDER BY key").map_err(|_| NativeError::Storage)?;
        let sealing = query
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|_| NativeError::Storage)?
            .collect::<std::result::Result<BTreeMap<_, _>, _>>()
            .map_err(|_| NativeError::Storage)?;
        Ok(OutboxSnapshot {
            rows,
            pending,
            references,
            sealing,
        })
    }

    pub(super) fn verify_receipt_delta(
        &self,
        before: &OutboxSnapshot,
        client_id: &str,
    ) -> Result<()> {
        let after = self.snapshot_outbox()?;
        if before.references != after.references
            || after.rows.keys().any(|id| !before.rows.contains_key(id))
        {
            return Err(NativeError::History);
        }
        let mut expected_pending = before.pending.clone();
        let mut expected_sealing = before.sealing.clone();
        for (id, row) in &before.rows {
            match after.rows.get(id) {
                Some(current) if current == row => {}
                None if id == client_id => {
                    expected_pending.remove(id);
                    expected_sealing.remove(&format!("mls.sealed:{id}"));
                    expected_sealing.remove(&format!("mls.welcome:{id}"));
                    if self
                        .engine
                        .store
                        .meta(&format!("mls.sealed:{id}"))
                        .map_err(|_| NativeError::Storage)?
                        .is_some()
                        || self
                            .engine
                            .store
                            .meta(&format!("mls.welcome:{id}"))
                            .map_err(|_| NativeError::Storage)?
                            .is_some()
                    {
                        return Err(NativeError::History);
                    }
                }
                _ => return Err(NativeError::History),
            }
        }
        if expected_pending != after.pending || expected_sealing != after.sealing {
            return Err(NativeError::History);
        }
        Ok(())
    }
}

fn contiguous_prefix(
    initial: &Seen,
    space: &str,
    client_id: &str,
    batch: &[Sequenced],
) -> Option<Seen> {
    if batch.is_empty() || batch.len() > MAX_BATCH {
        return None;
    }
    let mut frontier = initial.clone();
    for entry in batch {
        if entry.env.space() != space
            || entry.env.is_pruned()
            || entry.env.client_id() == client_id
            || Some(entry.seq) != frontier.seq.checked_add(1)
            || entry.prev != frontier.hash
        {
            return None;
        }
        frontier = Seen {
            seq: entry.seq,
            hash: entry.hash.clone(),
        };
    }
    Some(frontier)
}
