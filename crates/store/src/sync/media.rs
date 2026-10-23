use super::*;
use crate::StoreError;

impl Store {
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
            "SELECT EXISTS(SELECT 1 FROM blob_uploads WHERE blob_sha = ?1) OR EXISTS(SELECT 1 FROM meta WHERE key = 'native.blob.pending/1/' || ?1)",
            [blob],
            |row| row.get(0),
        )?)
    }

    pub fn uploads_pending(&self) -> Result<u64> {
        Ok(self.conn.query_row(
            "SELECT (SELECT count(*) FROM blob_uploads WHERE failed = 0) + (SELECT count(*) FROM meta WHERE key GLOB 'native.blob.pending/1/*' AND substr(key,23) NOT IN (SELECT blob_sha FROM blob_uploads))",
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
        self.meta_delete(&format!("native.blob.pending/1/{blob_sha}"))?;
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

    /// Canonical ciphertext is retained outside the bounded native SQLite
    /// capsule. This reference is pending, never proof of relay upload.
    pub fn queue_external_media(
        &self,
        plain: &str,
        blob: &str,
        key: &str,
        bytes: u64,
    ) -> Result<()> {
        if [plain, blob, key].iter().any(|s| {
            s.len() != 64
                || !s
                    .bytes()
                    .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
        }) || bytes <= 40
            || bytes > 16 * 1024 * 1024 + 40
        {
            return Err(StoreError::InvalidImage);
        }
        if self
            .media_key(plain)?
            .is_some_and(|v| v != (blob.into(), key.into()))
        {
            return Err(StoreError::InvalidImage);
        }
        let name = format!("native.blob.pending/1/{blob}");
        let value = serde_json::to_string(&(plain, bytes))?;
        if self.meta(&name)?.is_some_and(|old| old != value) {
            return Err(StoreError::InvalidImage);
        }
        let tx = self
            .conn
            .is_autocommit()
            .then(|| self.conn.unchecked_transaction())
            .transpose()?;
        self.learn_media_key(plain, blob, key)?;
        self.set_meta(&name, &value)?;
        if let Some(tx) = tx {
            tx.commit()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn external_references_preserve_pending_gate_and_exact_key_binding() {
        let s = Store::in_memory().unwrap();
        let (plain, blob, key) = ("a".repeat(64), "b".repeat(64), "c".repeat(64));
        s.queue_external_media(&plain, &blob, &key, 105).unwrap();
        s.queue_external_media(&plain, &blob, &key, 105).unwrap();
        assert!(s.upload_pending(&blob).unwrap());
        assert_eq!(s.uploads_pending().unwrap(), 1);
        assert!(s.pending_uploads(10).unwrap().is_empty());
        assert!(s.queue_external_media(&plain, &blob, &key, 106).is_err());
        assert!(s
            .queue_external_media(&plain, &"d".repeat(64), &key, 105)
            .is_err());
        let cold = Store::from_memory_image(&s.memory_image().unwrap()).unwrap();
        assert!(cold.upload_pending(&blob).unwrap());
        cold.upload_done(&blob).unwrap();
        assert!(!cold.upload_pending(&blob).unwrap());
        assert_eq!(cold.uploads_pending().unwrap(), 0);
        s.wipe_sync().unwrap();
        assert_eq!(s.uploads_pending().unwrap(), 0);
    }
}
