//! The same sealed local-metadata pattern as normal composer drafts. A single
//! current review/receipt per source message, retaining its chosen destination.
use super::*;
use hkdf::Hkdf;
use rusqlite::OptionalExtension;
use zeroize::Zeroizing;
const PREFIX: &str = "message.forward.";
const DOMAIN: &[u8] = b"zoen.message.forward.v1";
const MAX_PLAIN: usize = crate::client_api::MAX_REQUEST_BYTES + 16 * 1024;
const MAX_CIPHER: usize = (MAX_PLAIN + 40).div_ceil(3) * 4 + 3;
const MAX_SCOPES: usize = 4096;
const MAX_RETAINED_BYTES: usize = 64 << 20;
struct Storage {
    id: String,
    aad: Vec<u8>,
    key: Zeroizing<[u8; 32]>,
}
impl Engine {
    fn forward_storage(&self, scope: &ForwardSource) -> R<Storage> {
        self.forward_source_access(scope)?;
        let aad = serde_json::to_vec(&("zoen.message.forward.v1", scope)).map_err(|_| invalid())?;
        let id = format!("{PREFIX}{}", hex::encode(Sha256::digest(&aad)));
        let author = self.author_for(&scope.expected_identity_id)?;
        let mut key = Zeroizing::new([0u8; 32]);
        Hkdf::<Sha256>::new(Some(DOMAIN), &author.key.secret())
            .expand(b"local forward encryption", &mut *key)
            .map_err(|_| storage_error())?;
        Ok(Storage { id, aad, key })
    }
    pub(super) fn load_forward(&self, scope: &ForwardSource) -> R<Option<Record>> {
        let s = self.forward_storage(scope)?;
        let row: Option<Option<String>> = self.store.conn().query_row(
            "SELECT CASE WHEN length(CAST(value AS BLOB)) <= ?2 THEN value ELSE NULL END FROM meta WHERE key=?1",
            rusqlite::params![s.id, MAX_CIPHER], |r| r.get(0)).optional().map_err(|_| storage_error())?;
        let Some(encoded) = row else {
            return Ok(None);
        };
        let encoded = encoded.ok_or_else(storage_error)?;
        let plain = Zeroizing::new(
            crate::local_sealed::open(&s.key, &s.aad, &encoded, MAX_PLAIN)
                .map_err(|_| storage_error())?,
        );
        let record: Record = serde_json::from_slice(&plain).map_err(|_| storage_error())?;
        if !valid_id(&record.target_space)
            || !valid_id(&record.operation_id)
            || record.source_revision.len() != 64
            || !record
                .source_revision
                .bytes()
                .all(|b| b.is_ascii_hexdigit())
            || record.message.id != scope.message_id
            || record
                .accepted_message
                .as_deref()
                .is_some_and(|id| !valid_id(id))
        {
            return Err(storage_error());
        }
        Ok(Some(record))
    }
    pub(super) fn persist_forward(&self, scope: &ForwardScope, record: &Record) -> R<()> {
        let s = self.forward_storage(&scope.source())?;
        let plain = Zeroizing::new(serde_json::to_vec(record).map_err(|_| storage_error())?);
        if plain.len() > MAX_PLAIN {
            return Err(invalid());
        }
        let encoded =
            crate::local_sealed::seal(&s.key, &s.aad, &plain).map_err(|_| storage_error())?;
        let (count, bytes, old): (usize, usize, usize) = self.store.conn().query_row(
            "SELECT COUNT(*), COALESCE(SUM(length(CAST(value AS BLOB))),0), COALESCE(MAX(CASE WHEN key=?1 THEN length(CAST(value AS BLOB)) END),0) FROM meta WHERE key LIKE 'message.forward.%'",
            [&s.id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(|_| storage_error())?;
        if (old == 0 && count >= MAX_SCOPES)
            || bytes
                .checked_sub(old)
                .and_then(|n| n.checked_add(encoded.len()))
                .is_none_or(|n| n > MAX_RETAINED_BYTES)
        {
            return Err(invalid());
        }
        self.store.set_meta(&s.id, &encoded)?;
        Ok(())
    }
    pub(super) fn remove_forward(&self, scope: &ForwardScope) -> R<()> {
        let s = self.forward_storage(&scope.source())?;
        self.store
            .conn()
            .execute("DELETE FROM meta WHERE key = ?1", [&s.id])
            .map_err(|_| storage_error())?;
        Ok(())
    }
}
