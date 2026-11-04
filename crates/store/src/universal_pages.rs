//! One globally ordered page of the existing FTS projection. The Core binds
//! offsets to its committed generation and current access before calling this.
use crate::{Result, SearchMediaKind, SearchRow, Store, StoreError};
use rusqlite::params;

/// Filters are applied to canonical indexed rows before ranking and pagination.
#[derive(Default)]
pub struct SearchPageFilters<'a> {
    pub period: Option<std::ops::Range<i64>>,
    pub space: Option<&'a str>,
    pub author: Option<&'a str>,
}

const MEDIA_PREVIEW_CHARS: u32 = 240;

impl Store {
    pub fn reset_search_media(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS search_media (
                 space TEXT NOT NULL, message TEXT NOT NULL, category TEXT NOT NULL DEFAULT 'file',
                 PRIMARY KEY(space, message)
             ) WITHOUT ROWID;
",
        )?;
        let has_category: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('search_media') WHERE name = 'category')", [], |row| row.get(0),
        )?;
        if !has_category {
            self.conn.execute_batch(
                "ALTER TABLE search_media ADD COLUMN category TEXT NOT NULL DEFAULT 'file';",
            )?;
        }
        self.conn.execute("DELETE FROM search_media", [])?;
        Ok(())
    }

    pub fn index_search_media(&self, space: &str, message: &str) -> Result<()> {
        self.index_search_media_kind(space, message, SearchMediaKind::File)?;
        Ok(())
    }

    pub fn index_search_media_kind(
        &self,
        space: &str,
        message: &str,
        kind: SearchMediaKind,
    ) -> Result<()> {
        let category = kind.category().ok_or(StoreError::InvalidSearchKind)?;
        self.conn.execute(
            "INSERT INTO search_media(space, message, category) VALUES (?1, ?2, ?3)
                ON CONFLICT(space, message) DO UPDATE SET category = excluded.category",
            params![space, message, category],
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
        self.universal_search_page_filtered(
            query,
            kinds,
            readable_spaces,
            offset,
            limit,
            SearchPageFilters {
                period: range,
                ..SearchPageFilters::default()
            },
        )
    }

    pub fn universal_search_page_filtered(
        &self,
        query: &str,
        kinds: &[String],
        readable_spaces: &[String],
        offset: u32,
        limit: u32,
        filters: SearchPageFilters<'_>,
    ) -> Result<Vec<SearchRow>> {
        if !(1..=100).contains(&limit) {
            return Err(StoreError::InvalidSearchLimit);
        }
        let terms: Vec<String> = query
            .split(|c: char| !c.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .map(|word| format!("\"{word}\"*"))
            .collect();
        let media = SearchMediaKind::from_filter(kinds);
        let links = kinds == ["link"];
        let browse = query.trim().is_empty()
            && (links || media.is_some() || filters.author.is_some() || filters.space.is_some());
        if terms.is_empty() && !browse {
            return Ok(Vec::new());
        }
        let media_clause = if links {
            "AND EXISTS (SELECT 1 FROM search_links l
                         WHERE l.space = search.space AND l.message = search.ref_id)"
        } else if media.is_some() {
            "AND EXISTS (SELECT 1 FROM search_media m
                         WHERE m.space = search.space AND m.message = search.ref_id
                           AND (?10 IS NULL OR m.category = ?10))"
        } else {
            ""
        };
        let message_kind = ["message".to_owned()];
        let kinds = if links || media.is_some() {
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
               AND (?8 IS NULL OR (kind = 'message' AND space = ?8))
               AND (?9 IS NULL OR (kind = 'message' AND author = ?9))
               AND (?10 IS NULL OR kind = 'message')
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
                filters.period.as_ref().map(|range| range.start),
                filters.period.as_ref().map(|range| range.end),
                filters.space,
                filters.author,
                media.and_then(SearchMediaKind::category),
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
