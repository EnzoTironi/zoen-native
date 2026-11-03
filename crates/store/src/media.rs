//! Plaintext media cache; encrypted upload custody remains in sync/media.
use super::{params, OptionalExtension, Result, Store, StoreError};

impl Store {
    // ── media ──

    /// Stores the first payload for a media hash without replacing an existing cache entry.
    ///
    /// # Errors
    /// Returns SQLite errors while executing the operation or decoding its stored rows.
    pub fn put_media(&self, sha256: &str, mime: &str, bytes: &[u8], at_ms: i64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO media (sha256, mime, bytes, at_ms) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(sha256) DO NOTHING",
            params![sha256, mime, bytes, at_ms],
        )?;
        Ok(())
    }

    /// Cache an import whose hash the caller has verified, repairing damaged bytes.
    /// An identical cached payload preserves its original metadata and timestamp.
    ///
    /// # Errors
    /// Returns SQLite errors while executing the operation or decoding its stored rows.
    pub fn cache_imported_media(
        &self,
        sha256: &str,
        mime: &str,
        bytes: &[u8],
        at_ms: i64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO media (sha256, mime, bytes, at_ms) VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(sha256) DO UPDATE SET mime = excluded.mime, bytes = excluded.bytes, at_ms = excluded.at_ms \
             WHERE media.bytes <> excluded.bytes",
            params![sha256, mime, bytes, at_ms],
        )?;
        Ok(())
    }

    /// Whether these bytes are on this device (without reading them).
    ///
    /// # Errors
    /// Returns SQLite errors while executing the operation or decoding its stored rows.
    pub fn has_media(&self, sha256: &str) -> Result<bool> {
        Ok(self
            .conn
            .query_row(
                "SELECT 1 FROM media WHERE sha256 = ?1",
                [sha256],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    /// Reads the cached MIME type and payload for a hash.
    ///
    /// # Errors
    /// Returns SQLite errors while executing the operation or decoding its stored rows.
    pub fn media(&self, sha256: &str) -> Result<Option<(String, Vec<u8>)>> {
        Ok(self
            .conn
            .query_row(
                "SELECT mime, bytes FROM media WHERE sha256 = ?1",
                [sha256],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
    }

    /// Reads only the cached length, without materializing a payload.
    ///
    /// # Errors
    /// Returns SQLite errors while executing the operation or decoding its stored rows.
    pub fn media_bytes_len(&self, sha256: &str) -> Result<Option<u64>> {
        Ok(self
            .conn
            .query_row(
                "SELECT length(bytes) FROM media WHERE sha256 = ?1",
                [sha256],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// The length check and payload selection use the same SQLite snapshot. An
    /// oversized BLOB is never returned to Rust, including after a cache replacement.
    ///
    /// # Errors
    /// Returns `MediaTooLarge` if the byte limit cannot fit SQLite or the cached payload exceeds it, or SQLite read errors.
    pub fn media_bytes_bounded(&self, sha256: &str, max_bytes: usize) -> Result<Option<Vec<u8>>> {
        let limit = i64::try_from(max_bytes).map_err(|_| StoreError::MediaTooLarge)?;
        let row: Option<(u64, Option<Vec<u8>>)> = self
            .conn
            .query_row(
                "SELECT length(bytes), CASE WHEN length(bytes) <= ?2 THEN bytes ELSE NULL END \
                 FROM media WHERE sha256 = ?1",
                params![sha256, limit],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        match row {
            None => Ok(None),
            Some((len, Some(bytes))) if len <= max_bytes as u64 => Ok(Some(bytes)),
            Some(_) => Err(StoreError::MediaTooLarge),
        }
    }
}
