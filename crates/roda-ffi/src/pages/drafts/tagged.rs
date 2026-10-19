//! Tagged Plan branch of the existing actor/space/item journal row.

use std::io::{self, Write};

use base64::Engine as _;
use roda_docs::plan::{PlanBlockEdit, PlanRevision, MAX_PLAN_PACKET_BYTES};
use roda_types::PlanManifestV1;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::engine::{Engine, ItemState, R};
use crate::CoreError;

pub(crate) const MAX_CONTEXT_BYTES: usize = 128 * 1024;
pub(crate) const MAX_JOURNAL_BYTES: usize = 4 * 1024 * 1024;
const MAX_CONTEXT_PLAIN: usize = MAX_CONTEXT_BYTES / 4 * 3 - 40;
const MAX_JOURNAL_ENCODED: usize = (MAX_JOURNAL_BYTES + 40).div_ceil(3) * 4 + 3;
const RECEIPTS: usize = 512;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlanOrigin {
    pub space: String,
    pub item: String,
    pub creation_event: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlanContextV1 {
    pub tag: PlanOrigin,
    pub generation: u64,
    pub revision_base64: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlanSnapshotV1 {
    pub prefix: PlanManifestV1,
    pub update_json_base64: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlanCommitRef {
    pub client_id: String,
    pub event_id: String,
    pub published_revision_base64: String,
    pub published_content_hash: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlanReceiptV1 {
    pub tag: PlanOrigin,
    pub mutation_id: String,
    pub request_hash: String,
    pub context_hash: String,
    pub input_generation: u64,
    pub accepted_generation: u64,
    pub content_hash: String,
    pub accepted_revision_base64: String,
    pub applied_edit_context: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlanJournalV1 {
    pub tag: PlanOrigin,
    pub generation: u64,
    pub minimum_generation: u64,
    pub snapshot: Option<PlanSnapshotV1>,
    pub has_draft: bool,
    pub commit: Option<PlanCommitRef>,
    pub receipts: Vec<PlanReceiptV1>,
    #[serde(
        default,
        skip_serializing_if = "crate::pages::input::InputStateV1::is_unused"
    )]
    pub input: crate::pages::input::InputStateV1,
}

impl PlanJournalV1 {
    pub fn new(tag: PlanOrigin) -> Self {
        Self {
            tag,
            generation: 0,
            minimum_generation: 0,
            snapshot: None,
            has_draft: false,
            commit: None,
            receipts: Vec::new(),
            input: Default::default(),
        }
    }

    pub fn receipt(&self, id: &str) -> Option<&PlanReceiptV1> {
        self.receipts.iter().find(|r| r.mutation_id == id)
    }

    pub fn continuation(&self, context_hash: &str) -> Option<&PlanReceiptV1> {
        self.receipts
            .iter()
            .rev()
            .find(|r| r.context_hash == context_hash)
    }

    pub fn next_generation(&self) -> R<u64> {
        self.generation
            .checked_add(1)
            .ok_or_else(super::storage_error)
    }

    pub fn accept(&mut self, receipt: PlanReceiptV1) -> R<()> {
        if receipt.tag != self.tag || receipt.accepted_generation != self.next_generation()? {
            return Err(invalid("invalid Plan receipt generation or origin"));
        }
        self.generation = receipt.accepted_generation;
        self.receipts.push(receipt);
        if self.receipts.len() > RECEIPTS {
            let removed = self.receipts.remove(0);
            self.minimum_generation = self.minimum_generation.max(
                removed
                    .input_generation
                    .checked_add(1)
                    .ok_or_else(super::storage_error)?,
            );
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn invalidate_contexts(&mut self) -> R<()> {
        self.generation = self.next_generation()?;
        self.minimum_generation = self.generation;
        Ok(())
    }

    fn validate(&self, expected: &PlanOrigin) -> R<()> {
        self.input.validate()?;
        if self.tag != *expected
            || self.minimum_generation > self.generation
            || self.receipts.len() > RECEIPTS
        {
            return Err(invalid("invalid Plan journal origin or generation fence"));
        }
        let mut ids = std::collections::HashSet::new();
        let mut generation = 0;
        for receipt in &self.receipts {
            if receipt.tag != *expected
                || receipt.accepted_generation <= generation
                || receipt.accepted_generation > self.generation
                || receipt.input_generation >= receipt.accepted_generation
                || !ids.insert(&receipt.mutation_id)
            {
                return Err(invalid("invalid Plan journal receipt"));
            }
            decode_bytes(
                &receipt.accepted_revision_base64,
                PlanRevision::MAX_ENCODED_BYTES,
            )?;
            if receipt.applied_edit_context.len() > MAX_CONTEXT_BYTES {
                return Err(limit("Plan receipt context"));
            }
            generation = receipt.accepted_generation;
        }
        if let Some(snapshot) = &self.snapshot {
            decode_bytes(&snapshot.update_json_base64, MAX_PLAN_PACKET_BYTES)?;
        }
        if let Some(commit) = &self.commit {
            decode_bytes(
                &commit.published_revision_base64,
                PlanRevision::MAX_ENCODED_BYTES,
            )?;
        }
        Ok(())
    }
}

pub(crate) fn invalid(reason: &str) -> CoreError {
    CoreError::Invalid {
        reason: reason.into(),
    }
}

fn limit(what: &str) -> CoreError {
    invalid(&format!(
        "{what} exceeds the local bound; existing history was retained"
    ))
}

struct LimitedBytes {
    bytes: Vec<u8>,
    limit: usize,
}
impl Write for LimitedBytes {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self
            .bytes
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > self.limit)
        {
            return Err(io::Error::other("bounded Plan serialization"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(crate) fn bounded_json(value: &impl Serialize, maximum: usize) -> R<Vec<u8>> {
    let mut writer = LimitedBytes {
        bytes: Vec::new(),
        limit: maximum,
    };
    serde_json::to_writer(&mut writer, value).map_err(|_| limit("Plan encoded state"))?;
    Ok(writer.bytes)
}

// Serde's ordinary parser permits duplicate map keys. This visitor refuses them before
// the strict typed deserializer; the byte cap also bounds allocation and nesting.
struct StrictValue(serde_json::Value);
impl<'de> Deserialize<'de> for StrictValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = serde_json::Value;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("bounded JSON without duplicate keys")
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Self::Value, E> {
                Ok(v.into())
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Self::Value, E> {
                Ok(v.into())
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(v.into())
            }
            fn visit_f64<E: serde::de::Error>(self, _: f64) -> Result<Self::Value, E> {
                Err(E::custom("integer JSON required"))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(v.into())
            }
            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Self::Value, E> {
                Ok(v.into())
            }
            fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(serde_json::Value::Null)
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(serde_json::Value::Null)
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut a: A,
            ) -> Result<Self::Value, A::Error> {
                let mut out = Vec::new();
                while let Some(v) = a.next_element::<StrictValue>()? {
                    out.push(v.0);
                }
                Ok(out.into())
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut a: A,
            ) -> Result<Self::Value, A::Error> {
                let mut out = serde_json::Map::new();
                while let Some(k) = a.next_key::<String>()? {
                    if out.contains_key(&k) {
                        return Err(serde::de::Error::custom("duplicate JSON key"));
                    }
                    out.insert(k, a.next_value::<StrictValue>()?.0);
                }
                Ok(out.into())
            }
        }
        deserializer.deserialize_any(Visitor).map(Self)
    }
}

pub(crate) fn strict<T: DeserializeOwned + Serialize>(bytes: &[u8]) -> R<T> {
    let value: StrictValue = serde_json::from_slice(bytes).map_err(|_| super::storage_error())?;
    let result: T = serde_json::from_value(value.0.clone()).map_err(|_| super::storage_error())?;
    if serde_json::to_value(&result).map_err(|_| super::storage_error())? != value.0 {
        return Err(invalid("unknown or noncanonical Plan state field"));
    }
    Ok(result)
}

pub(crate) fn encode_bytes(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

pub(crate) fn decode_bytes(encoded: &str, maximum: usize) -> R<Vec<u8>> {
    if encoded.len() > maximum.div_ceil(3) * 4 {
        return Err(limit("Plan byte field"));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| super::storage_error())?;
    if bytes.len() > maximum || encode_bytes(&bytes) != encoded {
        return Err(limit("Plan byte field"));
    }
    Ok(bytes)
}

pub(crate) fn strict_legacy<T: DeserializeOwned>(bytes: &[u8]) -> R<T> {
    let value: StrictValue = serde_json::from_slice(bytes).map_err(|_| super::storage_error())?;
    serde_json::from_value(value.0).map_err(|_| super::storage_error())
}

fn scope(
    engine: &Engine,
    item: &ItemState,
    tag: &PlanOrigin,
    context: bool,
) -> R<(String, Vec<u8>, [u8; 32])> {
    if tag.space != item.space || tag.item != item.id {
        return Err(invalid("foreign Plan origin"));
    }
    let (id, mut aad, key) = engine.draft_scope(item)?;
    for field in [
        b"\0plan.v1".as_slice(),
        tag.space.as_bytes(),
        tag.item.as_bytes(),
        tag.creation_event.as_bytes(),
    ] {
        aad.extend((field.len() as u64).to_be_bytes());
        aad.extend(field);
    }
    if context {
        aad.extend(b"\0editor context");
    }
    Ok((id, aad, key))
}

fn seal_v2(key: &[u8; 32], aad: &[u8], plain: &[u8]) -> R<String> {
    let encoded = super::seal(key, aad, plain)?;
    Ok(format!(
        "v2:{}",
        encoded.strip_prefix("v1:").expect("legacy seal prefix")
    ))
}

fn open_v2(key: &[u8; 32], aad: &[u8], encoded: &str, maximum: usize) -> R<Vec<u8>> {
    let body = encoded
        .strip_prefix("v2:")
        .ok_or_else(super::storage_error)?;
    if body.len() > (maximum + 40).div_ceil(3) * 4 {
        return Err(limit("Plan ciphertext"));
    }
    let plain = super::open(key, aad, &format!("v1:{body}"))?;
    if plain.len() > maximum {
        return Err(limit("Plan plaintext"));
    }
    Ok(plain)
}

pub(crate) fn row_id(engine: &Engine, item: &ItemState) -> R<String> {
    let actor = engine.me_id()?;
    let aad = serde_json::to_vec(&("zoen.page.draft.v1", actor, &item.space, &item.id))
        .map_err(|_| super::storage_error())?;
    Ok(format!("page.draft.{}", hex::encode(Sha256::digest(aad))))
}

#[cfg(test)]
pub(crate) fn write_legacy_fixture(engine: &Engine, item: &ItemState, literal: &[u8]) -> R<String> {
    let (id, aad, key) = engine.draft_scope(item)?;
    let ciphertext = super::seal(&key, &aad, literal)?;
    engine.store.set_meta(&id, &ciphertext)?;
    Ok(ciphertext)
}

pub(crate) fn load(engine: &Engine, item: &ItemState, tag: &PlanOrigin) -> R<PlanJournalV1> {
    if engine.me.is_none() {
        return Ok(PlanJournalV1::new(tag.clone()));
    }
    let Some(encoded) = engine.store.meta(&row_id(engine, item)?)? else {
        return Ok(PlanJournalV1::new(tag.clone()));
    };
    if encoded.len() > MAX_JOURNAL_ENCODED {
        return Err(limit("Plan journal ciphertext"));
    }
    let (_, aad, key) = scope(engine, item, tag, false)?;
    let plain = if encoded.starts_with("v3:") {
        open_v3(&key, &aad, &encoded, b"plan-v1")?
    } else {
        open_v2(&key, &aad, &encoded, MAX_JOURNAL_BYTES)?
    };
    let journal: PlanJournalV1 = strict_legacy(&plain)?;
    if !encoded.starts_with("v3:") && journal.input != Default::default() {
        return Err(super::storage_error());
    }
    journal.validate(tag)?;
    Ok(journal)
}

pub(crate) fn save(engine: &Engine, item: &ItemState, journal: &PlanJournalV1) -> R<()> {
    journal.validate(&journal.tag)?;
    let (id, aad, key) = scope(engine, item, &journal.tag, false)?;
    let plain = bounded_json(journal, MAX_JOURNAL_BYTES)?;
    let ciphertext = if journal.input.is_unused() {
        seal_v2(&key, &aad, &plain)?
    } else {
        seal_v3(&key, &aad, &plain, b"plan-v1")?
    };
    engine.store.set_meta(&id, &ciphertext)?;
    Ok(())
}

pub(crate) fn context(
    engine: &Engine,
    item: &ItemState,
    tag: &PlanOrigin,
    generation: u64,
    revision: &[u8],
) -> R<String> {
    if revision.len() > PlanRevision::MAX_ENCODED_BYTES {
        return Err(limit("Plan revision"));
    }
    let (_, aad, key) = scope(engine, item, tag, true)?;
    let value = PlanContextV1 {
        tag: tag.clone(),
        generation,
        revision_base64: encode_bytes(revision),
    };
    let encoded = seal_v2(&key, &aad, &bounded_json(&value, MAX_CONTEXT_PLAIN)?)?;
    if encoded.len() > MAX_CONTEXT_BYTES {
        return Err(limit("Plan editing context"));
    }
    Ok(encoded)
}

pub(crate) fn decode_context(
    engine: &Engine,
    item: &ItemState,
    tag: &PlanOrigin,
    encoded: &str,
) -> R<PlanContextV1> {
    if encoded.len() > MAX_CONTEXT_BYTES {
        return Err(limit("Plan editing context"));
    }
    let (_, aad, key) = scope(engine, item, tag, true)?;
    let context: PlanContextV1 = strict(&open_v2(&key, &aad, encoded, MAX_CONTEXT_PLAIN)?)?;
    if context.tag != *tag {
        return Err(invalid("foreign Plan context"));
    }
    decode_bytes(&context.revision_base64, PlanRevision::MAX_ENCODED_BYTES)?;
    Ok(context)
}

pub(crate) fn context_hash(context: &str) -> String {
    hex::encode(
        Sha256::new()
            .chain_update(b"zoen.plan.editor.context.v1\0")
            .chain_update(context)
            .finalize(),
    )
}

pub(crate) fn request_hash(
    tag: &PlanOrigin,
    id: &str,
    context: &str,
    order: &[String],
    changed: &[PlanBlockEdit],
) -> R<String> {
    if order.len() > roda_docs::plan::MAX_PLAN_ATOMS
        || changed.len() > roda_docs::plan::MAX_PLAN_ATOMS
    {
        return Err(limit("Plan edit block count"));
    }
    let changed: Vec<_> = changed
        .iter()
        .map(|b| (&b.id, &b.content, &b.text, &b.spans))
        .collect();
    let bytes = bounded_json(&(tag, id, context, order, changed), MAX_PLAN_PACKET_BYTES)?;
    Ok(hex::encode(
        Sha256::new()
            .chain_update(b"zoen.plan.edit.request.v1\0")
            .chain_update(bytes)
            .finalize(),
    ))
}

// v3 occupies the same encrypted row and uses a distinct authenticated domain.
pub(crate) fn seal_v3(key: &[u8; 32], aad: &[u8], plain: &[u8], tag: &[u8]) -> R<String> {
    let mut domain = aad.to_vec();
    domain.extend(b"\0input-journal.v3\0");
    domain.extend(tag);
    let encoded = super::seal(key, &domain, plain)?;
    Ok(format!(
        "v3:{}",
        encoded
            .strip_prefix("v1:")
            .ok_or_else(super::storage_error)?
    ))
}
pub(crate) fn open_v3(key: &[u8; 32], aad: &[u8], encoded: &str, tag: &[u8]) -> R<Vec<u8>> {
    if encoded.len() > MAX_JOURNAL_ENCODED {
        return Err(limit("input journal ciphertext"));
    }
    let body = encoded
        .strip_prefix("v3:")
        .ok_or_else(super::storage_error)?;
    let mut domain = aad.to_vec();
    domain.extend(b"\0input-journal.v3\0");
    domain.extend(tag);
    crate::local_sealed::open(key, &domain, &format!("v1:{body}"), MAX_JOURNAL_BYTES)
        .map_err(|_| super::storage_error())
}
