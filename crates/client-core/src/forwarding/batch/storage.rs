//! Sealed current intent per ordered source set. Authenticated completed and
//! zero-output reviews can be compacted; partial acceptance is never evicted.
use super::*;
use hkdf::Hkdf;
use rusqlite::OptionalExtension;
use zeroize::Zeroizing;
const PREFIX: &str = "message.forwardbatch.";
const DOMAIN: &[u8] = b"zoen.message.forwardbatch.v1";
const MAX_PLAIN: usize = 8 << 20;
const MAX_CIPHER: usize = (MAX_PLAIN + 40).div_ceil(3) * 4 + 3;
pub(super) const MAX_SCOPES: usize = 512;
const MAX_RETAINED_BYTES: usize = 64 << 20;
struct Storage {
    id: String,
    aad: Vec<u8>,
    key: Zeroizing<[u8; 32]>,
}
fn batch_aad(source: &ForwardBatchSource, id: &str) -> R<Vec<u8>> {
    // The hash binds every source ID and its order; known row keys permit
    // authenticated compaction without a second plaintext completion index.
    serde_json::to_vec(&(
        "zoen.message.forwardbatch.v1",
        &source.expected_identity_id,
        &source.expected_device_id,
        id,
    ))
    .map_err(|_| invalid())
}
impl Engine {
    fn batch_storage(&self, source: &ForwardBatchSource) -> R<Storage> {
        self.batch_access(source)?;
        let bytes = serde_json::to_vec(source).map_err(|_| invalid())?;
        let id = format!("{PREFIX}{}", hex::encode(Sha256::digest(bytes)));
        let aad = batch_aad(source, &id)?;
        let author = self.author_for(&source.expected_identity_id)?;
        let mut key = Zeroizing::new([0u8; 32]);
        Hkdf::<Sha256>::new(Some(DOMAIN), &author.key.secret())
            .expand(b"local batch forward encryption", &mut *key)
            .map_err(|_| storage_error())?;
        Ok(Storage { id, aad, key })
    }
    pub(super) fn load_forward_batch(&self, source: &ForwardBatchSource) -> R<Option<Batch>> {
        let s = self.batch_storage(source)?;
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
        let batch: Batch = serde_json::from_slice(&plain).map_err(|_| storage_error())?;
        if !valid_id(&batch.batch_id)
            || !valid_id(&batch.review_revision)
            || !valid_id(&batch.target_space)
            || batch.records.len() != source.message_ids.len()
            || batch
                .records
                .iter()
                .zip(&source.message_ids)
                .any(|(record, id)| {
                    record.message.id != *id
                        || !valid_id(&record.target_space)
                        || (record.accepted_message.is_none()
                            && record.target_space != batch.target_space)
                        || !valid_id(&record.operation_id)
                        || record.source_revision.len() != 64
                        || !record
                            .source_revision
                            .bytes()
                            .all(|b| b.is_ascii_hexdigit())
                        || record
                            .accepted_message
                            .as_deref()
                            .is_some_and(|id| !valid_id(id))
                })
            || batch.skipped_message_ids.iter().any(|id| {
                !source.message_ids.contains(id)
                    || batch
                        .records
                        .iter()
                        .any(|r| r.message.id == *id && r.accepted_message.is_some())
            })
            || batch
                .skipped_message_ids
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                != batch.skipped_message_ids.len()
            || (batch.completed
                && batch.records.iter().any(|r| {
                    r.accepted_message.is_none()
                        && !batch.skipped_message_ids.contains(&r.message.id)
                }))
        {
            return Err(storage_error());
        }
        Ok(Some(batch))
    }
    fn make_batch_room(&self, source: &ForwardBatchSource, s: &Storage, size: usize) -> R<()> {
        let (retained,total,old):(usize,usize,usize)=self.store.conn().query_row(
            "SELECT COUNT(*),COALESCE(SUM(length(CAST(value AS BLOB))),0),COALESCE(MAX(CASE WHEN key=?1 THEN length(CAST(value AS BLOB)) END),0) FROM meta WHERE key LIKE 'message.forwardbatch.%'",
            [&s.id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(|_|storage_error())?;
        let mut count = retained + usize::from(old == 0);
        let mut bytes = total
            .checked_sub(old)
            .and_then(|n| n.checked_add(size))
            .ok_or_else(invalid)?;
        if count <= MAX_SCOPES && bytes <= MAX_RETAINED_BYTES {
            return Ok(());
        }
        let mut rows=self.store.conn().prepare(
            "SELECT key,CASE WHEN length(CAST(value AS BLOB))<=?1 THEN value ELSE NULL END FROM meta WHERE key LIKE 'message.forwardbatch.%' ORDER BY key LIMIT ?2").map_err(|_| storage_error())?;
        let entries = rows
            .query_map(rusqlite::params![MAX_CIPHER, MAX_SCOPES + 1], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
            })
            .map_err(|_| storage_error())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| storage_error())?;
        for (id, encoded) in entries {
            if count <= MAX_SCOPES && bytes <= MAX_RETAINED_BYTES {
                return Ok(());
            }
            if id == s.id {
                continue;
            }
            let Some(encoded) = encoded else {
                continue;
            };
            let aad = batch_aad(source, &id)?;
            let Ok(plain) = crate::local_sealed::open(&s.key, &aad, &encoded, MAX_PLAIN) else {
                continue;
            };
            let plain = Zeroizing::new(plain);
            let Ok(batch) = serde_json::from_slice::<Batch>(&plain) else {
                continue;
            };
            let none = batch.records.iter().all(|r| r.accepted_message.is_none());
            let complete = batch.completed
                && batch.records.iter().all(|r| {
                    r.accepted_message.is_some()
                        || batch.skipped_message_ids.contains(&r.message.id)
                });
            if !none && !complete {
                continue;
            }
            self.store
                .conn()
                .execute("DELETE FROM meta WHERE key=?1", [id])
                .map_err(|_| storage_error())?;
            count -= 1;
            bytes -= encoded.len();
        }
        if count > MAX_SCOPES || bytes > MAX_RETAINED_BYTES {
            return Err(invalid());
        }
        Ok(())
    }
    pub(super) fn persist_forward_batch(
        &self,
        source: &ForwardBatchSource,
        batch: &Batch,
    ) -> R<()> {
        let s = self.batch_storage(source)?;
        let plain = Zeroizing::new(serde_json::to_vec(batch).map_err(|_| storage_error())?);
        if plain.len() > MAX_PLAIN {
            return Err(invalid());
        }
        let encoded =
            crate::local_sealed::seal(&s.key, &s.aad, &plain).map_err(|_| storage_error())?;
        // Compaction and admission are one metadata transaction, nested safely
        // inside an individual signed publication's existing agent_write savepoint.
        self.store
            .conn()
            .execute_batch("SAVEPOINT forward_batch_storage")
            .map_err(|_| storage_error())?;
        let result = self
            .make_batch_room(source, &s, encoded.len())
            .and_then(|()| {
                self.store.set_meta(&s.id, &encoded)?;
                Ok(())
            });
        match result {
            Ok(()) => self
                .store
                .conn()
                .execute_batch("RELEASE forward_batch_storage")
                .map_err(|_| storage_error()),
            Err(reason) => {
                self.store
                    .conn()
                    .execute_batch(
                        "ROLLBACK TO forward_batch_storage; RELEASE forward_batch_storage",
                    )
                    .map_err(|_| storage_error())?;
                Err(reason)
            }
        }
    }
}
