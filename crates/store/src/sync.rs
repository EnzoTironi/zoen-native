//! Sync bookkeeping: the outbox (signed events waiting for the relay) and which Spaces
//! are relay-ordered. Kept apart from the core schema so it migrates on its own.

use rusqlite::{params, OptionalExtension};
use types::Event;

use crate::{Result, Store};

mod outbox;

/// An event waiting for the relay.
#[derive(Debug, Clone)]
pub struct Pending {
    pub event: Event,
    pub attempts: u32,
    pub last_error: Option<String>,
    /// The relay refused it for good.
    pub failed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peer_watermarks_merge_exact_unsigned_maxima_without_duplicate_updates() {
        let store = Store::in_memory().unwrap();
        assert_eq!(
            store.raise_peer_receipt("s", "p", u64::MAX, 1).unwrap(),
            (u64::MAX, 1)
        );
        assert_eq!(
            store.raise_peer_receipt("s", "p", 2, u64::MAX - 1).unwrap(),
            (u64::MAX, u64::MAX - 1)
        );
        store.conn().execute_batch("CREATE TEMP TRIGGER reject_receipt_update BEFORE UPDATE ON peer_receipts BEGIN SELECT RAISE(ABORT, 'no redundant updates'); END;").unwrap();
        assert_eq!(
            store.raise_peer_receipt("s", "p", 1, 1).unwrap(),
            (u64::MAX, u64::MAX - 1)
        );
        store
            .conn()
            .execute_batch("DROP TRIGGER reject_receipt_update;")
            .unwrap();
        assert_eq!(
            store.raise_peer_receipt("s", "p", 0, u64::MAX).unwrap(),
            (u64::MAX, u64::MAX)
        );
        assert_eq!(
            store.raise_peer_receipt("s", "other", 0, 7).unwrap(),
            (7, 7)
        );
        assert_eq!(
            store.raise_peer_receipt("other", "p", 8, 0).unwrap(),
            (8, 0)
        );
        let rows = store.peer_receipts().unwrap();
        assert_eq!(rows.len(), 3);
        assert!(rows.contains(&PeerReceipt {
            space: "s".into(),
            identity: "p".into(),
            delivered: u64::MAX,
            read: u64::MAX
        }));
        store.wipe_sync().unwrap();
        assert!(store.peer_receipts().unwrap().is_empty());
    }

    #[test]
    fn peer_receipt_schema_rejects_lossy_or_noncanonical_sequence_encodings() {
        let store = Store::in_memory().unwrap();
        for value in [
            "1",
            "-0000000000000000001",
            "18446744073709551616",
            "0000000000000000000x",
        ] {
            assert!(store.conn().execute(
                "INSERT INTO peer_receipts VALUES ('space', 'peer', ?1, '00000000000000000000')", [value]
            ).is_err(), "{value}");
        }
        assert!(store.conn().execute(
            "INSERT INTO peer_receipts VALUES ('space', 'peer', '00000000000000000001', '00000000000000000002')", []
        ).is_err());
        assert!(store.peer_receipts().unwrap().is_empty());
        store.raise_peer_receipt("space", "peer", 2, 1).unwrap();
        store.wipe().unwrap();
        assert!(store.peer_receipts().unwrap().is_empty());
    }

    #[test]
    fn recovery_upload_requires_success_and_rolls_back_with_its_commit() {
        let store = Store::in_memory().unwrap();
        {
            let _tx = store.conn().unchecked_transaction().unwrap();
            store
                .queue_recovery_upload("rolled-back", b"ciphertext", 1)
                .unwrap();
        }
        assert_eq!(store.uploads_pending().unwrap(), 0);
        assert!(!store.recovery_upload_confirmed("rolled-back").unwrap());
        let tx = store.conn().unchecked_transaction().unwrap();
        store
            .queue_recovery_upload("context", b"ciphertext", 2)
            .unwrap();
        tx.commit().unwrap();
        assert!(!store.recovery_upload_confirmed("context").unwrap());
        store.upload_refused("context").unwrap();
        assert_eq!(
            store.pending_uploads(1).unwrap(),
            vec![("context".into(), b"ciphertext".to_vec())]
        );
        assert!(!store.recovery_upload_confirmed("context").unwrap());
        store.upload_done("context").unwrap();
        assert_eq!(store.uploads_pending().unwrap(), 0);
        assert!(store.recovery_upload_confirmed("context").unwrap());
        store.upload_done("context").unwrap();
        assert!(store.recovery_upload_confirmed("context").unwrap());
        store.clear_recovery_upload("context").unwrap();
        assert!(!store.recovery_upload_confirmed("context").unwrap());
    }

