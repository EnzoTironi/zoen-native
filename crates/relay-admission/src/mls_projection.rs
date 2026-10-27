//! Durable MLS epoch and retention transitions shared by native and portable logs.
//! Callers admit the signed envelope first and persist these writes atomically
//! with its canonical receipt. This module never authenticates a device.

use crate::Effect;
use std::collections::BTreeMap;
use types::{Privacy, Role};

pub const ADDED: &str = "*";
pub const DEFAULT_PRUNE_CEILING_MS: i64 = 30 * 24 * 3600 * 1000;
pub const PRUNE_BATCH: usize = 256;
pub type Holds = BTreeMap<(String, String), (u64, i64)>;

#[must_use]
pub fn ceiling_ms(seconds: Option<&str>) -> i64 {
    seconds
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| *value > 0)
        .and_then(|value| value.checked_mul(1000))
        .unwrap_or(DEFAULT_PRUNE_CEILING_MS)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Write {
    Epoch {
        next: u64,
        device: String,
    },
    Hold {
        identity: String,
        device: String,
        seq: u64,
        when_ms: i64,
    },
    ClearHold {
        identity: String,
        device: String,
    },
    ClearIdentity {
        identity: String,
    },
}

/// Projection inputs precede this event. Membership changes are persisted by
/// the caller; their MLS retention changes are part of the same transaction.
/// # Errors
/// Refuses an exhausted MLS epoch rather than wrapping the next epoch.
pub fn transition(
    effect: &Effect,
    privacy: Option<Privacy>,
    author: &str,
    seq: u64,
    now_ms: i64,
    members: &BTreeMap<String, Role>,
    holds: &Holds,
) -> Result<Vec<Write>, &'static str> {
    let hold = |identity: &str, device: &str, at| Write::Hold {
        identity: identity.into(),
        device: device.into(),
        seq: at,
        when_ms: now_ms,
    };
    let initial_epoch = || Write::Epoch {
        next: 0,
        device: String::new(),
    };
    Ok(match effect {
        Effect::Create {
            privacy: Privacy::EndToEnd,
            ..
        } => vec![initial_epoch(), hold(author, ADDED, seq)],
        Effect::Add { identity, .. } if privacy == Some(Privacy::EndToEnd) => {
            vec![hold(identity, ADDED, seq)]
        }
        Effect::Remove { identity } => vec![Write::ClearIdentity {
            identity: identity.clone(),
        }],
        Effect::Encrypt => {
            let mut writes = vec![initial_epoch()];
            writes.extend(members.keys().map(|identity| hold(identity, ADDED, seq)));
            writes
        }
        Effect::Commit { epoch, device } => vec![Write::Epoch {
            next: epoch.checked_add(1).ok_or("MLS epoch exhausted")?,
            device: device.clone(),
        }],
        Effect::Checkpoint { device, upto }
            if privacy == Some(Privacy::EndToEnd) && !device.is_empty() =>
        {
            let mut writes = vec![];
            let previous = holds.get(&(author.into(), device.clone()));
            // Equal checkpoints refresh the live-device ceiling. Older ones
            // never move a device backwards or extend its old hold's lifetime.
            if previous.is_none_or(|(seq, _)| *seq <= *upto) {
                writes.push(hold(author, device, *upto));
            }
            if holds.contains_key(&(author.into(), ADDED.into())) {
                writes.push(Write::ClearHold {
                    identity: author.into(),
                    device: ADDED.into(),
                });
            }
            writes
        }
        Effect::Hold { device } => vec![hold(author, device, seq)],
        _ => vec![],
    })
}

/// Apply the same plan to an in-memory projection after durable writes succeed.
/// Native callers may persist each write while applying it in their transaction.
pub fn apply(mls: &mut Option<(u64, String)>, holds: &mut Holds, writes: &[Write]) {
    for write in writes {
        match write {
            Write::Epoch { next, device } => *mls = Some((*next, device.clone())),
            Write::Hold {
                identity,
                device,
                seq,
                when_ms,
            } => {
                holds.insert((identity.clone(), device.clone()), (*seq, *when_ms));
            }
            Write::ClearHold { identity, device } => {
                holds.remove(&(identity.clone(), device.clone()));
            }
            Write::ClearIdentity { identity } => holds.retain(|(who, _), _| who != identity),
        }
    }
}

/// The native retention rule: every current identity must have a hold record;
/// holds beyond the device ceiling stop preventing pruning. Future timestamps
/// remain live, and integer extremes cannot overflow the elapsed-time check.
#[must_use]
pub fn prune_floor(
    privacy: Option<Privacy>,
    members: &BTreeMap<String, Role>,
    holds: &Holds,
    now_ms: i64,
    ceiling_ms: i64,
) -> Option<u64> {
    if privacy != Some(Privacy::EndToEnd) || members.is_empty() || ceiling_ms <= 0 {
        return None;
    }
    let all_hold = members.keys().all(|identity| {
        holds
            .range((identity.clone(), String::new())..)
            .next()
            .is_some_and(|((who, _), _)| who == identity)
    });
    all_hold
        .then(|| {
            holds
                .values()
                .filter(|(_, when)| i128::from(now_ms) - i128::from(*when) < i128::from(ceiling_ms))
                .map(|(seq, _)| *seq)
                .min()
        })
        .flatten()
}

/// Native pruning scans at most 256 rows, then advances to the first unscanned
/// sequence. Sparse rows are valid; a short complete scan advances to the floor.
/// # Errors
/// Refuses invalid scan bounds, inconsistent row counts/last sequence or an exhausted cursor.
pub fn pruned_cursor(
    from: u64,
    floor: u64,
    count: usize,
    last: Option<u64>,
) -> Result<u64, &'static str> {
    if floor < from || count > PRUNE_BATCH || (count == 0) != last.is_none() {
        return Err("invalid pruning scan");
    }
    if let Some(last) = last {
        if last < from || last >= floor {
            return Err("pruning row outside the admitted range");
        }
        if count == PRUNE_BATCH {
            return last.checked_add(1).ok_or("pruning cursor exhausted");
        }
    }
    Ok(floor)
}

/// Rewrites only ciphertext whose signed digest authenticates a stub. Plain
/// entries and legacy ciphertext retain their original bytes, including unknown
/// protobuf fields, exactly as in the native log.
/// # Errors
/// Refuses invalid event encoding or a stub that changes the signed ciphertext digest.
pub fn pruned_bytes(bytes: &[u8]) -> Result<Option<Vec<u8>>, String> {
    let event = protocol::Sequenced::decode(bytes).map_err(|error| error.to_string())?;
    let Some(stub) = event.env.pruned() else {
        return Ok(None);
    };
    if stub.wire_hash() != event.env.wire_hash() {
        return Err("pruning changed the signed ciphertext digest".into());
    }
    Ok(Some(protocol::Sequenced { env: stub, ..event }.encode()))
}
