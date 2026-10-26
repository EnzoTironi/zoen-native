//! Authenticate overlapping operation IDs, Lamport dependencies and text mark pairs.

use loro::{IdSpan, JsonOpContent, JsonSchema, JsonTextOp, ID};
use std::collections::BTreeMap;

use super::super::{error, CanonicalPlanSeed, PlanError, PlanUpdate, Result, MIGRATION_PEER};
use super::atoms::{atoms, conflict, Atom, Atoms};
use super::serialized;
use crate::Page;

/// All original overlaps are compared before Loro can discard known operation IDs.
pub(in crate::plan) fn validate(
    page: &Page,
    update: &PlanUpdate,
    seed: &CanonicalPlanSeed,
) -> Result<()> {
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
