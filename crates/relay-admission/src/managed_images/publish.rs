//! A publication consumes independent effect history before any linear permit
//! can enter the relay queue. Existing consumption is only an observation: the
//! actual source may prove Accepted, but absence cannot authorize another send.
use super::file::{options, regular, sync_directory, unavailable};
use super::{AnchorError, AnchorReader, Checkpoint, GenerationGuard};
use protocol::{ClientFrame, Envelope};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Write};

/// Read-only transaction condition. Possessing/decoding it is not a permit.
/// Relay append must receive the private linear ManagedPublishPermit as well.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishCondition {
    pub namespace: String,
    pub source: String,
    pub agent: String,
    pub device: String,
    pub checkpoint: Checkpoint,
    pub checkpoint_record_digest: String,
    pub effect_digest: String,
    pub wire_digest: String,
    pub record_digest: String,
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Record {
    schema: u8,
    namespace: String,
    source: String,
    agent: String,
    device: String,
    checkpoint: Checkpoint,
    checkpoint_record_digest: String,
    effect_digest: String,
    wire_digest: String,
}

pub enum ConsumePublish {
    Fresh(ManagedPublishPermit),
    /// Never a permit. Look up the exact original Accepted/event in the actual
    /// source; if absent, retain Unknown rather than recreate a publication.
    Consumed(PublishCondition),
}

/// Not Clone/Deserialize. The queue must own this value until its real append
/// transaction terminates, including after cancellation of its caller future.
pub struct ManagedPublishPermit {
    reader: AnchorReader,
    condition: PublishCondition,
    _generation: GenerationGuard,
}

pub fn publish_digest(namespace: &str, envelope: &Envelope) -> Result<String, AnchorError> {
    if namespace.is_empty()
        || namespace.len() > 256
        || envelope.verify().is_err()
        || envelope.is_pruned()
        || !envelope.is_sealed()
    {
        return Err(AnchorError::Invalid);
    }
    let wire = ClientFrame::Publish {
        env: envelope.clone(),
    }
    .encode();
    Ok(event_log::sha256_hex(
        &serde_json::to_vec(&(
            "managed-images/publish/1",
            namespace,
            event_log::sha256_hex(&wire),
        ))
        .map_err(|_| AnchorError::Invalid)?,
    ))
}

impl Record {
    fn condition(&self) -> Result<PublishCondition, AnchorError> {
        Ok(PublishCondition {
            namespace: self.namespace.clone(),
            source: self.source.clone(),
            agent: self.agent.clone(),
            device: self.device.clone(),
            checkpoint: self.checkpoint.clone(),
            checkpoint_record_digest: self.checkpoint_record_digest.clone(),
            effect_digest: self.effect_digest.clone(),
            wire_digest: self.wire_digest.clone(),
            record_digest: event_log::sha256_hex(
                &serde_json::to_vec(self).map_err(|_| AnchorError::Invalid)?,
            ),
        })
    }
}

impl AnchorReader {
    /// Compare a conflict-tracked store read with independent live history.
    /// A concurrent prepared transition may legitimately have committed its
    /// store half before the final external rename. This is observation only;
    /// callers still need the original effect's guard/permit through commit.
    pub fn check_store_checkpoint(&self, observed: &Checkpoint) -> Result<(), AnchorError> {
        let state = self.state()?;
        if state.committed != *observed
            && !state
                .prepared
                .as_ref()
                .is_some_and(|prepared| prepared.next == *observed)
        {
            return Err(AnchorError::Unreconciled);
        }
        Ok(())
    }

    /// Read-only binding facts for a per-effect transport challenge. These
    /// facts cannot enter append without consume_publish's private permit.
    pub fn inspect_publish(
        &self,
        checkpoint: &Checkpoint,
        envelope: &Envelope,
    ) -> Result<PublishCondition, AnchorError> {
        let _generation = self.effect_generation_guard(checkpoint)?;
        self.publish_record(checkpoint, envelope)?.condition()
    }

    fn publish_record(
        &self,
        checkpoint: &Checkpoint,
        envelope: &Envelope,
    ) -> Result<Record, AnchorError> {
        let device = envelope.device().ok_or(AnchorError::Invalid)?;
        if !self.deployment.principal(envelope.author(), device) {
            return Err(AnchorError::Invalid);
        }
        let effect_digest = publish_digest(self.deployment.namespace(), envelope)?;
        let prepared = self
            .checkpoint_effect(checkpoint)?
            .ok_or(AnchorError::Unreconciled)?;
        if prepared.retirement || prepared.effect_digest != effect_digest {
            return Err(AnchorError::Invalid);
        }
        let wire = ClientFrame::Publish {
            env: envelope.clone(),
        }
        .encode();
        Ok(Record {
            schema: 1,
            namespace: self.deployment.namespace().into(),
            source: self.deployment.source().into(),
            agent: envelope.author().into(),
            device: device.into(),
            checkpoint: checkpoint.clone(),
            checkpoint_record_digest: event_log::sha256_hex(
                &serde_json::to_vec(&prepared).map_err(|_| AnchorError::Invalid)?,
            ),
            effect_digest,
            wire_digest: event_log::sha256_hex(&wire),
        })
    }

