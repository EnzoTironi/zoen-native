//! Inspectable Plan operations. This codec is isolated from production PageContent.

use std::collections::{BTreeMap, HashSet};
use std::io::{self, Write};
use std::sync::Arc;

use loro::{
    ContainerID, ContainerType, IdSpan, JsonMapOp, JsonMovableListOp, JsonOp, JsonOpContent,
    JsonSchema, JsonTextOp, LoroValue, ID,
};
use serde::de::{DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::{error, CanonicalPlanSeed, PlanError, PlanUpdate, Result, MIGRATION_PEER, SCHEMA};
use crate::Page;

/// Matches the existing Page observed-context byte ceiling. Inline/blob selection is separate.
pub const MAX_PLAN_PACKET_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_PLAN_CHANGES: usize = 4_096;
pub const MAX_PLAN_RAW_OPS: usize = 32_768;
pub const MAX_PLAN_ATOMS: usize = 65_536;
const MAX_DEPS: usize = 4_096;
const MAX_JSON_DEPTH: usize = 32;
const MAX_JSON_NODES: usize = 262_144;
const FORMAT: &str = "zoen.plan.loro.json";
const VERSION: u8 = 1;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    format: String,
    version: u8,
    operations: JsonSchema,
}

#[derive(Serialize)]
struct EncodedEnvelope<'a> {
    format: &'static str,
    version: u8,
    operations: &'a JsonSchema,
}

pub(super) fn decode(bytes: &[u8]) -> Result<PlanUpdate> {
    if bytes.len() > MAX_PLAN_PACKET_BYTES {
        return Err(PlanError::Limit("plan packet bytes"));
    }
    // Parse bounded bytes before handing any input to Loro. Value also lets us reject
    // unknown/ignored fields instead of accepting a different operation interpretation.
    let raw = strict_json(bytes)?;
    let envelope: Envelope = serde_json::from_value(raw.clone()).map_err(error)?;
    if envelope.format != FORMAT || envelope.version != VERSION {
        return Err(PlanError::UnsupportedCodec);
    }
    if serde_json::to_value(&envelope).map_err(error)? != raw {
        return Err(error("noncanonical or unknown plan codec fields"));
    }
    let update = PlanUpdate(envelope.operations);
    atoms(&update.0)?;
    Ok(update)
}

pub(super) fn encode(update: &PlanUpdate) -> Result<Vec<u8>> {
    let bytes = serialized(update)?;
    atoms(&update.0)?;
    Ok(bytes)
}

pub(super) fn strict_value(bytes: &[u8]) -> Result<Value> {
    strict_json(bytes)
}

pub(super) fn fingerprint(update: &PlanUpdate) -> Result<[u8; 32]> {
    let _ = serialized(update)?;
    let mut digest = Sha256::new().chain_update(b"zoen-plan-operations-v1\0");
    for (id, atom) in atoms(&update.0)? {
        let provenance = atom
            .reserved_provenance
            .as_ref()
            .map(|(timestamp, message)| (*timestamp, message.as_deref()));
        let bytes = serde_json::to_vec(&(
            id,
            atom.container,
            atom.content,
            atom.deps,
            atom.lamport,
            provenance,
        ))
        .map_err(error)?;
        digest.update((bytes.len() as u64).to_le_bytes());
        digest.update(bytes);
    }
    Ok(digest.finalize().into())
}

fn serialized(update: &PlanUpdate) -> Result<Vec<u8>> {
    let envelope = EncodedEnvelope {
        format: FORMAT,
        version: VERSION,
        operations: &update.0,
    };
    let bytes = bounded_json(&envelope)?;
    strict_json(&bytes)?;
    Ok(bytes)
}

pub(super) fn bounded_json(value: &impl Serialize) -> Result<Vec<u8>> {
    let mut writer = BoundedWriter(Vec::new());
    serde_json::to_writer(&mut writer, value).map_err(|_| PlanError::Limit("plan packet bytes"))?;
    Ok(writer.0)
}

struct BoundedWriter(Vec<u8>);

impl Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_PLAN_PACKET_BYTES.saturating_sub(self.0.len()) {
            return Err(io::Error::other("plan packet bytes"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct JsonBudget {
    remaining: usize,
    exhausted: Option<&'static str>,
}

struct JsonNode<'a> {
    budget: &'a mut JsonBudget,
    depth: usize,
}

impl<'de> DeserializeSeed<'de> for JsonNode<'_> {
    type Value = Value;

    fn deserialize<D: serde::Deserializer<'de>>(
        self,
        de: D,
    ) -> std::result::Result<Value, D::Error> {
        use serde::de::Error;
        if self.depth > MAX_JSON_DEPTH {
            self.budget.exhausted = Some("plan JSON depth");
            return Err(D::Error::custom("plan JSON depth"));
        }
        if self.budget.remaining == 0 {
            self.budget.exhausted = Some("plan JSON nodes");
            return Err(D::Error::custom("plan JSON nodes"));
        }
        self.budget.remaining -= 1;
        de.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for JsonNode<'_> {
    type Value = Value;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("bounded JSON without duplicate object keys")
    }

    fn visit_bool<E: serde::de::Error>(self, value: bool) -> std::result::Result<Value, E> {
        Ok(Value::Bool(value))
    }

    fn visit_i64<E: serde::de::Error>(self, value: i64) -> std::result::Result<Value, E> {
        Ok(Value::from(value))
    }

    fn visit_u64<E: serde::de::Error>(self, value: u64) -> std::result::Result<Value, E> {
        Ok(Value::from(value))
    }

