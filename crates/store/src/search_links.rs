//! Disposable traits of the same authorized message/FTS generation, never history.
use crate::{Result, Store};
impl Store {
    pub fn reset_search_links(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS search_links (
                 space TEXT NOT NULL, message TEXT NOT NULL,
                 PRIMARY KEY(space, message)
             ) WITHOUT ROWID;
             DELETE FROM search_links;",
        )?;
        Ok(())
    }

    pub fn index_search_links(&self, space: &str, message: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO search_links(space, message) VALUES (?1, ?2)",
            rusqlite::params![space, message],
        )?;
        Ok(())
    }
}