    fn read_publish(&self, effect: &str) -> Result<Record, AnchorError> {
        if !super::state::digest(effect) {
            return Err(AnchorError::Invalid);
        }
        let path = self.effect_directory()?.join(format!("{effect}.json"));
        regular(&path)?;
        let mut bytes = Vec::new();
        File::open(path)
            .map_err(unavailable)?
            .take(16_385)
            .read_to_end(&mut bytes)
            .map_err(unavailable)?;
        if bytes.len() > 16_384 {
            return Err(AnchorError::Unreconciled);
        }
        let record: Record =
            serde_json::from_slice(&bytes).map_err(|_| AnchorError::Unreconciled)?;
        let canonical = serde_json::to_vec(&record).map_err(|_| AnchorError::Invalid)?;
        if canonical != bytes
            || record.schema != 1
            || record.effect_digest != effect
            || record.namespace != self.deployment.namespace()
            || record.source != self.deployment.source()
            || !self.deployment.principal(&record.agent, &record.device)
        {
            return Err(AnchorError::Unreconciled);
        }
        let prepared = self
            .checkpoint_effect(&record.checkpoint)?
            .ok_or(AnchorError::Unreconciled)?;
        if prepared.retirement
            || prepared.effect_digest != record.effect_digest
            || event_log::sha256_hex(
                &serde_json::to_vec(&prepared).map_err(|_| AnchorError::Invalid)?,
            ) != record.checkpoint_record_digest
            || !super::state::digest(&record.wire_digest)
        {
            return Err(AnchorError::Unreconciled);
        }
        Ok(record)
    }

    pub fn consume_publish(
        &self,
        checkpoint: &Checkpoint,
        envelope: &Envelope,
    ) -> Result<ConsumePublish, AnchorError> {
        let generation = self.effect_generation_guard(checkpoint)?;
        let record = self.publish_record(checkpoint, envelope)?;
        let directory = self.effect_directory()?;
        let path = directory.join(format!("{}.json", record.effect_digest));
        let mut file = match options().open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let original = self.read_publish(&record.effect_digest)?;
                if original.wire_digest != record.wire_digest
                    || original.agent != record.agent
                    || original.device != record.device
                {
                    return Err(AnchorError::Invalid);
                }
                return Ok(ConsumePublish::Consumed(original.condition()?));
            }
            Err(error) => return Err(unavailable(error)),
        };
        // This filename is a consumed tombstone immediately. Never unlink it on
        // a failed write/ACK: a partial or uncertain record remains Unknown.
        file.write_all(&serde_json::to_vec(&record).map_err(|_| AnchorError::Invalid)?)
            .map_err(unavailable)?;
        file.sync_all().map_err(unavailable)?;
        sync_directory(&directory)?;
        let permit = ManagedPublishPermit {
            reader: self.clone(),
            condition: record.condition()?,
            _generation: generation,
        };
        permit.check(envelope)?;
        Ok(ConsumePublish::Fresh(permit))
    }
}

impl ManagedPublishPermit {
    pub fn condition(&self) -> &PublishCondition {
        &self.condition
    }

    /// The append transaction reads current CP with a conflict range and calls
    /// this before committing consumption and the exact original event. An old
    /// matching original CP is insufficient when independent history advanced.
    pub fn check_store_checkpoint(&self, observed: &Checkpoint) -> Result<(), AnchorError> {
        if observed.generation != self.condition.checkpoint.generation
            || observed.sequence < self.condition.checkpoint.sequence
        {
            return Err(AnchorError::Unreconciled);
        }
        self.reader.check_store_checkpoint(observed)
    }

    /// Independent live proof, kept under the original shared generation lock.
    /// FDB must additionally compare the historical checkpoint record and CAS
    /// absent consumption in the same transaction as the exact original append.
    pub fn check(&self, envelope: &Envelope) -> Result<(), AnchorError> {
        let state = self.reader.state()?;
        if state.committed.generation != self.condition.checkpoint.generation
            || publish_digest(&self.condition.namespace, envelope)? != self.condition.effect_digest
            || self
                .reader
                .read_publish(&self.condition.effect_digest)?
                .condition()?
                != self.condition
        {
            return Err(AnchorError::Unreconciled);
        }
        Ok(())
    }
}
