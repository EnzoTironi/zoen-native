//! Existing FTS lookup and conversation paging.
use crate::{Result, SearchRow, Store, StoreError};
use rusqlite::params;

impl Store {
    /// Remove a row while rebuilding an index or preparing a disposable view.
    /// The caller owns the index transaction/generation; this is not log state.
    pub fn index_remove_message(&self, space: &str, message: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM search WHERE kind = 'message' AND space = ?1 AND ref_id = ?2",
            params![space, message],
        )?;
        Ok(())
    }

    /// Prefix search over the index. `query` is the user's text; every word must match
    /// (as a prefix, diacritics folded). Best matches first, then newest.
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
        let hashtag = crate::hashtags::query(query);
        let terms: Vec<String> = hashtag
            .as_deref()
            .map_or(query, |tag| tag)
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
                    snippet(search, 5, '[[', ']]', '…', 8), body
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
                if hashtag.is_some() {
                    -1_i64
                } else {
                    i64::from(limit) + 1
                }
            ],
            |row| {
                Ok((
                    SearchRow {
                        kind: row.get(0)?,
                        ref_id: row.get(1)?,
                        space: row.get(2)?,
                        author: row.get(3)?,
                        at_ms: row.get(4)?,
                        title: row.get(5)?,
                        snippet: row.get(6)?,
                        title_snippet: row.get(7)?,
                    },
                    row.get::<_, String>(8)?,
                ))
            },
        )?;
        let mut hits = Vec::new();
        for row in rows {
            let (hit, body) = row?;
            if hashtag
                .as_ref()
                .is_none_or(|tag| crate::hashtags::contains(&body, tag))
            {
                hits.push(hit);
                if hits.len() > limit as usize {
                    break;
                }
            }
        }
        Ok(hits)
    }
}
