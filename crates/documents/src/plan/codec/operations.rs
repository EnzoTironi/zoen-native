//! Allowed Plan container grammar, attribute values and operation slicing.

use loro::{
    ContainerID, ContainerType, JsonMapOp, JsonMovableListOp, JsonOp, JsonOpContent, JsonTextOp,
    LoroValue, ID,
};
use serde_json::Value;

use super::super::{error, PlanError, Result, MIGRATION_PEER, SCHEMA};
use super::json::json_depth;

pub(super) fn valid_id(id: ID) -> Result<()> {
    if id.counter < 0 || id.peer == u64::MAX {
        return Err(error("invalid plan operation ID"));
    }
    Ok(())
}

pub(super) fn validate_op(op: &JsonOp, peer: u64) -> Result<()> {
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
                super::super::edit::validate_mark_value(style_key, style_value)?;
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
        ("prose_kind", LoroValue::String(s)) => super::super::edit::parse_prose_kind(s).is_ok(),
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
pub(super) fn atom_contents(op: &JsonOp) -> Result<Vec<Value>> {
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
