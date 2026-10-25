//! One exact, non-destructive native image compatibility step.
use super::{schema, Result, Schema, Store};

// SQLite's retained SQL for the pre-FIFO outbox. Do not accept merely similar
// columns: additional triggers, constraints or schema objects must still refuse.
pub(super) const PRE_FIFO_OUTBOX: &str = r"CREATE TABLE outbox (
                client_id  TEXT PRIMARY KEY,
                space      TEXT NOT NULL,
                json       TEXT NOT NULL,
                created_ms INTEGER NOT NULL,
                attempts   INTEGER NOT NULL DEFAULT 0,
                last_error TEXT,
                failed     INTEGER NOT NULL DEFAULT 0
            )";

pub(super) fn supported(store: &Store, trusted: &Store) -> Result<bool> {
    let actual = schema(store)?;
    let current = schema(trusted)?;
    if actual == current {
        return Ok(true);
    }
    let legacy = current
        .into_iter()
        .filter_map(|mut entry| {
            if entry.0 == "index" && entry.1 == "outbox_queue_order" {
                return None;
            }
            if entry.0 == "table" && entry.1 == "outbox" {
                entry.3 = Some(PRE_FIFO_OUTBOX.into());
            } else if entry.0 == "index" && entry.1 == "outbox_by_space" {
                entry.3 = Some("CREATE INDEX outbox_by_space ON outbox (space, created_ms)".into());
            }
            Some(entry)
        })
        .collect::<Schema>();
    Ok(actual == legacy)
}
