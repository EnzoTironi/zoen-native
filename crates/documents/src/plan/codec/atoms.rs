//! Canonical operation atoms and immutable provenance for fingerprinting and replay.

use loro::{ContainerID, JsonSchema};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use super::super::{error, PlanError, Result, MIGRATION_PEER};
use super::operations::{atom_contents, valid_id, validate_op};
use super::{MAX_PLAN_ATOMS, MAX_PLAN_CHANGES, MAX_PLAN_RAW_OPS};

const MAX_DEPS: usize = 4_096;

pub(super) type OpId = (u64, i32);
pub(super) type Atoms = BTreeMap<OpId, Atom>;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Atom {
    pub(super) container: ContainerID,
    pub(super) content: Value,
    pub(super) deps: Vec<OpId>,
    pub(super) lamport: u32,
    // Commit metadata can coalesce on ordinary peers. It is neither content nor authority.
    // The reserved deterministic tape has fixed metadata, which is compared as well.
    pub(super) reserved_provenance: Option<(i64, Option<Arc<str>>)>,
}

pub(super) fn conflict(peer: u64) -> PlanError {
    if peer == MIGRATION_PEER {
        PlanError::ReservedSeed
    } else {
        PlanError::OperationConflict
    }
}

pub(super) fn atoms(schema: &JsonSchema) -> Result<Atoms> {
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
