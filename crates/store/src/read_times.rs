//! Device-local authenticated receipt evidence, never part of signed history or backup.
use crate::{params, OptionalExtension, Result, Store};

/// One peer-reported newly-read interval and this device's independent observation clock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadTimeRow {
    /// Exclusive previous read prefix.
    pub after_seq: u64,
    /// Inclusive reported read prefix.
    pub through_seq: u64,
    /// Peer clock; not verified UTC or local arrival time.
    pub reported_at_ms: i64,
    /// Local device clock at first admitted arrival.
    pub observed_at_ms: i64,
}
impl Store {
    pub(crate) fn migrate_read_times(&self) -> Result<()> {
        self.conn.execute_batch("CREATE TABLE IF NOT EXISTS peer_read_times (
            space TEXT NOT NULL, identity TEXT NOT NULL,
            after_seq TEXT COLLATE BINARY NOT NULL CHECK(length(after_seq)=20 AND after_seq NOT GLOB '*[^0-9]*'),
            through_seq TEXT COLLATE BINARY NOT NULL CHECK(length(through_seq)=20 AND through_seq NOT GLOB '*[^0-9]*' AND through_seq>after_seq AND through_seq<='18446744073709551615'),
            reported_at_ms INTEGER NOT NULL CHECK(reported_at_ms>0 AND reported_at_ms<=253402300799999), observed_at_ms INTEGER NOT NULL,
            PRIMARY KEY(space,identity,after_seq,through_seq));
            CREATE INDEX IF NOT EXISTS read_time_age ON peer_read_times(observed_at_ms);")?;
        Ok(())
    }
    /// First observation wins for an exact interval; duplicates/replay cannot invent a newer time.
    pub fn record_read_time(
        &self,
        space: &str,
        identity: &str,
        row: &ReadTimeRow,
        cutoff: i64,
    ) -> Result<bool> {
        self.migrate_read_times()?;
        let changed = self.conn.execute("INSERT OR IGNORE INTO peer_read_times
            (space,identity,after_seq,through_seq,reported_at_ms,observed_at_ms) VALUES (?1,?2,?3,?4,?5,?6)",
            params![space,identity,format!("{:020}",row.after_seq),format!("{:020}",row.through_seq),row.reported_at_ms,row.observed_at_ms])? > 0;
        self.conn.execute(
            "DELETE FROM peer_read_times WHERE observed_at_ms<?1",
            [cutoff],
        )?;
        self.conn.execute("DELETE FROM peer_read_times WHERE rowid IN
            (SELECT rowid FROM peer_read_times WHERE space=?1 AND identity=?2 ORDER BY observed_at_ms DESC,rowid DESC LIMIT -1 OFFSET 256)",params![space,identity])?;
        self.conn.execute("DELETE FROM peer_read_times WHERE rowid IN
            (SELECT rowid FROM peer_read_times ORDER BY observed_at_ms DESC,rowid DESC LIMIT -1 OFFSET 65536)",[])?;
        Ok(changed)
    }
    /// Bounded, ordered evidence for replay or exact message projection.
    pub fn read_times(&self, space: &str, identity: &str, cutoff: i64) -> Result<Vec<ReadTimeRow>> {
        let kind: Option<String> = self
            .conn
            .query_row(
                "SELECT type FROM sqlite_master WHERE name='peer_read_times'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        match kind.as_deref() {
            None => return Ok(Vec::new()),
            Some("table") => {}
            _ => return Err(rusqlite::Error::InvalidQuery.into()),
        }
        let mut statement = self.conn.prepare("SELECT after_seq,through_seq,reported_at_ms,observed_at_ms FROM peer_read_times
            WHERE space=?1 AND identity=?2 AND observed_at_ms>=?3 ORDER BY through_seq,after_seq LIMIT 256")?;
        let rows = statement.query_map(params![space, identity, cutoff], |r| {
            let decode = |index| -> rusqlite::Result<u64> {
                let value: String = r.get(index)?;
                value.parse().map_err(|e| {
                    rusqlite::Error::FromSqlConversionFailure(
                        index,
                        rusqlite::types::Type::Text,
                        Box::new(e),
                    )
                })
            };
            Ok(ReadTimeRow {
                after_seq: decode(0)?,
                through_seq: decode(1)?,
                reported_at_ms: r.get(2)?,
                observed_at_ms: r.get(3)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row(after: u64, through: u64, reported: i64, observed: i64) -> ReadTimeRow {
        ReadTimeRow {
            after_seq: after,
            through_seq: through,
            reported_at_ms: reported,
            observed_at_ms: observed,
        }
    }
    #[test]
    fn intervals_keep_peer_clock_and_first_local_observation_across_duplicate_and_old_image() {
        let store = Store::in_memory().unwrap();
        let original = row(5, u64::MAX, 1234, 9000);
        assert!(store.record_read_time("s", "peer", &original, 0).unwrap());
        assert!(!store
            .record_read_time("s", "peer", &row(5, u64::MAX, 9999, 9999), 0)
            .unwrap());
        let reopened = Store::from_memory_image(&store.memory_image().unwrap()).unwrap();
        assert_eq!(reopened.read_times("s", "peer", 0).unwrap(), vec![original]);
        assert!(reopened.read_times("s", "other", 0).unwrap().is_empty());
        assert!(reopened.read_times("s", "peer", 9001).unwrap().is_empty());
        reopened
            .conn()
            .execute_batch("DROP TABLE peer_read_times")
            .unwrap();
        let old = Store::from_memory_image(&reopened.memory_image().unwrap()).unwrap();
        assert!(old.read_times("s", "peer", 0).unwrap().is_empty());
        old.record_read_time("s", "peer", &row(8, 9, 1000, 1000), 0)
            .unwrap();
        assert_eq!(old.read_times("s", "peer", 0).unwrap().len(), 1);
        old.wipe_sync().unwrap();
        assert!(old.read_times("s", "peer", 0).unwrap().is_empty());
    }
    #[test]
    fn timestamp_retention_is_bounded_without_lowering_legacy_watermark() {
        let store = Store::in_memory().unwrap();
        store.raise_peer_receipt("s", "peer", 400, 400).unwrap();
        for n in 1..=300 {
            store
                .record_read_time("s", "peer", &row(n - 1, n, 1000, n as i64), 0)
                .unwrap();
        }
        let rows = store.read_times("s", "peer", 0).unwrap();
        assert_eq!(rows.len(), 256);
        assert_eq!(rows[0].after_seq, 44);
        assert_eq!(store.peer_receipts().unwrap()[0].read, 400);
    }
}
