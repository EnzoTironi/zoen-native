use super::{params, OptionalExtension, Result, Store};

impl Store {
    /// Called inside the transaction that stages MLS state and its signed commit.
    ///
    /// # Errors
    /// Returns SQLite insertion errors; the caller-owned transaction controls rollback of both rows.
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
    ///
    /// # Errors
    /// Returns SQLite errors while executing the operation or decoding its stored rows.
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

    /// Removes the recovery association and its queued ciphertext upload.
    ///
    /// # Errors
    /// Returns SQLite errors while executing the operation or decoding its stored rows.
    pub fn clear_recovery_upload(&self, blob_sha: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM mls_recovery_uploads WHERE blob_sha = ?1",
            [blob_sha],
        )?;
        self.conn
            .execute("DELETE FROM blob_uploads WHERE blob_sha = ?1", [blob_sha])?;
        Ok(())
    }

    /// Reads a cached recovery context for the requested space and digest.
    ///
    /// # Errors
    /// Returns SQLite errors while executing the operation or decoding its stored rows.
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
    ///
    /// # Errors
    /// Returns SQLite errors while executing the operation or decoding its stored rows.
    pub fn cache_recovery_blob(&self, space: &str, blob_sha: &str, bytes: &[u8]) -> Result<()> {
        self.conn.execute(
            "INSERT INTO mls_recovery_blobs (space, blob_sha, bytes) VALUES (?1, ?2, ?3)
             ON CONFLICT(space) DO UPDATE SET blob_sha = excluded.blob_sha, bytes = excluded.bytes",
            params![space, blob_sha, bytes],
        )?;
        Ok(())
    }

    /// Drops a stale recovery context, or all contexts when no digest is retained.
    ///
    /// # Errors
    /// Returns SQLite errors while executing the operation or decoding its stored rows.
    pub fn invalidate_recovery_blob(&self, space: &str, blob_sha: Option<&str>) -> Result<()> {
        self.conn.execute(
            "DELETE FROM mls_recovery_blobs WHERE space = ?1 AND (?2 IS NULL OR blob_sha != ?2)",
            params![space, blob_sha],
        )?;
        Ok(())
    }
}
