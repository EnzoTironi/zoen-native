//! One globally ordered page of the existing FTS projection. The Core binds
//! offsets to its committed generation and current access before calling this.
use crate::{Result, SearchRow, Store, StoreError};
use rusqlite::params;

const MEDIA_PREVIEW_CHARS: u32 = 240;

impl Store {
    pub fn reset_search_media(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS search_media (
                 space TEXT NOT NULL, message TEXT NOT NULL,
                 PRIMARY KEY(space, message)
             ) WITHOUT ROWID;
             DELETE FROM search_media;",
        )?;
        Ok(())
    }

    pub fn index_search_media(&self, space: &str, message: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO search_media(space, message) VALUES (?1, ?2)",
            params![space, message],
        )?;
        Ok(())
    }

    pub fn universal_search_page(
        &self,
        query: &str,
        kinds: &[String],
        readable_spaces: &[String],
        offset: u32,
        limit: u32,
    ) -> Result<Vec<SearchRow>> {
        self.universal_search_page_in_range(query, kinds, readable_spaces, offset, limit, None)
    }

    pub fn universal_search_page_in_range(
        &self,
        query: &str,
        kinds: &[String],
        readable_spaces: &[String],
        offset: u32,
        limit: u32,
        range: Option<std::ops::Range<i64>>,
    ) -> Result<Vec<SearchRow>> {
        if !(1..=100).contains(&limit) {
            return Err(StoreError::InvalidSearchLimit);
        }
        let terms: Vec<String> = query
            .split(|c: char| !c.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .map(|word| format!("\"{word}\"*"))
            .collect();
        let browse = query.trim().is_empty() && kinds == ["media"];
        if terms.is_empty() && !browse {
            return Ok(Vec::new());
        }
        let media_clause = if kinds == ["media"] {
            "AND EXISTS (SELECT 1 FROM search_media m
                         WHERE m.space = search.space AND m.message = search.ref_id)"
        } else {
            ""
        };
        let message_kind = ["message".to_owned()];
        let kinds = if kinds == ["media"] {
            &message_kind[..]
        } else {
            kinds
        };
        let (body, title, matching, rank) = if browse {
            (
                "substr(body, 1, ?1) || CASE WHEN length(body) > ?1 THEN '…' ELSE '' END",
                "title",
                "1",
                "0.0",
            )
        } else {
            (
                "snippet(search, 6, '[[', ']]', '…', 10)",
                "snippet(search, 5, '[[', ']]', '…', 8)",
                "search MATCH ?1",
                "bm25(search, 0.0, 0.0, 0.0, 0.0, 0.0, 4.0, 1.0)",
            )
        };
        let sql = format!(
            "SELECT kind, ref_id, space, author, at_ms, title,
                    {body}, {title}
             FROM search WHERE {matching}
               AND kind IN (SELECT value FROM json_each(?2))
               AND (space = '' OR space IN (SELECT value FROM json_each(?3)))
               AND (?6 IS NULL OR CAST(at_ms AS INTEGER) >= ?6)
               AND (?7 IS NULL OR CAST(at_ms AS INTEGER) < ?7)
             {media_clause}
             ORDER BY {rank},
                      CAST(at_ms AS INTEGER) DESC, kind ASC, space ASC, ref_id ASC
             LIMIT ?4 OFFSET ?5",
        );
        let mut statement = self.conn.prepare_cached(&sql)?;
        let match_or_preview = if browse {
            rusqlite::types::Value::from(i64::from(MEDIA_PREVIEW_CHARS))
        } else {
            rusqlite::types::Value::from(terms.join(" "))
        };
        let rows = statement.query_map(
            params![
                match_or_preview,
                serde_json::to_string(kinds)?,
                serde_json::to_string(readable_spaces)?,
                limit + 1,
                offset,
                range.as_ref().map(|range| range.start),
                range.as_ref().map(|range| range.end),
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