    #[test]
    fn terminal_upload_migration_preserves_ciphertext_and_transaction_rollback() {
        let store = Store::in_memory().unwrap();
        // The pre-patch queue is additive: old rows remain retryable until an
        // actual transport result, never guessed from missing historical facts.
        store.conn().execute_batch("DROP TABLE blob_uploads; CREATE TABLE blob_uploads(blob_sha TEXT PRIMARY KEY, bytes BLOB NOT NULL, created_ms INTEGER NOT NULL); INSERT INTO blob_uploads VALUES ('blob', X'010203', 1);").unwrap();
        store.migrate_sync().unwrap();
        store.migrate_sync().unwrap();
        assert_eq!(
            store.pending_uploads(8).unwrap(),
            vec![("blob".into(), vec![1, 2, 3])]
        );
        {
            let _transaction = store.conn().unchecked_transaction().unwrap();
            assert!(store.upload_refused("blob").unwrap());
            assert!(store.pending_uploads(8).unwrap().is_empty());
        }
        assert_eq!(
            store.pending_uploads(8).unwrap(),
            vec![("blob".into(), vec![1, 2, 3])]
        );
        assert!(store.upload_refused("blob").unwrap());
        assert!(!store.upload_refused("blob").unwrap());
        assert!(
            store.upload_pending("blob").unwrap(),
            "failed is not uploaded"
        );
        assert_eq!(store.uploads_pending().unwrap(), 0);
        let retained: Vec<u8> = store
            .conn()
            .query_row(
                "SELECT bytes FROM blob_uploads WHERE blob_sha = 'blob' AND failed = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(retained, vec![1, 2, 3]);
    }
}

/// An outbox entry without its event.
#[derive(Debug, Clone)]
pub struct OutboxHead {
    pub client_id: String,
    pub space: String,
    pub last_error: Option<String>,
    pub failed: bool,
}

/// Device-local knowledge received from a peer, separate from the signed history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerReceipt {
    pub space: String,
    pub identity: String,
    pub delivered: u64,
    pub read: u64,
}

