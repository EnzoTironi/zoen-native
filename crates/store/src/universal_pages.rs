//! One globally ordered page of the existing FTS projection. The Core binds
//! offsets to its committed generation and current access before calling this.
use crate::{Result, SearchRow, Store, StoreError};
use rusqlite::params;

impl Store {
    pub fn universal_search_page(
        &self,
        query: &str,
        kinds: &[String],
        readable_spaces: &[String],
        offset: u32,
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
        let mut statement = self.conn.prepare_cached(
            "SELECT kind, ref_id, space, author, at_ms, title,
                    snippet(search, 6, '[[', ']]', '…', 10),
                    snippet(search, 5, '[[', ']]', '…', 8)
             FROM search WHERE search MATCH ?1
               AND kind IN (SELECT value FROM json_each(?2))
               AND (space = '' OR space IN (SELECT value FROM json_each(?3)))
             ORDER BY bm25(search, 0.0, 0.0, 0.0, 0.0, 0.0, 4.0, 1.0),
                      CAST(at_ms AS INTEGER) DESC, kind ASC, space ASC, ref_id ASC
             LIMIT ?4 OFFSET ?5",
        )?;
        let rows = statement.query_map(
            params![
                terms.join(" "),
                serde_json::to_string(kinds)?,
                serde_json::to_string(readable_spaces)?,
                limit + 1,
                offset
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
}
