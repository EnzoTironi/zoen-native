//! Disposable ordering accelerator for attachment classifications in the existing FTS index.
use super::*;

#[derive(Debug, Clone)]
pub struct RecentAttachmentIndexRow {
    pub space: String,
    pub message: String,
    pub author: String,
    pub at_ms: i64,
    pub entry_index: u64,
}

impl Store {
    /// Called only inside the existing index rebuild savepoint. Never a source of truth.
    pub fn recent_attachments_reset(&self) -> Result<()> {
        self.conn.execute_batch(
            "DROP TABLE IF EXISTS attachment_recent;
             CREATE TABLE attachment_recent (
                kind TEXT NOT NULL, space TEXT NOT NULL, message TEXT NOT NULL,
                author TEXT NOT NULL, at_ms INTEGER NOT NULL, order_at INTEGER NOT NULL, entry_index INTEGER NOT NULL,
                fts_rowid INTEGER NOT NULL, PRIMARY KEY(kind, space, message)
             ) WITHOUT ROWID;
             CREATE INDEX attachment_recent_newest ON attachment_recent(kind, order_at, space, message);
             CREATE INDEX attachment_recent_author_newest ON attachment_recent(kind, author, order_at, space, message);
             INSERT INTO meta(key,value) VALUES('search:attachment-format','1')
                ON CONFLICT(key) DO UPDATE SET value='1';",
        )?;
        Ok(())
    }

