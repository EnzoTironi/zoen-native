//! Persisted enqueue order is independent of the signed wall-clock timestamp.
use super::*;

#[cfg(test)]
mod tests;

impl Store {
    /// Upgrade the device-local outbox after a caller has validated its schema.
    /// Disk opens call this automatically; authenticated native images call it
    /// only after matching the exact supported previous schema.
    pub fn upgrade_outbox_order(&self) -> Result<()> {
        let ordered: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('outbox') WHERE name = 'queue_order')",
            [],
            |row| row.get(0),
        )?;
        if ordered {
            return Ok(());
        }
        self.conn.execute_batch("SAVEPOINT outbox_order")?;
        let result = self.conn.execute_batch(
            "ALTER TABLE outbox ADD COLUMN queue_order INTEGER NOT NULL DEFAULT 0
                CHECK(typeof(queue_order) = 'integer');
             UPDATE outbox SET queue_order = rowid;
             CREATE UNIQUE INDEX outbox_queue_order ON outbox(queue_order);
             DROP INDEX outbox_by_space;
             CREATE INDEX outbox_by_space ON outbox(space, queue_order);
             RELEASE outbox_order;",
        );
        if result.is_err() {
            self.conn
                .execute_batch("ROLLBACK TO outbox_order; RELEASE outbox_order")?;
        }
        result?;
        Ok(())
    }

    /// Queues a signed, unsequenced event. Same `client_id` twice is a no-op.
    pub fn outbox_put(&self, e: &Event) -> Result<()> {
        self.conn.execute(
            "INSERT INTO outbox (client_id, space, json, created_ms, queue_order)
             SELECT ?1, ?2, ?3, ?4, coalesce((SELECT max(queue_order) FROM outbox), 0) + 1
             WHERE NOT EXISTS(SELECT 1 FROM outbox WHERE client_id = ?1)",
            params![e.client_id, e.space, serde_json::to_string(e)?, e.at_ms],
        )?;
        Ok(())
    }

    /// Everything still waiting (failed ones included), in enqueue order.
    pub fn outbox(&self) -> Result<Vec<Pending>> {
        let mut st = self.conn.prepare(
            "SELECT json, attempts, last_error, failed FROM outbox ORDER BY queue_order",
        )?;
        let rows = st.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (json, attempts, last_error, failed) = row?;
            out.push(Pending {
                event: serde_json::from_str(&json)?,
                attempts: attempts as u32,
                last_error,
                failed: failed != 0,
            });
        }
        Ok(out)
    }

    /// What is waiting, without the events themselves (in enqueue order): cheap enough to call
    /// after every write, then `outbox_get` only the entries that need work.
    pub fn outbox_heads(&self) -> Result<Vec<OutboxHead>> {
        let mut st = self.conn.prepare(
            "SELECT client_id, space, last_error, failed FROM outbox ORDER BY queue_order",
        )?;
        let rows = st.query_map([], |r| {
            Ok(OutboxHead {
                client_id: r.get(0)?,
                space: r.get(1)?,
                last_error: r.get(2)?,
                failed: r.get::<_, i64>(3)? != 0,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Queued MLS handshakes (Commit, Welcome) not failed, in enqueue order: (client id,
    /// space, kind).
    pub fn outbox_handshakes(&self) -> Result<Vec<(String, String, String)>> {
        let mut st = self.conn.prepare(
            "SELECT client_id, space, json_extract(json, '$.body.Sealed.kind') AS kind FROM outbox
             WHERE NOT failed AND kind IN ('Commit', 'Welcome') ORDER BY queue_order",
        )?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn outbox_get(&self, client_id: &str) -> Result<Option<Event>> {
        let json: Option<String> = self
            .conn
            .query_row(
                "SELECT json FROM outbox WHERE client_id = ?1",
                [client_id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(match json {
            Some(j) => Some(serde_json::from_str(&j)?),
            None => None,
        })
    }

    pub fn outbox_remove(&self, client_id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM outbox WHERE client_id = ?1", [client_id])?;
        Ok(())
    }

    pub fn outbox_note(&self, client_id: &str, error: &str, failed: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE outbox SET attempts = attempts + 1, last_error = ?2, failed = ?3 WHERE client_id = ?1",
            params![client_id, error, failed as i64],
        )?;
        Ok(())
    }

    /// A refused creation cannot have any confirmed descendants. Fail its
    /// entire queued Space atomically, including transport-only invite metadata.
    pub fn outbox_refuse_space(&self, space: &str, error: &str) -> Result<()> {
        self.conn.execute_batch("SAVEPOINT refuse_space")?;
        let result = (|| -> Result<()> {
            self.conn.execute(
                "UPDATE outbox SET attempts = attempts + 1, last_error = ?2, failed = 1 WHERE space = ?1 AND NOT failed",
                params![space, error],
            )?;
            self.conn.execute(
                "DELETE FROM meta WHERE key IN (SELECT 'invite:' || client_id FROM outbox WHERE space = ?1)",
                [space],
            )?;
            self.conn.execute_batch("RELEASE refuse_space")?;
            Ok(())
        })();
        if result.is_err() {
            let _ = self
                .conn
                .execute_batch("ROLLBACK TO refuse_space; RELEASE refuse_space");
        }
        result
    }

    pub fn outbox_len(&self) -> Result<u64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM outbox WHERE failed = 0", [], |r| {
                r.get::<_, i64>(0)
            })? as u64)
    }
}
