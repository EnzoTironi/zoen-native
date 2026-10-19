//! Native current/removal visibility and complete Sync-all cursor selection.
use protocol::Cursor;
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncRead {
    pub space: String,
    pub from: u64,
    /// A removed member learns of the removal itself, never events after it.
    pub through: Option<u64>,
}

/// Decide one requested cursor using facts for that exact Space and identity.
pub fn cursor_read(cursor: Cursor, member: bool, removed_at: Option<u64>) -> Option<SyncRead> {
    let through = if member {
        None
    } else {
        Some(removed_at.filter(|last| cursor.next_seq <= *last)?)
    };
    Some(SyncRead {
        space: cursor.space,
        from: cursor.next_seq,
        through,
    })
}

/// `all` includes current Spaces absent from the caller's cursors, from genesis.
/// Preserve native cursor order and repeats; they are not silently coalesced.
pub fn include_current(plan: &mut Vec<SyncRead>, current: &[String], all: bool) {
    if all {
        let mut included: BTreeSet<_> = plan.iter().map(|read| read.space.clone()).collect();
        for space in current {
            if included.insert(space.clone()) {
                plan.push(SyncRead {
                    space: space.clone(),
                    from: 0,
                    through: None,
                });
            }
        }
    }
}