    fn visit_f64<E: serde::de::Error>(self, value: f64) -> std::result::Result<Value, E> {
        serde_json::Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| E::custom("nonfinite JSON number"))
    }

    fn visit_str<E: serde::de::Error>(self, value: &str) -> std::result::Result<Value, E> {
        Ok(Value::String(value.into()))
    }

    fn visit_string<E: serde::de::Error>(self, value: String) -> std::result::Result<Value, E> {
        Ok(Value::String(value))
    }

    fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> std::result::Result<Value, A::Error> {
        let mut values = Vec::new();
        while let Some(value) = seq.next_element_seed(JsonNode {
            budget: self.budget,
            depth: self.depth + 1,
        })? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> std::result::Result<Value, A::Error> {
        use serde::de::Error;
        let mut values = serde_json::Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if values.contains_key(&key) {
                return Err(A::Error::custom("duplicate plan JSON key"));
            }
            let value = map.next_value_seed(JsonNode {
                budget: self.budget,
                depth: self.depth + 1,
            })?;
            values.insert(key, value);
        }
        Ok(Value::Object(values))
    }
}

fn strict_json(bytes: &[u8]) -> Result<Value> {
    let mut de = serde_json::Deserializer::from_slice(bytes);
    let mut budget = JsonBudget {
        remaining: MAX_JSON_NODES,
        exhausted: None,
    };
    let value = JsonNode {
        budget: &mut budget,
        depth: 0,
    }
    .deserialize(&mut de);
    if let Some(limit) = budget.exhausted {
        return Err(PlanError::Limit(limit));
    }
    let value = value.map_err(error)?;
    de.end().map_err(error)?;
    Ok(value)
}

