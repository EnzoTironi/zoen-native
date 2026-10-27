//! Disposable full-text indexing and bounded keyset queries.
use super::{params, Result, SearchRow, Store, StoreError};

impl Store {
    // ── search index ──

    /// Clears disposable search rows without changing canonical event history.
    ///
    /// # Errors
    /// Returns SQLite errors while executing the operation or decoding its stored rows.
    pub fn index_clear(&self) -> Result<()> {
        self.conn.execute("DELETE FROM search", [])?;
        Ok(())
    }

    /// Replaces the row for (`kind`, `ref_id`).
    #[allow(clippy::too_many_arguments)]
    ///
    /// # Errors
    /// Returns SQLite errors while executing the operation or decoding its stored rows.
    pub fn index_put(
        &self,
        kind: &str,
        ref_id: &str,
        space: &str,
        author: &str,
        at_ms: i64,
        title: &str,
        body: &str,
    ) -> Result<()> {
        self.conn.execute(
            "DELETE FROM search WHERE kind = ?1 AND ref_id = ?2",
            params![kind, ref_id],
        )?;
        self.conn.execute(
            "INSERT INTO search (kind, ref_id, space, author, at_ms, title, body) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![kind, ref_id, space, author, at_ms, title, body],
        )?;
        Ok(())
    }

    /// Starts the savepoint for one disposable-index rebuild.
    ///
    /// # Errors
    /// Returns SQLite errors while executing the operation or decoding its stored rows.
    pub fn index_begin(&self) -> Result<()> {
        self.conn.execute_batch("SAVEPOINT idx")?;
        Ok(())
    }

    /// Commits the index savepoint and advances its cursor generation.
    ///
    /// # Errors
    /// Returns SQLite errors while executing the operation or decoding its stored rows.
    pub fn index_commit(&self) -> Result<()> {
        self.conn.execute_batch(
            "INSERT INTO meta (key, value) VALUES ('search:generation', '1')
             ON CONFLICT(key) DO UPDATE SET value = CAST(value AS INTEGER) + 1;
             RELEASE idx",
        )?;
        Ok(())
    }

    /// Changes only after a committed rebuild; cursors cannot silently span edits.
    ///
    /// # Errors
    /// Returns SQLite errors while executing the operation or decoding its stored rows.
    pub fn search_generation(&self) -> Result<u64> {
        Ok(self.conn.query_row(
            "SELECT COALESCE((SELECT CAST(value AS INTEGER) FROM meta WHERE key = 'search:generation'), 0)",
            [],
            |row| row.get(0),
        )?)
    }

    /// Undo a failed rebuild. Never leave the savepoint open: every later write would sit
    /// in an uncommitted transaction and vanish when the app is killed.
    pub fn index_abort(&self) {
        let _ = self.conn.execute_batch("ROLLBACK TO idx; RELEASE idx");
    }

    /// Prefix search over the index. `query` is the user's text; every word must match
    /// (as a prefix, diacritics folded). Best matches first, then newest.
    ///
    /// # Errors
    /// Returns SQLite errors while executing the operation or decoding its stored rows.
    pub fn search(&self, query: &str, kind: &str, limit: u32) -> Result<Vec<SearchRow>> {
        let terms: Vec<String> = query
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .map(|w| format!("\"{}\"*", w.replace('"', "")))
            .collect();
        if terms.is_empty() {
            return Ok(vec![]);
        }
        let fts = terms.join(" ");
        let mut st = self.conn.prepare_cached(
            "SELECT kind, ref_id, space, author, at_ms, title,
                    snippet(search, 6, '[[', ']]', '…', 10), snippet(search, 5, '[[', ']]', '…', 8)
             FROM search WHERE search MATCH ?1 AND kind = ?2
             ORDER BY bm25(search, 0.0, 0.0, 0.0, 0.0, 0.0, 4.0, 1.0), CAST(at_ms AS INTEGER) DESC, space ASC, ref_id ASC LIMIT ?3",
        )?;
        let rows = st.query_map(params![fts, kind, limit], |r| {
            Ok(SearchRow {
                kind: r.get(0)?,
                ref_id: r.get(1)?,
                space: r.get(2)?,
                author: r.get(3)?,
                at_ms: r.get::<_, i64>(4)?,
                title: r.get(5)?,
                snippet: r.get(6)?,
                title_snippet: r.get(7)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Conversation-only prefix search, newest first with an immutable ID tie-break.
    /// `after` is the last timestamp/ID returned; no OFFSET drift or global truncation.
    /// Returns at most `limit + 1` rows so callers can determine whether a next page exists.
    ///
    /// # Errors
    /// Returns `InvalidSearchLimit` outside 1..=100, or SQLite query/row-decoding errors.
    pub fn search_messages_page(
        &self,
        query: &str,
        space: &str,
        after: Option<(i64, &str)>,
        limit: u32,
    ) -> Result<Vec<SearchRow>> {
        if !(1..=100).contains(&limit) {
            return Err(StoreError::InvalidSearchLimit);
        }
        let terms: Vec<String> = query
            .split(|c: char| !c.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .map(|word| format!("\"{word}\"*"))
            .collect();
        if terms.is_empty() {
            return Ok(Vec::new());
        }
        let (at_ms, ref_id) = after.unwrap_or((0, ""));
        let mut statement = self.conn.prepare_cached(
            "SELECT kind, ref_id, space, author, at_ms, title,
                    snippet(search, 6, '[[', ']]', '…', 10),
                    snippet(search, 5, '[[', ']]', '…', 8)
             FROM search WHERE search MATCH ?1 AND kind = 'message' AND space = ?2
               AND (?3 = 0 OR CAST(at_ms AS INTEGER) < ?4
                    OR (CAST(at_ms AS INTEGER) = ?4 AND ref_id > ?5))
             ORDER BY CAST(at_ms AS INTEGER) DESC, ref_id ASC LIMIT ?6",
        )?;
        let rows = statement.query_map(
            params![
                terms.join(" "),
                space,
                after.is_some(),
                at_ms,
                ref_id,
                limit + 1
            ],
            |row| {
                Ok(SearchRow {
                    kind: row.get(0)?,
                    ref_id: row.get(1)?,
                    space: row.get(2)?,
                    author: row.get(3)?,
                    at_ms: row.get(4)?,
                    title: row.get(5)?,
                    snippet: row.get(6)?,
                    title_snippet: row.get(7)?,
                })
            },
        )?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Counts current rows in the disposable search index.
    ///
    /// # Errors
    /// Returns SQLite errors while executing the operation or decoding its stored rows.
    pub fn index_count(&self) -> Result<u64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM search", [], |r| r.get::<_, u64>(0))?)
    }
}
