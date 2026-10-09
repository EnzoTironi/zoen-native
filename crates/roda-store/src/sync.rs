//! Sync bookkeeping: the outbox (signed events waiting for the relay) and which Spaces
//! are relay-ordered. Kept apart from the core schema so it migrates on its own.

use roda_types::Event;
use rusqlite::{params, OptionalExtension};

use crate::{Result, Store};

/// An event waiting for the relay.
#[derive(Debug, Clone)]
pub struct Pending {
    pub event: Event,
    pub attempts: u32,
    pub last_error: Option<String>,
    /// The relay refused it for good.
    pub failed: bool,
}

/// An outbox entry without its event.
#[derive(Debug, Clone)]
pub struct OutboxHead {
    pub client_id: String,
    pub space: String,
    pub last_error: Option<String>,
    pub failed: bool,
}

impl Store {
    pub(crate) fn migrate_sync(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS outbox (
                client_id  TEXT PRIMARY KEY,
                space      TEXT NOT NULL,
                json       TEXT NOT NULL,
                created_ms INTEGER NOT NULL,
                attempts   INTEGER NOT NULL DEFAULT 0,
                last_error TEXT,
                failed     INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS outbox_by_space ON outbox (space, created_ms);
            CREATE TABLE IF NOT EXISTS synced_spaces (
                space TEXT PRIMARY KEY
            );
            -- The key and relay address of each attachment's encrypted copy.
            CREATE TABLE IF NOT EXISTS media_keys (
                plain_sha TEXT PRIMARY KEY,
                blob_sha  TEXT NOT NULL,
                key       TEXT NOT NULL
            );
            -- Encrypted copies waiting to reach the relay's blob store.
            CREATE TABLE IF NOT EXISTS blob_uploads (
                blob_sha   TEXT PRIMARY KEY,
                bytes      BLOB NOT NULL,
                created_ms INTEGER NOT NULL
            );
            ",
        )?;
        Ok(())
    }

    /// Queues a signed, unsequenced event. Same `client_id` twice is a no-op.
    pub fn outbox_put(&self, e: &Event) -> Result<()> {
        self.conn.execute(
            "INSERT INTO outbox (client_id, space, json, created_ms) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(client_id) DO NOTHING",
            params![e.client_id, e.space, serde_json::to_string(e)?, e.at_ms],
        )?;
        Ok(())
    }

    /// Everything still waiting (failed ones included), oldest first.
    pub fn outbox(&self) -> Result<Vec<Pending>> {
        let mut st = self.conn.prepare(
            "SELECT json, attempts, last_error, failed FROM outbox ORDER BY created_ms, rowid",
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

    /// What is waiting, without the events themselves (oldest first): cheap enough to call
    /// after every write, then `outbox_get` only the entries that need work.
    pub fn outbox_heads(&self) -> Result<Vec<OutboxHead>> {
        let mut st = self.conn.prepare(
            "SELECT client_id, space, last_error, failed FROM outbox ORDER BY created_ms, rowid",
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

    /// Queued MLS handshakes (Commit, Welcome) not failed, oldest first: (client id,
    /// space, kind).
    pub fn outbox_handshakes(&self) -> Result<Vec<(String, String, String)>> {
        let mut st = self.conn.prepare(
            "SELECT client_id, space, json_extract(json, '$.body.Sealed.kind') AS kind FROM outbox
             WHERE NOT failed AND kind IN ('Commit', 'Welcome') ORDER BY created_ms, rowid",
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

    pub fn set_synced(&self, space: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO synced_spaces (space) VALUES (?1) ON CONFLICT DO NOTHING",
            [space],
        )?;
        Ok(())
    }

    pub fn synced_spaces(&self) -> Result<Vec<String>> {
        let mut st = self.conn.prepare("SELECT space FROM synced_spaces")?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Next sequence number this device is missing for each relay-ordered Space.
    pub fn cursors(&self) -> Result<Vec<(String, u64)>> {
        let mut st = self.conn.prepare(
            "SELECT s.space, COALESCE((SELECT MAX(seq) + 1 FROM events e WHERE e.space = s.space), 0) FROM synced_spaces s",
        )?;
        let rows = st.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u64))
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn meta_delete(&self, key: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM meta WHERE key = ?1", [key])?;
        Ok(())
    }

    pub fn wipe_sync(&self) -> Result<()> {
        self.conn.execute_batch("DELETE FROM outbox; DELETE FROM synced_spaces; DELETE FROM media_keys; DELETE FROM blob_uploads;")?;
        Ok(())
    }

    // ── encrypted media ──

    pub fn media_key(&self, plain_sha: &str) -> Result<Option<(String, String)>> {
        Ok(self
            .conn
            .query_row(
                "SELECT blob_sha, key FROM media_keys WHERE plain_sha = ?1",
                [plain_sha],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
    }

    /// Remembers the key and queues the encrypted copy for upload, in one go.
    pub fn seal_media(
        &self,
        plain_sha: &str,
        blob_sha: &str,
        key: &str,
        ciphertext: &[u8],
        at_ms: i64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO media_keys (plain_sha, blob_sha, key) VALUES (?1, ?2, ?3) ON CONFLICT(plain_sha) DO NOTHING",
            params![plain_sha, blob_sha, key],
        )?;
        self.conn.execute(
            "INSERT INTO blob_uploads (blob_sha, bytes, created_ms) VALUES (?1, ?2, ?3) ON CONFLICT(blob_sha) DO NOTHING",
            params![blob_sha, ciphertext, at_ms],
        )?;
        Ok(())
    }

    /// A key learned from someone else's event (so re-sharing that photo reuses the copy).
    pub fn learn_media_key(&self, plain_sha: &str, blob_sha: &str, key: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO media_keys (plain_sha, blob_sha, key) VALUES (?1, ?2, ?3) ON CONFLICT(plain_sha) DO NOTHING",
            params![plain_sha, blob_sha, key],
        )?;
        Ok(())
    }

    pub fn pending_uploads(&self, limit: u32) -> Result<Vec<(String, Vec<u8>)>> {
        let mut st = self
            .conn
            .prepare("SELECT blob_sha, bytes FROM blob_uploads ORDER BY created_ms LIMIT ?1")?;
        let rows = st.query_map([limit], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn uploads_pending(&self) -> Result<u64> {
        Ok(self
            .conn
            .query_row("SELECT count(*) FROM blob_uploads", [], |r| {
                r.get::<_, i64>(0)
            })? as u64)
    }

    pub fn upload_done(&self, blob_sha: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM blob_uploads WHERE blob_sha = ?1", [blob_sha])?;
        Ok(())
    }
}
