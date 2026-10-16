use crate::{Result, Store, StoreError};
use rusqlite::{Connection, MAIN_DB};

pub const MAX_MEMORY_IMAGE_BYTES: usize = 4 * 1024 * 1024;

fn page_size(bytes: &[u8]) -> Result<usize> {
    if !(100..=MAX_MEMORY_IMAGE_BYTES).contains(&bytes.len())
        || bytes.get(..16) != Some(b"SQLite format 3\0")
        || bytes[18] != 1
        || bytes[19] != 1
    {
        return Err(StoreError::InvalidImage);
    }
    let encoded = u16::from_be_bytes([bytes[16], bytes[17]]);
    let size = if encoded == 1 {
        65536
    } else {
        usize::from(encoded)
    };
    if !(512..=65536).contains(&size)
        || !size.is_power_of_two()
        || !bytes.len().is_multiple_of(size)
    {
        return Err(StoreError::InvalidImage);
    }
    Ok(size)
}

impl Store {
    /// Complete, committed in-memory database, including every provider table.
    /// These plaintext bytes confer no provenance; the custody owner must seal them.
    pub fn memory_image(&self) -> Result<Vec<u8>> {
        if !self.conn.is_autocommit() {
            return Err(StoreError::InvalidImage);
        }
        let mode: String = self
            .conn
            .pragma_query_value(None, "journal_mode", |r| r.get(0))?;
        if mode != "memory" {
            return Err(StoreError::InvalidImage);
        }
        let pages: u64 = self
            .conn
            .pragma_query_value(None, "page_count", |r| r.get(0))?;
        let size: u64 = self
            .conn
            .pragma_query_value(None, "page_size", |r| r.get(0))?;
        if pages
            .checked_mul(size)
            .is_none_or(|n| n > MAX_MEMORY_IMAGE_BYTES as u64)
        {
            return Err(StoreError::InvalidImage);
        }
        let data = self.conn.serialize(MAIN_DB)?;
        page_size(&data)?;
        Ok(data.to_vec())
    }

    /// Deserialize without running migrations. The native facade must verify
    /// its exact supported schema and event/provider versions before reuse.
    pub fn from_memory_image(bytes: &[u8]) -> Result<Self> {
        let size = page_size(bytes)?;
        let mut conn = Connection::open_in_memory()?;
        conn.pragma_update(None, "trusted_schema", false)?;
        conn.deserialize_read_exact(MAIN_DB, bytes, bytes.len(), false)?;
        conn.pragma_update(
            None,
            "max_page_count",
            (MAX_MEMORY_IMAGE_BYTES / size) as u64,
        )?;
        Ok(Self { conn })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_image_keeps_provider_style_tables_and_unmigrated_metadata() {
        let store = Store::in_memory().unwrap();
        store.conn().execute_batch("CREATE TABLE vc_retained_fixture (data BLOB); INSERT INTO vc_retained_fixture VALUES (x'010203');").unwrap();
        store.set_meta("event_format", "legacy-marker").unwrap();
        let imported = Store::from_memory_image(&store.memory_image().unwrap()).unwrap();
        let bytes: Vec<u8> = imported
            .conn()
            .query_row("SELECT data FROM vc_retained_fixture", [], |r| r.get(0))
            .unwrap();
        assert_eq!(bytes, vec![1, 2, 3]);
        assert_eq!(
            imported.meta("event_format").unwrap().as_deref(),
            Some("legacy-marker")
        );
    }

    #[test]
    fn export_requires_commit_and_import_bounds_growth() {
        let store = Store::in_memory().unwrap();
        let tx = store.conn().unchecked_transaction().unwrap();
        assert!(matches!(
            store.memory_image(),
            Err(StoreError::InvalidImage)
        ));
        tx.rollback().unwrap();
        let imported = Store::from_memory_image(&store.memory_image().unwrap()).unwrap();
        assert!(imported
            .conn()
            .execute(
                "INSERT INTO media VALUES ('large','fixture',zeroblob(?1),0)",
                [MAX_MEMORY_IMAGE_BYTES as i64]
            )
            .is_err());
        assert!(!imported.has_media("large").unwrap());
        assert!(imported.memory_image().is_ok());
    }

    #[test]
    fn malformed_oversized_and_wal_images_are_refused() {
        let store = Store::in_memory().unwrap();
        let mut image = store.memory_image().unwrap();
        image[18] = 2;
        assert!(Store::from_memory_image(&image).is_err());
        assert!(Store::from_memory_image(&vec![0; MAX_MEMORY_IMAGE_BYTES + 1]).is_err());
        assert!(Store::from_memory_image(b"SQLite format 3\0").is_err());
    }
}