    pub fn recent_attachments_ready(&self) -> Result<bool> {
        Ok(self.conn.query_row(
            "SELECT COALESCE((SELECT value FROM meta WHERE key='search:attachment-format'),'')='1'
               AND (SELECT COUNT(*) FROM sqlite_master WHERE name IN
                    ('attachment_recent','attachment_recent_newest','attachment_recent_author_newest'))=3",
            [],
            |row| row.get(0),
        )?)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn recent_attachment_put(
        &self,
        kind: &str,
        space: &str,
        message: &str,
        author: &str,
        at_ms: i64,
        entry_index: u64,
        name: &str,
    ) -> Result<()> {
        // One globally unique FTS reference even when peers choose equal message ids.
        let reference = serde_json::to_string(&(space, message))?;
        self.index_put(kind, &reference, space, author, at_ms, name, name)?;
        self.conn.execute(
            "INSERT INTO attachment_recent(kind,space,message,author,at_ms,entry_index,fts_rowid,order_at)
             VALUES(?1,?2,?3,?4,?5,?6,
                (SELECT rowid FROM search WHERE kind=?1 AND ref_id=?7),?8)",
            params![kind, space, message, author, at_ms, entry_index, reference, !at_ms],
        )?;
        Ok(())
    }

    pub fn recent_attachment_reference(
        &self,
        kind: &str,
        space: &str,
        message: &str,
    ) -> Result<Option<RecentAttachmentIndexRow>> {
        Ok(self
            .conn
            .query_row(
                "SELECT space,message,author,at_ms,entry_index,order_at FROM attachment_recent
             WHERE kind=?1 AND space=?2 AND message=?3",
                params![kind, space, message],
                recent_row,
            )
            .optional()?)
    }

    /// Filtering precedes LIMIT. The B-tree provides newest/keyset order; FTS only
    /// answers text membership and cannot truncate the candidate set prematurely.
    pub fn recent_attachments_page(
        &self,
        kind: &str,
        author: Option<&str>,
        query: &str,
        after: Option<(i64, &str, &str)>,
        limit: u32,
    ) -> Result<Vec<RecentAttachmentIndexRow>> {
        if !(1..=50).contains(&limit) {
            return Err(StoreError::InvalidSearchLimit);
        }
        let terms: Vec<_> = query
            .split(|c: char| !c.is_alphanumeric())
            .filter(|word| !word.is_empty())
            .map(|word| format!("\"{word}\"*"))
            .collect();
        if !query.trim().is_empty() && terms.is_empty() {
            return Ok(Vec::new());
        }
        let (at, space, message) = after.unwrap_or((0, "", ""));
        let sql = recent_sql(author.is_some(), after.is_some(), !terms.is_empty());
        let mut statement = self.conn.prepare_cached(&sql)?;
        let mut values: Vec<rusqlite::types::Value> = vec![
            kind.to_owned().into(),
            author.unwrap_or("").to_owned().into(),
            (!at).into(),
            space.to_owned().into(),
            message.to_owned().into(),
            i64::from(limit + 1).into(),
        ];
        if !terms.is_empty() {
            values.push(terms.join(" ").into());
        }
        let rows = statement.query_map(rusqlite::params_from_iter(values), recent_row)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
}

fn recent_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RecentAttachmentIndexRow> {
    let at_ms: i64 = row.get(3)?;
    if row.get::<_, i64>(5)? != !at_ms {
        return Err(rusqlite::Error::InvalidQuery);
    }
    Ok(RecentAttachmentIndexRow {
        space: row.get(0)?,
        message: row.get(1)?,
        author: row.get(2)?,
        at_ms,
        entry_index: row.get(4)?,
    })
}

fn recent_sql(author: bool, after: bool, matching: bool) -> String {
    let ordering = if author {
        "attachment_recent_author_newest"
    } else {
        "attachment_recent_newest"
    };
    let authored = if author { " AND author=?2" } else { "" };
    // Bitwise complement exactly reverses signed-i64 order, including extrema.
    // All three ordering columns can therefore use one B-tree tuple seek.
    let keyset = if after {
        " AND (order_at,space,message)>(?3,?4,?5)"
    } else {
        ""
    };
    let matching = if matching {
        " AND fts_rowid IN (SELECT rowid FROM search WHERE search MATCH ?7)"
    } else {
        ""
    };
    format!("SELECT space,message,author,at_ms,entry_index,order_at FROM attachment_recent INDEXED BY {ordering}
        WHERE kind=?1{authored}{keyset}{matching}
        ORDER BY order_at ASC,space ASC,message ASC LIMIT ?6")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn newest_and_deep_keyset_use_btree_seeks_and_filter_fts_before_limit() {
        let store = Store::in_memory().unwrap();
        store.index_begin().unwrap();
        store.recent_attachments_reset().unwrap();
        store.conn.execute_batch(
            "WITH RECURSIVE n(i) AS (VALUES(1) UNION ALL SELECT i+1 FROM n WHERE i<5000)
             INSERT INTO search(kind,ref_id,space,author,at_ms,title,body)
             SELECT 'attachment_file',printf('%06d',i),'source','self',i,
                CASE WHEN i%100=0 THEN 'needle.pdf' ELSE 'other.pdf' END,'' FROM n;
             INSERT INTO attachment_recent(kind,space,message,author,at_ms,order_at,entry_index,fts_rowid)
             SELECT kind,space,ref_id,author,CAST(at_ms AS INTEGER),~CAST(at_ms AS INTEGER),0,rowid FROM search;"
        ).unwrap();
        store.index_commit().unwrap();
        for after in [false, true] {
            let sql = format!("EXPLAIN QUERY PLAN {}", recent_sql(true, after, false));
            let plan = store
                .conn
                .prepare(&sql)
                .unwrap()
                .query_map(
                    params!["attachment_file", "self", !2500i64, "source", "002500", 3],
                    |row| row.get::<_, String>(3),
                )
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap();
            eprintln!("attachment keyset after={after}: {plan:?}");
            assert!(
                plan.iter().any(|line| line.contains(
                    "SEARCH attachment_recent USING INDEX attachment_recent_author_newest"
                )),
                "{plan:?}"
            );
            assert!(
                !plan.iter().any(|line| line.contains("TEMP B-TREE")),
                "{plan:?}"
            );
            if after {
                assert!(
                    plan.iter()
                        .any(|line| line.contains("(order_at,space,message)>(?,?,?)")),
                    "{plan:?}"
                );
            }
        }
        let first = store
            .recent_attachments_page("attachment_file", Some("self"), "needle", None, 2)
            .unwrap();
        assert_eq!(
            first.iter().map(|row| row.at_ms).collect::<Vec<_>>(),
            vec![5000, 4900, 4800]
        );
        let deep = store
            .recent_attachments_page(
                "attachment_file",
                Some("self"),
                "needle",
                Some((2500, "source", "002500")),
                2,
            )
            .unwrap();
        assert_eq!(
            deep.iter().map(|row| row.at_ms).collect::<Vec<_>>(),
            vec![2400, 2300, 2200]
        );
    }

    #[test]
    fn equal_time_cross_space_ties_and_i64_edges_are_lossless() {
        let store = Store::in_memory().unwrap();
        store.index_begin().unwrap();
        store.recent_attachments_reset().unwrap();
        for (space, message, at) in [
            ("b", "same", i64::MAX),
            ("a", "same", i64::MAX),
            ("z", "old", i64::MIN),
        ] {
            store
                .recent_attachment_put("attachment_music", space, message, "any", at, 0, "song")
                .unwrap();
        }
        store.index_commit().unwrap();
        let first = store
            .recent_attachments_page("attachment_music", None, "", None, 1)
            .unwrap();
        assert_eq!(
            (first[0].space.as_str(), first[1].space.as_str()),
            ("a", "b")
        );
        let next = store
            .recent_attachments_page(
                "attachment_music",
                None,
                "",
                Some((i64::MAX, "a", "same")),
                1,
            )
            .unwrap();
        assert_eq!((next[0].space.as_str(), next[1].at_ms), ("b", i64::MIN));
    }
}
