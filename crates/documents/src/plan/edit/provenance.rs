//! Validate operation provenance and historical block ownership before admission.
use super::super::{
    codec, error, new_block_id, parse_role, read_node, role_key, string, BlockKind, MIGRATION_PEER,
};
use super::{
    ContainerID, ContainerType, HashMap, HashSet, JsonMapOp, JsonMovableListOp, JsonOpContent,
    LoroValue, Page, PlanUpdate, Result, ID,
};
use loro::VersionVector;

pub(super) fn historical_ids(page: &Page) -> Result<HashMap<String, HashSet<ContainerID>>> {
    let update =
        PlanUpdate(page.doc.export_json_updates_without_peer_compression(
            &VersionVector::default(),
            &page.version(),
        ));
    codec::fingerprint(&update)?;
    let mut result: HashMap<String, HashSet<ContainerID>> = HashMap::new();
    for change in update.0.changes {
        for op in change.ops {
            if let JsonOpContent::Map(JsonMapOp::Insert {
                key,
                value: LoroValue::String(id),
            }) = op.content
            {
                if key == "id" {
                    result
                        .entry(id.to_string())
                        .or_default()
                        .insert(op.container);
                }
            }
        }
    }
    Ok(result)
}

pub(in crate::plan) fn unused_id(page: &Page) -> Result<String> {
    let history = historical_ids(page)?;
    loop {
        let id = new_block_id()?;
        if !history.contains_key(&id) {
            return Ok(id);
        }
    }
}

pub(in crate::plan) fn validate_operations(page: &Page, update: &PlanUpdate) -> Result<()> {
    let vv = page.version();
    let mut ids = historical_ids(page)?;
    let mut roles = HashMap::new();
    for change in &update.0.changes {
        for op in &change.ops {
            if let JsonOpContent::Map(JsonMapOp::Insert {
                key,
                value: LoroValue::String(role),
            }) = &op.content
            {
                if key == "kind" {
                    roles.insert(op.container.clone(), parse_role(role)?);
                }
                if key == "id" && change.id.peer == MIGRATION_PEER {
                    ids.entry(role.to_string())
                        .or_default()
                        .insert(op.container.clone());
                }
            }
        }
    }
    for change in &update.0.changes {
        for op in &change.ops {
            if op.counter < *vv.get(&change.id.peer).unwrap_or(&0) {
                continue;
            }
            match &op.content {
                JsonOpContent::Map(JsonMapOp::Insert { key, value }) => {
                    if matches!(&op.container, ContainerID::Root { name, .. } if name.as_str() == "page")
                    {
                        if !matches!(key.as_str(), "schema" | "root") {
                            return Err(error("unknown plan root attribute"));
                        }
                        continue;
                    }
                    let role = page
                        .doc
                        .try_get_map(op.container.clone())
                        .and_then(|map| string(&map, "kind").ok())
                        .map(|role| parse_role(&role))
                        .transpose()?
                        .or_else(|| roles.get(&op.container).copied())
                        .ok_or_else(|| error("plan attributes need a typed block role"))?;
                    if !role_key(role, key) {
                        return Err(error("attribute does not belong to its plan block role"));
                    }
                    if change.id.peer != MIGRATION_PEER {
                        if key == "id" {
                            let LoroValue::String(id) = value else {
                                return Err(error("invalid block ID"));
                            };
                            let owners = ids.entry(id.to_string()).or_default();
                            if owners.iter().any(|owner| *owner != op.container) {
                                return Err(error("a historical plan block ID cannot be reused"));
                            }
                            owners.insert(op.container.clone());
                        }
                        if key == "text" {
                            let expected = ContainerID::new_normal(
                                ID::new(change.id.peer, op.counter),
                                ContainerType::Text,
                            );
                            if value != &LoroValue::Container(expected) {
                                return Err(error("a new block must own its rich text container"));
                            }
                        }
                    }
                }
                JsonOpContent::MovableList(JsonMovableListOp::Insert { value, .. }) => {
                    if change.id.peer != MIGRATION_PEER {
                        validate_new_entries(change.id.peer, op.counter, value)?;
                    }
                }
                JsonOpContent::MovableList(JsonMovableListOp::Set { value, .. })
                    if change.id.peer != MIGRATION_PEER =>
                {
                    let expected = ContainerID::new_normal(
                        ID::new(change.id.peer, op.counter),
                        ContainerType::Map,
                    );
                    if value != &LoroValue::Container(expected) {
                        return Err(error("a replacement list entry must own a new block map"));
                    }
                }
                _ => {}
            }
        }
    }
    Ok(())
}

pub(in crate::plan) fn validate_touched_maps(page: &Page, update: &PlanUpdate) -> Result<()> {
    let mut maps = HashSet::new();
    for change in &update.0.changes {
        for op in &change.ops {
            if op.container.container_type() == ContainerType::Map
                && matches!(op.container, ContainerID::Normal { .. })
            {
                maps.insert(op.container.clone());
            }
        }
    }
    for address in maps {
        let map = page
            .doc
            .try_get_map(address)
            .ok_or_else(|| error("missing typed block map"))?;
        let node = read_node(map)?;
        if node.kind == BlockKind::Line
            && node.map.get_last_editor("line_id") != Some(MIGRATION_PEER)
            && string(&node.map, "line_id")? != format!("ln_{}", node.id)
        {
            return Err(error("new line identity must derive from its block ID"));
        }
    }
    Ok(())
}

fn validate_new_entries(peer: u64, counter: i32, values: &[LoroValue]) -> Result<()> {
    for (offset, value) in values.iter().enumerate() {
        let expected = ContainerID::new_normal(
            ID::new(
                peer,
                counter
                    .checked_add(i32::try_from(offset).map_err(error)?)
                    .ok_or_else(|| error("plan counter overflow"))?,
            ),
            ContainerType::Map,
        );
        if value != &LoroValue::Container(expected) {
            return Err(error("a new list entry must own a new block map"));
        }
    }
    Ok(())
}