fn receipt_seq(row: &rusqlite::Row<'_>, column: usize) -> rusqlite::Result<u64> {
    let value: String = row.get(column)?;
    value.parse().map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(column, rusqlite::types::Type::Text, Box::new(e))
    })
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
            -- Fixed-width decimal TEXT preserves the complete protocol u64 range and
            -- makes BINARY lexical MAX an exact unsigned numeric maximum.
            CREATE TABLE IF NOT EXISTS peer_receipts (
                space TEXT NOT NULL,
                identity TEXT NOT NULL,
                delivered TEXT NOT NULL COLLATE BINARY
                    CHECK(length(delivered) = 20 AND delivered NOT GLOB '*[^0-9]*'
                        AND delivered <= '18446744073709551615'),
                read TEXT NOT NULL COLLATE BINARY
                    CHECK(length(read) = 20 AND read NOT GLOB '*[^0-9]*'
                        AND read <= '18446744073709551615' AND read <= delivered),
                PRIMARY KEY (space, identity)
            ) WITHOUT ROWID;
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
                created_ms INTEGER NOT NULL,
                failed     INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS mls_recovery_uploads (
                blob_sha TEXT PRIMARY KEY,
                confirmed INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS mls_recovery_blobs (
                space TEXT PRIMARY KEY,
                blob_sha TEXT NOT NULL,
                bytes BLOB NOT NULL
            );
            ",
        )?;
        let has_failed: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('blob_uploads') WHERE name = 'failed')",
            [],
            |row| row.get(0),
        )?;
        if !has_failed {
            self.conn.execute_batch(
                "ALTER TABLE blob_uploads ADD COLUMN failed INTEGER NOT NULL DEFAULT 0",
            )?;
        }
        self.upgrade_outbox_order()
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

    /// Raise each peer watermark independently, atomically. A read also proves delivery.
    /// Callers validate room membership before admitting untrusted relay frames here.
    pub fn raise_peer_receipt(
        &self,
        space: &str,
        identity: &str,
        delivered: u64,
        read: u64,
    ) -> Result<(u64, u64)> {
        let delivered = format!("{:020}", delivered.max(read));
        let read = format!("{read:020}");
        self.conn.execute(
            "INSERT INTO peer_receipts (space, identity, delivered, read) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(space, identity) DO UPDATE SET
                delivered = MAX(peer_receipts.delivered, excluded.delivered),
                read = MAX(peer_receipts.read, excluded.read)
             WHERE excluded.delivered > peer_receipts.delivered OR excluded.read > peer_receipts.read",
            params![space, identity, delivered, read],
        )?;
        Ok(self.conn.query_row(
            "SELECT delivered, read FROM peer_receipts WHERE space = ?1 AND identity = ?2",
            params![space, identity],
            |r| Ok((receipt_seq(r, 0)?, receipt_seq(r, 1)?)),
        )?)
    }

    pub fn peer_receipts(&self) -> Result<Vec<PeerReceipt>> {
        let mut statement = self
            .conn
            .prepare("SELECT space, identity, delivered, read FROM peer_receipts")?;
        let rows = statement.query_map([], |r| {
            Ok(PeerReceipt {
                space: r.get(0)?,
                identity: r.get(1)?,
                delivered: receipt_seq(r, 2)?,
                read: receipt_seq(r, 3)?,
            })
        })?;
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
        self.conn.execute_batch("DELETE FROM outbox; DELETE FROM synced_spaces; DELETE FROM peer_receipts; DELETE FROM media_keys; DELETE FROM blob_uploads; DELETE FROM mls_recovery_uploads; DELETE FROM mls_recovery_blobs;")?;
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
            .prepare("SELECT blob_sha, bytes FROM blob_uploads WHERE failed = 0 ORDER BY created_ms LIMIT ?1")?;
        let rows = st.query_map([limit], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn upload_pending(&self, blob: &str) -> Result<bool> {
        Ok(self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM blob_uploads WHERE blob_sha = ?1)",
            [blob],
            |row| row.get(0),
        )?)
    }

    pub fn uploads_pending(&self) -> Result<u64> {
        Ok(self.conn.query_row(
            "SELECT count(*) FROM blob_uploads WHERE failed = 0",
            [],
            |r| r.get::<_, i64>(0),
        )? as u64)
    }

    pub fn upload_done(&self, blob_sha: &str) -> Result<()> {
        let tx = self
            .conn
            .is_autocommit()
            .then(|| self.conn.unchecked_transaction())
            .transpose()?;
        self.conn.execute(
            "UPDATE mls_recovery_uploads SET confirmed = 1 WHERE blob_sha = ?1",
            [blob_sha],
        )?;
        self.conn
            .execute("DELETE FROM blob_uploads WHERE blob_sha = ?1", [blob_sha])?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }

    /// Retain exact ciphertext, but stop automatic retries after a permanent refusal.
    /// Recovery keeps its existing retry/confirmation contract.
    pub fn upload_refused(&self, blob_sha: &str) -> Result<bool> {
        let changed = self.conn.execute(
            "UPDATE blob_uploads SET failed = 1 WHERE blob_sha = ?1 AND failed = 0 AND NOT EXISTS
             (SELECT 1 FROM mls_recovery_uploads WHERE blob_sha = ?1)",
            [blob_sha],
        )?;
        Ok(changed != 0)
    }

    /// Called inside the transaction that stages MLS state and its signed commit.
    pub fn queue_recovery_upload(&self, blob_sha: &str, bytes: &[u8], at_ms: i64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO blob_uploads (blob_sha, bytes, created_ms) VALUES (?1, ?2, ?3)",
            params![blob_sha, bytes, at_ms],
        )?;
        self.conn.execute(
            "INSERT INTO mls_recovery_uploads (blob_sha) VALUES (?1)",
            [blob_sha],
        )?;
        Ok(())
    }

    /// Missing receipts fail closed, including after an interrupted migration.
    pub fn recovery_upload_confirmed(&self, blob_sha: &str) -> Result<bool> {
        Ok(self
            .conn
            .query_row(
                "SELECT confirmed FROM mls_recovery_uploads WHERE blob_sha = ?1",
                [blob_sha],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(false))
    }

    pub fn clear_recovery_upload(&self, blob_sha: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM mls_recovery_uploads WHERE blob_sha = ?1",
            [blob_sha],
        )?;
        self.conn
            .execute("DELETE FROM blob_uploads WHERE blob_sha = ?1", [blob_sha])?;
        Ok(())
    }

    pub fn recovery_blob(&self, space: &str, blob_sha: &str) -> Result<Option<Vec<u8>>> {
        Ok(self
            .conn
            .query_row(
                "SELECT bytes FROM mls_recovery_blobs WHERE space = ?1 AND blob_sha = ?2",
                params![space, blob_sha],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// At most the latest downloaded context per Space is retained on this device.
    pub fn cache_recovery_blob(&self, space: &str, blob_sha: &str, bytes: &[u8]) -> Result<()> {
        self.conn.execute(
            "INSERT INTO mls_recovery_blobs (space, blob_sha, bytes) VALUES (?1, ?2, ?3)
             ON CONFLICT(space) DO UPDATE SET blob_sha = excluded.blob_sha, bytes = excluded.bytes",
            params![space, blob_sha, bytes],
        )?;
        Ok(())
    }

    pub fn invalidate_recovery_blob(&self, space: &str, blob_sha: Option<&str>) -> Result<()> {
        self.conn.execute(
            "DELETE FROM mls_recovery_blobs WHERE space = ?1 AND (?2 IS NULL OR blob_sha != ?2)",
            params![space, blob_sha],
        )?;
        Ok(())
    }
}