fn json_depth(value: &Value, depth: usize) -> Result<()> {
    if depth > MAX_JSON_DEPTH {
        return Err(PlanError::Limit("plan JSON depth"));
    }
    match value {
        Value::Array(values) => {
            for value in values {
                json_depth(value, depth + 1)?;
            }
        }
        Value::Object(values) => {
            for value in values.values() {
                json_depth(value, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}

type OpId = (u64, i32);
type Atoms = BTreeMap<OpId, Atom>;

#[derive(Debug, PartialEq, Eq)]
struct Atom {
    container: ContainerID,
    content: Value,
    deps: Vec<OpId>,
    lamport: u32,
    // Commit metadata can coalesce on ordinary peers. It is neither content nor authority.
    // The reserved deterministic tape has fixed metadata, which is compared as well.
    reserved_provenance: Option<(i64, Option<Arc<str>>)>,
}

fn conflict(peer: u64) -> PlanError {
    if peer == MIGRATION_PEER {
        PlanError::ReservedSeed
    } else {
        PlanError::OperationConflict
    }
}

/// All original overlaps are compared before Loro can discard known operation IDs.
pub(super) fn validate(page: &Page, update: &PlanUpdate, seed: &CanonicalPlanSeed) -> Result<()> {
    // Updates generated in memory also obey the packet boundary; no constructor bypass.
    let _ = serialized(update)?;
    let incoming = atoms(&update.0)?;
    let current = page.version();
    let reserved = seed.0.page.version();
    let mut covered = BTreeMap::<u64, Vec<i32>>::new();
    for &(peer, counter) in incoming.keys() {
        let vv = if peer == MIGRATION_PEER {
            &reserved
        } else {
            &current
        };
        if counter < *vv.get(&peer).unwrap_or(&0) {
            covered.entry(peer).or_default().push(counter);
        } else if peer == MIGRATION_PEER {
            return Err(PlanError::ReservedSeed);
        }
    }
    for (peer, counters) in covered {
        let expected_doc = if peer == MIGRATION_PEER {
            &seed.0.page.doc
        } else {
            &page.doc
        };
        let mut start = counters[0];
        let mut end = start + 1;
        for next in counters.into_iter().skip(1).chain(std::iter::once(-1)) {
            if next == end {
                end += 1;
                continue;
            }
            let changes = expected_doc.export_json_in_id_span(IdSpan::new(peer, start, end));
            let expected = atoms(&JsonSchema {
                schema_version: 1,
                start_version: Default::default(),
                peers: None,
                changes,
            })?;
            if expected.len() != (end - start) as usize {
                return Err(conflict(peer));
            }
            for (id, witness) in expected {
                if incoming.get(&id) != Some(&witness) {
                    return Err(conflict(peer));
                }
            }
            start = next;
            end = start + 1;
        }
    }
    validate_lamports(page, seed, &incoming)?;
    validate_mark_pairs(page, &incoming)?;
    Ok(())
}

fn validate_mark_pairs(page: &Page, incoming: &Atoms) -> Result<()> {
    let lookup = |peer: u64, counter: i32| -> Result<Atom> {
        let end = counter
            .checked_add(1)
            .ok_or_else(|| error("mark counter overflow"))?;
        let changes = page
            .doc
            .export_json_in_id_span(IdSpan::new(peer, counter, end));
        atoms(&JsonSchema {
            schema_version: 1,
            start_version: Default::default(),
            peers: None,
            changes,
        })?
        .remove(&(peer, counter))
        .ok_or(PlanError::MissingDependencies)
    };
    for (&(peer, counter), atom) in incoming {
        let Ok(JsonOpContent::Text(mark)) = serde_json::from_value(atom.content.clone()) else {
            continue;
        };
        let (partner, want_end) = match mark {
            JsonTextOp::Mark { .. } => (
                counter
                    .checked_add(1)
                    .ok_or_else(|| error("mark counter overflow"))?,
                true,
            ),
            JsonTextOp::MarkEnd => (
                counter
                    .checked_sub(1)
                    .filter(|n| *n >= 0)
                    .ok_or_else(|| error("orphan mark end"))?,
                false,
            ),
            _ => continue,
        };
        let known;
        let other = if let Some(other) = incoming.get(&(peer, partner)) {
            other
        } else {
            known = lookup(peer, partner)?;
            &known
        };
        let matches = match serde_json::from_value(other.content.clone()) {
            Ok(JsonOpContent::Text(JsonTextOp::MarkEnd)) => {
                want_end && other.deps == vec![(peer, counter)]
            }
            Ok(JsonOpContent::Text(JsonTextOp::Mark { .. })) => {
                !want_end && atom.deps == vec![(peer, partner)]
            }
            _ => false,
        };
        if !matches || other.container != atom.container {
            return Err(error("plan text marks must retain their paired anchors"));
        }
    }
    Ok(())
}

fn validate_lamports(page: &Page, seed: &CanonicalPlanSeed, incoming: &Atoms) -> Result<()> {
    let mut known = BTreeMap::new();
    for atom in incoming.values() {
        let mut expected = 0_u32;
        for &(peer, counter) in &atom.deps {
            let id = (peer, counter);
            let lamport = if let Some(dep) = incoming.get(&id) {
                dep.lamport
            } else if let Some(value) = known.get(&id) {
                *value
            } else {
                let doc = if peer == MIGRATION_PEER {
                    &seed.0.page.doc
                } else {
                    &page.doc
                };
                let change = doc
                    .get_change(ID::new(peer, counter))
                    .ok_or(PlanError::MissingDependencies)?;
                let value = change
                    .lamport
                    .checked_add((counter - change.id.counter) as u32)
                    .ok_or_else(|| error("plan Lamport overflow"))?;
                known.insert(id, value);
                value
            };
            expected = expected.max(
                lamport
                    .checked_add(1)
                    .ok_or_else(|| error("plan Lamport overflow"))?,
            );
        }
        if atom.lamport != expected {
            return Err(error("plan Lamport does not match causal dependencies"));
        }
    }
    Ok(())
}

fn atoms(schema: &JsonSchema) -> Result<Atoms> {
    if schema.schema_version != 1 || schema.peers.is_some() {
        return Err(PlanError::UnsupportedCodec);
    }
    if schema.changes.len() > MAX_PLAN_CHANGES || schema.start_version.len() > MAX_DEPS {
        return Err(PlanError::Limit("plan changes or dependencies"));
    }
    for id in schema.start_version.iter() {
        valid_id(id)?;
    }
    let mut result = Atoms::new();
    let mut raw_ops = 0usize;
    let mut expanded = 0usize;
    for change in &schema.changes {
        valid_id(change.id)?;
        if change
            .msg
            .as_ref()
            .is_some_and(|m| m.len() > crate::MAX_SPAN_VALUE_BYTES)
        {
            return Err(PlanError::Limit("plan commit message"));
        }
        if change.ops.is_empty() || change.deps.len() > MAX_DEPS {
            return Err(error("invalid plan change or dependencies"));
        }
        raw_ops = raw_ops
            .checked_add(change.ops.len())
            .filter(|n| *n <= MAX_PLAN_RAW_OPS)
            .ok_or(PlanError::Limit("plan raw operations"))?;
        let mut peers = HashSet::new();
        let mut initial_deps = Vec::with_capacity(change.deps.len());
        for id in &change.deps {
            valid_id(*id)?;
            if !peers.insert(id.peer)
                || (id.peer == change.id.peer && id.counter >= change.id.counter)
            {
                return Err(error("invalid plan causal dependencies"));
            }
            initial_deps.push((id.peer, id.counter));
        }
        initial_deps.sort_unstable();
        let message = change.msg.as_deref().map(Arc::<str>::from);
        let mut counter = change.id.counter;
        for op in &change.ops {
            if op.counter != counter {
                return Err(error("plan counters must be contiguous"));
            }
            validate_op(op, change.id.peer)?;
            let len = op.content.op_len();
            if len == 0 {
                return Err(error("empty plan operation"));
            }
            expanded = expanded
                .checked_add(len)
                .filter(|n| *n <= MAX_PLAN_ATOMS)
                .ok_or(PlanError::Limit("plan operation atoms"))?;
            let end = counter
                .checked_add(i32::try_from(len).map_err(error)?)
                .ok_or_else(|| error("plan counter overflow"))?;
            change
                .lamport
                .checked_add(u32::try_from(end - change.id.counter).map_err(error)?)
                .ok_or_else(|| error("plan Lamport overflow"))?;
            // Text is visited once: repeated nth()/char slicing would be quadratic.
            let contents = atom_contents(op)?;
            for (offset, content) in contents.into_iter().enumerate() {
                let ctr = counter + offset as i32;
                let deps = if ctr == change.id.counter {
                    initial_deps.clone()
                } else {
                    vec![(change.id.peer, ctr - 1)]
                };
                let atom = Atom {
                    container: op.container.clone(),
                    content,
                    deps,
                    lamport: change.lamport + (ctr - change.id.counter) as u32,
                    reserved_provenance: (change.id.peer == MIGRATION_PEER)
                        .then(|| (change.timestamp, message.clone())),
                };
                let id = (change.id.peer, ctr);
                if let Some(previous) = result.get(&id) {
                    if previous != &atom {
                        return Err(conflict(change.id.peer));
                    }
                } else {
                    result.insert(id, atom);
                }
            }
            counter = end;
        }
    }
    Ok(result)
}

fn valid_id(id: ID) -> Result<()> {
    if id.counter < 0 || id.peer == u64::MAX {
        return Err(error("invalid plan operation ID"));
    }
    Ok(())
}

fn validate_op(op: &JsonOp, peer: u64) -> Result<()> {
    match &op.container {
        ContainerID::Root {
            name,
            container_type,
        } => {
            if !((name.as_str() == "page" && *container_type == ContainerType::Map)
                || (name.as_str() == "blocks" && *container_type == ContainerType::MovableList))
            {
                return Err(error("unsupported plan root container"));
            }
        }
        ContainerID::Normal { peer, counter, .. } => valid_id(ID::new(*peer, *counter))?,
    }
    match (&op.content, op.container.container_type()) {
        (JsonOpContent::Map(map), ContainerType::Map) => validate_map(map, peer),
        (JsonOpContent::Text(text), ContainerType::Text) => {
            if let JsonTextOp::Mark {
                start,
                end,
                style_key,
                style_value,
                info,
            } = text
            {
                if start >= end || style_key.len() > crate::MAX_BLOCK_ID_BYTES {
                    return Err(error("invalid plan text mark"));
                }
                let value = serde_json::to_vec(style_value).map_err(error)?;
                if value.len() > crate::MAX_SPAN_VALUE_BYTES {
                    return Err(PlanError::Limit("plan style value"));
                }
                json_depth(&serde_json::to_value(style_value).map_err(error)?, 0)?;
                super::edit::validate_mark_value(style_key, style_value)?;
                // Pinned public JSON encoding uses bit 7 for alive, bit 1 for before
                // and bit 2 for after. Unmark keeps after expansion; none becomes both.
                // The real Loro mark/unmark export fixture checks these encoding values.
                let expands_after = matches!(style_key.as_str(), "b" | "i" | "s");
                let expected = if matches!(style_value, LoroValue::Null) {
                    if expands_after {
                        0x84
                    } else {
                        0x86
                    }
                } else if expands_after {
                    0x84
                } else {
                    0x80
                };
                if *info != expected {
                    return Err(error("plan mark expansion does not match its grammar"));
                }
            }
            if let JsonTextOp::Delete { pos, len, start_id } = text {
                validate_delete(*pos, *len, *start_id)?;
            }
            Ok(())
        }
        (JsonOpContent::MovableList(list), ContainerType::MovableList) => {
            if !matches!(&op.container, ContainerID::Root { name, .. } if name.as_str() == "blocks")
            {
                return Err(error("plan has only one root block list"));
            }
            match list {
                JsonMovableListOp::Insert { value, .. } => {
                    for value in value {
                        block_container(value)?;
                    }
                }
                JsonMovableListOp::Set { value, .. } => block_container(value)?,
                JsonMovableListOp::Delete { pos, len, start_id } => {
                    validate_delete(*pos, *len, *start_id)?;
                }
                JsonMovableListOp::Move { .. } => {}
            }
            Ok(())
        }
        _ => Err(error("unsupported plan operation/container type")),
    }
}

fn block_container(value: &LoroValue) -> Result<()> {
    match value {
        LoroValue::Container(ContainerID::Normal {
            peer,
            counter,
            container_type: ContainerType::Map,
        }) => valid_id(ID::new(*peer, *counter)),
        _ => Err(error("plan list entries must be block map containers")),
    }
}

fn validate_map(map: &JsonMapOp, peer: u64) -> Result<()> {
    let JsonMapOp::Insert { key, value } = map else {
        return Err(error("plan attributes cannot be deleted"));
    };
    let valid = match (key.as_str(), value) {
        ("id", LoroValue::String(s)) => !s.is_empty() && s.len() <= crate::MAX_BLOCK_ID_BYTES,
        ("line_id", LoroValue::String(_)) => true,
        ("kind", LoroValue::String(s)) => {
            matches!(
                s.as_str(),
                "plan_title" | "plan_summary" | "plan_section" | "plan_line" | "plan_prose"
            )
        }
        ("schema", LoroValue::String(s)) => s.as_str() == SCHEMA,
        ("root", LoroValue::String(s)) => !s.is_empty(),
        ("prose_kind", LoroValue::String(s)) => super::edit::parse_prose_kind(s).is_ok(),
        (
            "text",
            LoroValue::Container(ContainerID::Normal {
                peer,
                counter,
                container_type: ContainerType::Text,
            }),
        ) => {
            valid_id(ID::new(*peer, *counter))?;
            true
        }
        // Authenticated legacy replay must retain exact signed values. Only the
        // reserved tape may introduce negatives; validate() then binds every
        // reserved atom to the independently replayed seed before import.
        ("cost_cents" | "budget_cents", LoroValue::I64(n)) => peer == MIGRATION_PEER || *n >= 0,
        ("budget_cents", LoroValue::Null) | ("done", LoroValue::Bool(_)) => true,
        _ => false,
    };
    if !valid {
        return Err(error("unsupported or invalid plan attribute"));
    }
    Ok(())
}

fn validate_delete(pos: i32, len: i32, start_id: ID) -> Result<()> {
    valid_id(start_id)?;
    if len == 0
        || pos < 0
        || (len < 0 && i64::from(pos) + 1 + i64::from(len) < 0)
        || i64::from(start_id.counter) + i64::from(len.unsigned_abs()) > i64::from(i32::MAX)
    {
        return Err(error("invalid plan deletion range"));
    }
    Ok(())
}

// These are single-operation slices, not a private binary decoder. The pinned Loro
// public export_json_in_id_span is the real oracle in the split-span regression.
fn atom_contents(op: &JsonOp) -> Result<Vec<Value>> {
    let mut atoms = Vec::with_capacity(op.content.op_len());
    match &op.content {
        JsonOpContent::Text(JsonTextOp::Insert { pos, text }) => {
            for (offset, scalar) in text.chars().enumerate() {
                atoms.push(
                    serde_json::to_value(JsonTextOp::Insert {
                        pos: pos
                            .checked_add(offset as u32)
                            .ok_or_else(|| error("text position overflow"))?,
                        text: scalar.to_string(),
                    })
                    .map_err(error)?,
                );
            }
        }
        JsonOpContent::MovableList(JsonMovableListOp::Insert { pos, value }) => {
            for (offset, value) in value.iter().enumerate() {
                atoms.push(
                    serde_json::to_value(JsonMovableListOp::Insert {
                        pos: pos
                            .checked_add(offset as u32)
                            .ok_or_else(|| error("list position overflow"))?,
                        value: vec![value.clone()],
                    })
                    .map_err(error)?,
                );
            }
        }
        JsonOpContent::Text(JsonTextOp::Delete { pos, len, start_id })
        | JsonOpContent::MovableList(JsonMovableListOp::Delete { pos, len, start_id }) => {
            for offset in 0..len.unsigned_abs() as i32 {
                let target_offset = if *len > 0 {
                    offset
                } else {
                    len.unsigned_abs() as i32 - 1 - offset
                };
                let position = if *len > 0 {
                    *pos
                } else {
                    pos.checked_sub(offset)
                        .ok_or_else(|| error("delete position overflow"))?
                };
                // A one-element deletion has no direction. Loro may merge it into
                // forward or backward runs while retaining exactly the same OpID.
                atoms.push(serde_json::json!({
                    "type": "delete", "pos": position, "len": 1,
                    "start_id": ID::new(start_id.peer, start_id.counter + target_offset).to_string(),
                }));
            }
        }
        _ => atoms.push(serde_json::to_value(&op.content).map_err(error)?),
    }
    Ok(atoms)
}

#[cfg(test)]
mod tests {
    use super::super::{
        read_nodes, BlockKind, ExpectedLine, LegacyVersion, LinePatch, PlanCommand, PlanDocument,
    };
    use super::*;
    use loro::{ContainerTrait, ExportMode, JsonChange};
    use zoen_types::{PlanDoc, PlanLine, PlanSection};

    fn seed() -> CanonicalPlanSeed {
        CanonicalPlanSeed::replay(&[LegacyVersion {
            event_id: "creation".into(),
            document: PlanDoc {
                title: "Trip".into(),
                summary: "".into(),
                budget_cents: None,
                sections: vec![PlanSection {
                    title: "Stay".into(),
                    lines: vec![PlanLine {
                        id: "original".into(),
                        text: "abcdefghi".into(),
                        cost_cents: 100,
                        done: false,
                    }],
                }],
            },
        }])
        .unwrap()
    }

    fn line_id(doc: &PlanDocument) -> String {
        doc.view().unwrap().sections[0].lines[0].block_id.clone()
    }

    fn change_cost(doc: &mut PlanDocument, cents: i64) {
        doc.apply(PlanCommand::ChangeLine {
            block: line_id(doc),
            expected: ExpectedLine::AnyCurrent,
            patch: LinePatch {
                cost_cents: Some(cents),
                ..Default::default()
            },
        })
        .unwrap();
    }

    fn raw_packet(schema: &JsonSchema) -> Vec<u8> {
        serde_json::to_vec(&EncodedEnvelope {
            format: FORMAT,
            version: VERSION,
            operations: schema,
        })
        .unwrap()
    }

    fn split(doc: &PlanDocument, update: &PlanUpdate) -> PlanUpdate {
        let mut changes = Vec::new();
        for change in &update.0.changes {
            for counter in change.id.counter..change.id.counter + change.op_len() as i32 {
                changes.extend(doc.page.doc.export_json_in_id_span(IdSpan::new(
                    change.id.peer,
                    counter,
                    counter + 1,
                )));
            }
        }
        PlanUpdate(JsonSchema {
            changes,
            ..update.0.clone()
        })
    }

    fn unchanged(
        doc: &PlanDocument,
        vv: &super::super::PlanVersion,
        view: &super::super::PlanView,
    ) {
        assert_eq!(&doc.version(), vv);
        assert_eq!(&doc.view().unwrap(), view);
    }

    #[test]
    fn versioned_roundtrip_is_explicit_and_never_falls_back_to_binary() {
        let seed = seed();
        let doc = seed.open_editor().unwrap();
        let update = doc.full_update();
        let bytes = update.to_json().unwrap();
        let raw: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(raw["format"], FORMAT);
        assert_eq!(raw["version"], VERSION);
        assert_eq!(
            PlanUpdate::from_json(&bytes).unwrap().to_json().unwrap(),
            bytes
        );
        let duplicate = String::from_utf8(bytes.clone()).unwrap().replacen(
            "\"version\":1",
            "\"version\":2,\"version\":1",
            1,
        );
        assert!(PlanUpdate::from_json(duplicate.as_bytes()).is_err());
        assert!(PlanUpdate::from_json(&serde_json::to_vec(&update.0).unwrap()).is_err());
        assert!(
            PlanUpdate::from_json(&doc.page.doc.export(ExportMode::Snapshot).unwrap()).is_err()
        );
        let mut unsupported = raw.clone();
        unsupported["version"] = Value::from(2);
        assert_eq!(
            PlanUpdate::from_json(&serde_json::to_vec(&unsupported).unwrap()).unwrap_err(),
            PlanError::UnsupportedCodec
        );
        let mut ignored = raw;
        ignored["operations"]["changes"][0]["ops"][0]["ignored_field"] = Value::Bool(true);
        assert!(PlanUpdate::from_json(&serde_json::to_vec(&ignored).unwrap()).is_err());
    }

    #[test]
    fn known_ordinary_collision_cannot_hide_behind_an_exact_replayed_id() {
        let seed = seed();
        let mut author = seed.open_editor().unwrap();
        let mut receiver = seed.open_editor().unwrap();
        let from = author.version();
        change_cost(&mut author, 700);
        let good = author.updates_since(&from).unwrap();
        receiver
            .import(&PlanUpdate::from_json(&good.to_json().unwrap()).unwrap())
            .unwrap();
        let vv = receiver.version();
        let view = receiver.view().unwrap();
        let mut bad = good.clone();
        let JsonOpContent::Map(JsonMapOp::Insert { value, .. }) =
            &mut bad.0.changes[0].ops[0].content
        else {
            panic!("real cost register")
        };
        *value = LoroValue::I64(701);
        let decoded = PlanUpdate::from_json(&bad.to_json().unwrap()).unwrap();
        assert_eq!(
            receiver.import(&decoded).unwrap_err(),
            PlanError::OperationConflict
        );
        unchanged(&receiver, &vv, &view);
        receiver.import(&good).unwrap();
        unchanged(&receiver, &vv, &view);
    }

    #[test]
    fn ordinary_lamport_is_immutable_and_real_slices_keep_the_same_causal_value() {
        let seed = seed();
        let mut author = seed.open_editor().unwrap();
        let from = author.version();
        author
            .apply(PlanCommand::ChangeLine {
                block: line_id(&author),
                expected: ExpectedLine::AnyCurrent,
                patch: LinePatch {
                    text: Some("🐟road🚀".into()),
                    ..Default::default()
                },
            })
            .unwrap();
        let good = author.updates_since(&from).unwrap();
        let sliced = split(&author, &good);
        assert_eq!(atoms(&good.0).unwrap(), atoms(&sliced.0).unwrap());
        let mut receiver = seed.open_editor().unwrap();
        receiver.import(&good).unwrap();
        let vv = receiver.version();
        let view = receiver.view().unwrap();
        let mut forged = sliced.clone();
        forged.0.changes[0].lamport += 1;
        assert_eq!(
            receiver.import(&PlanUpdate::from_json(&forged.to_json().unwrap()).unwrap()),
            Err(PlanError::OperationConflict)
        );
        unchanged(&receiver, &vv, &view);
        receiver.import(&sliced).unwrap();
        unchanged(&receiver, &vv, &view);

        // Even a fresh receiver refuses a forged declared Lamport instead of
        // accepting bytes that Loro would recalculate and then reject on retry.
        let mut fresh = seed.open_editor().unwrap();
        let vv = fresh.version();
        let view = fresh.view().unwrap();
        let mut forged = good.clone();
        forged.0.changes[0].lamport += 1;
        assert_eq!(
            fresh.import(&PlanUpdate::from_json(&forged.to_json().unwrap()).unwrap()),
            Err(error("plan Lamport does not match causal dependencies"))
        );
        unchanged(&fresh, &vv, &view);
        fresh.import(&good).unwrap();
        assert_eq!(fresh.view().unwrap(), author.view().unwrap());

        // Ordinary commit grouping metadata is not operation identity.
        let mut regrouped = sliced;
        for change in &mut regrouped.0.changes {
            change.timestamp = 99;
            change.msg = Some("regrouped ordinary commit".into());
        }
        receiver.import(&regrouped).unwrap();
        assert_eq!(receiver.view().unwrap(), author.view().unwrap());
    }

    #[test]
    fn conflicting_overlaps_inside_one_packet_are_refused_before_first_import() {
        let seed = seed();
        let mut author = seed.open_editor().unwrap();
        let from = author.version();
        change_cost(&mut author, 700);
        let mut packet = author.updates_since(&from).unwrap();
        let mut duplicate = packet.0.changes[0].clone();
        let JsonOpContent::Map(JsonMapOp::Insert { value, .. }) = &mut duplicate.ops[0].content
        else {
            panic!("real cost register")
        };
        *value = LoroValue::I64(701);
        packet.0.changes.push(duplicate);
        assert_eq!(
            PlanUpdate::from_json(&raw_packet(&packet.0)).unwrap_err(),
            PlanError::OperationConflict
        );
        let mut receiver = seed.open_editor().unwrap();
        let vv = receiver.version();
        let view = receiver.view().unwrap();
        assert_eq!(
            receiver.import(&packet).unwrap_err(),
            PlanError::OperationConflict
        );
        unchanged(&receiver, &vv, &view);
        packet.0.changes.pop();
        packet.0.changes.push(packet.0.changes[0].clone());
        let retry = PlanUpdate::from_json(&raw_packet(&packet.0)).unwrap();
        receiver.import(&retry).unwrap();
        let vv = receiver.version();
        let view = receiver.view().unwrap();
        receiver.import(&retry).unwrap();
        unchanged(&receiver, &vv, &view);
        assert_eq!(view.sections[0].lines[0].line.cost_cents, 700);
    }

    #[test]
    fn canonical_atom_normalization_matches_real_unicode_and_reverse_delete_slices() {
        let seed = seed();
        let author = seed.open_editor().unwrap();
        author.page.doc.set_peer_id(17).unwrap();
        let from = author.version();
        let nodes = read_nodes(&author.page, seed.root()).unwrap();
        let line = nodes.iter().find(|n| n.kind == BlockKind::Line).unwrap();
        line.text.insert(2, "🐟é\u{2028}").unwrap();
        // Forward run, then actual repeated backspaces that Loro merges into a reverse run.
        line.text.delete(0, 2).unwrap();
        line.text.delete(7, 1).unwrap();
        line.text.delete(6, 1).unwrap();
        line.text.delete(5, 1).unwrap();
        author.page.doc.commit();
        let packet = author.updates_since(&from).unwrap();
        assert!(packet.0.changes.iter().flat_map(|c| &c.ops).any(|op| matches!(&op.content, JsonOpContent::Text(JsonTextOp::Delete { len, .. }) if *len < -1)), "fixture must contain an actual reverse run");
        let sliced = split(&author, &packet);
        assert_eq!(atoms(&packet.0).unwrap(), atoms(&sliced.0).unwrap());
        let mut a = seed.open_editor().unwrap();
        let mut b = seed.open_editor().unwrap();
        a.import(&PlanUpdate::from_json(&packet.to_json().unwrap()).unwrap())
            .unwrap();
        b.import(&PlanUpdate::from_json(&sliced.to_json().unwrap()).unwrap())
            .unwrap();
        assert_eq!(a.view().unwrap(), author.view().unwrap());
        assert_eq!(b.view().unwrap(), author.view().unwrap());
        let before = a.version();
        a.import(&sliced).unwrap();
        assert_eq!(a.version(), before);
    }

    #[test]
    fn reserved_prefix_accepts_valid_splits_but_refuses_a_forged_slice() {
        let seed = seed();
        let doc = seed.open_editor().unwrap();
        let packet = split(&doc, &doc.full_update());
        let mut receiver = seed.open_editor().unwrap();
        receiver
            .import(&PlanUpdate::from_json(&packet.to_json().unwrap()).unwrap())
            .unwrap();
        assert_eq!(receiver.view().unwrap(), doc.view().unwrap());
        let vv = receiver.version();
        let view = receiver.view().unwrap();
        let mut forged = packet;
        let change = forged.0.changes.iter_mut().find(|c| c.ops.iter().any(|op| matches!(&op.content, JsonOpContent::Map(JsonMapOp::Insert { key, .. }) if key == "cost_cents"))).unwrap();
        let JsonOpContent::Map(JsonMapOp::Insert { value, .. }) = &mut change.ops[0].content else {
            unreachable!()
        };
        *value = LoroValue::I64(999);
        assert_eq!(
            receiver
                .import(&PlanUpdate::from_json(&forged.to_json().unwrap()).unwrap())
                .unwrap_err(),
            PlanError::ReservedSeed
        );
        unchanged(&receiver, &vv, &view);
    }

    #[test]
    fn a_valid_overlapping_span_replays_and_a_forged_overlap_refuses_atomically() {
        let seed = seed();
        let mut author = seed.open_editor().unwrap();
        let from = author.version();
        author
            .apply(PlanCommand::ChangeLine {
                block: line_id(&author),
                expected: ExpectedLine::AnyCurrent,
                patch: LinePatch {
                    text: Some("🐟road🚀".into()),
                    ..Default::default()
                },
            })
            .unwrap();
        let full = author.updates_since(&from).unwrap();
        let sliced = split(&author, &full);
        let mut receiver = seed.open_editor().unwrap();
        let cut = sliced.0.changes.len() / 2;
        let partial = PlanUpdate(JsonSchema {
            changes: sliced.0.changes[..cut].to_vec(),
            ..sliced.0.clone()
        });
        receiver
            .import(&PlanUpdate::from_json(&partial.to_json().unwrap()).unwrap())
            .unwrap();
        let vv = receiver.version();
        let view = receiver.view().unwrap();
        let mut forged = full.clone();
        let peer = forged.0.changes[0].id.peer;
        let known_end = *receiver.page.version().get(&peer).unwrap();
        let op = forged
            .0
            .changes
            .iter_mut()
            .flat_map(|c| &mut c.ops)
            .find(|op| op.counter < known_end)
            .unwrap();
        if let JsonOpContent::Text(JsonTextOp::Delete { pos, .. }) = &mut op.content {
            *pos += 1;
        } else {
            panic!("fixture's first text operation is a deletion")
        }
        assert_eq!(
            receiver
                .import(&PlanUpdate::from_json(&forged.to_json().unwrap()).unwrap())
                .unwrap_err(),
            PlanError::OperationConflict
        );
        unchanged(&receiver, &vv, &view);
        receiver
            .import(&PlanUpdate::from_json(&full.to_json().unwrap()).unwrap())
            .unwrap();
        assert_eq!(receiver.view().unwrap(), author.view().unwrap());
    }

    #[test]
    fn missing_dependency_packet_retries_with_identical_bytes_after_predecessor() {
        let seed = seed();
        let author = seed.open_editor().unwrap();
        author.page.doc.set_peer_id(77).unwrap();
        let from = author.version();
        let nodes = read_nodes(&author.page, seed.root()).unwrap();
        let node = nodes.iter().find(|n| n.kind == BlockKind::Line).unwrap();
        node.text.insert(0, "arrived ").unwrap();
        author.page.doc.commit();
        let first = author.updates_since(&from).unwrap();
        let middle = author.version();
        node.map.insert("cost_cents", 444_i64).unwrap();
        author.page.doc.commit();
        let second = author.updates_since(&middle).unwrap().to_json().unwrap();
        let mut receiver = seed.open_editor().unwrap();
        let vv = receiver.version();
        let view = receiver.view().unwrap();
        assert_eq!(
            receiver
                .import(&PlanUpdate::from_json(&second).unwrap())
                .unwrap_err(),
            PlanError::MissingDependencies
        );
        unchanged(&receiver, &vv, &view);
        receiver
            .import(&PlanUpdate::from_json(&first.to_json().unwrap()).unwrap())
            .unwrap();
        receiver
            .import(&PlanUpdate::from_json(&second).unwrap())
            .unwrap();
        assert_eq!(receiver.view().unwrap(), author.view().unwrap());
        let vv = receiver.version();
        receiver
            .import(&PlanUpdate::from_json(&second).unwrap())
            .unwrap();
        assert_eq!(receiver.version(), vv);
    }

    #[test]
    fn legacy_negatives_survive_but_new_raw_operations_cannot_bypass_commands() {
        let mut legacy = seed().legacy_at(1).unwrap().document.clone();
        legacy.budget_cents = Some(i64::MIN);
        legacy.sections[0].lines[0].cost_cents = -31;
        let seed = CanonicalPlanSeed::replay(&[LegacyVersion {
            event_id: "negative-creation".into(),
            document: legacy.clone(),
        }])
        .unwrap();
        let mut receiver = seed.open_editor().unwrap();
        let split_seed = split(&receiver, &receiver.full_update());
        receiver
            .import(&PlanUpdate::from_json(&split_seed.to_json().unwrap()).unwrap())
            .unwrap();
        assert_eq!(receiver.view().unwrap().to_plan_doc(), legacy);

        let mut author = seed.open_editor().unwrap();
        let from = author.version();
        author
            .apply(PlanCommand::ChangeLine {
                block: line_id(&author),
                expected: ExpectedLine::AnyCurrent,
                patch: LinePatch {
                    text: Some("historical negative cost retained".into()),
                    ..Default::default()
                },
            })
            .unwrap();
        receiver
            .import(&author.updates_since(&from).unwrap())
            .unwrap();
        assert_eq!(receiver.view().unwrap().budget_cents, Some(i64::MIN));
        assert_eq!(
            receiver.view().unwrap().sections[0].lines[0]
                .line
                .cost_cents,
            -31
        );

        for field in ["cost_cents", "budget_cents"] {
            let mut attacker = receiver.fork().unwrap();
            let vv = receiver.version();
            let view = receiver.view().unwrap();
            let command = if field == "cost_cents" {
                PlanCommand::ChangeLine {
                    block: line_id(&attacker),
                    expected: ExpectedLine::AnyCurrent,
                    patch: LinePatch {
                        cost_cents: Some(-1),
                        ..Default::default()
                    },
                }
            } else {
                PlanCommand::SetBudget(Some(-1))
            };
            assert!(attacker.apply(command).is_err());
            unchanged(&attacker, &vv, &view);

            // Generate real Loro operations outside PlanCommand, as a remote
            // peer could do. The packet grammar must enforce the same rule.
            let nodes = read_nodes(&attacker.page, seed.root()).unwrap();
            let node = if field == "cost_cents" {
                nodes.iter().find(|n| n.kind == BlockKind::Line).unwrap()
            } else {
                nodes.iter().find(|n| n.kind == BlockKind::Title).unwrap()
            };
            node.map.insert(field, -1_i64).unwrap();
            attacker.page.doc.commit();
            let update = attacker.updates_since(&vv).unwrap();
            assert!(PlanUpdate::from_json(&raw_packet(&update.0)).is_err());
            assert!(receiver.import(&update).is_err());
            unchanged(&receiver, &vv, &view);
        }
    }

    #[test]
    fn bounded_codec_refuses_expansion_and_counter_arithmetic_before_import() {
        let seed = seed();
        let doc = seed.open_editor().unwrap();
        assert_eq!(
            PlanUpdate::from_json(&vec![b' '; MAX_PLAN_PACKET_BYTES + 1]).unwrap_err(),
            PlanError::Limit("plan packet bytes")
        );
        let deep = format!(
            "{}0{}",
            "[".repeat(MAX_JSON_DEPTH + 1),
            "]".repeat(MAX_JSON_DEPTH + 1)
        );
        assert_eq!(
            PlanUpdate::from_json(deep.as_bytes()).unwrap_err(),
            PlanError::Limit("plan JSON depth")
        );
        let broad = format!("[{}0]", "0,".repeat(MAX_JSON_NODES));
        assert!(broad.len() < MAX_PLAN_PACKET_BYTES);
        assert_eq!(
            PlanUpdate::from_json(broad.as_bytes()).unwrap_err(),
            PlanError::Limit("plan JSON nodes")
        );
        let nodes = read_nodes(&doc.page, seed.root()).unwrap();
        let node = nodes.iter().find(|n| n.kind == BlockKind::Line).unwrap();
        let mut raw = doc.full_update().0;
        raw.changes = vec![JsonChange {
            id: ID::new(88, 0),
            timestamp: 0,
            deps: vec![],
            lamport: 0,
            msg: None,
            ops: vec![JsonOp {
                counter: 0,
                container: node.text.id(),
                content: JsonOpContent::Text(JsonTextOp::Delete {
                    pos: 0,
                    len: MAX_PLAN_ATOMS as i32 + 1,
                    start_id: ID::new(0, 0),
                }),
            }],
        }];
        assert_eq!(
            PlanUpdate::from_json(&raw_packet(&raw)).unwrap_err(),
            PlanError::Limit("plan operation atoms")
        );
        raw.changes[0].id.counter = i32::MAX;
        raw.changes[0].ops[0].counter = i32::MAX;
        raw.changes[0].ops[0].content = JsonOpContent::Text(JsonTextOp::Insert {
            pos: 0,
            text: "x".into(),
        });
        assert!(PlanUpdate::from_json(&raw_packet(&raw)).is_err());
        assert_eq!(
            doc.view().unwrap().sections[0].lines[0].line.text,
            "abcdefghi"
        );
    }

    #[test]
    fn removed_block_identity_cannot_be_reassigned_through_an_invisible_map() {
        let seed = seed();
        let mut doc = seed.open_editor().unwrap();
        let original = read_nodes(&doc.page, seed.root())
            .unwrap()
            .into_iter()
            .find(|n| n.kind == BlockKind::Line)
            .unwrap();
        // The remote branch still has this map alive while the receiver removes
        // it. Mutating after deletion is refused by Loro before admission.
        let attacker = doc.fork().unwrap();
        let from = attacker.version();
        attacker
            .page
            .doc
            .get_map(original.map.id())
            .insert("id", "new-line")
            .unwrap();
        attacker.page.doc.commit();
        let update = attacker.updates_since(&from).unwrap();
        doc.apply(PlanCommand::RemoveLine {
            block: original.id.clone(),
            expected: ExpectedLine::AnyCurrent,
        })
        .unwrap();
        let vv = doc.version();
        let view = doc.view().unwrap();
        assert!(doc
            .import(&PlanUpdate::from_json(&update.to_json().unwrap()).unwrap())
            .is_err());
        unchanged(&doc, &vv, &view);
    }
}
