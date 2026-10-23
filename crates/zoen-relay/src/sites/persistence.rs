use object_store::{path::Path, ObjectStoreExt, PutMode, PutOptions, PutPayload};
use protocol::sites::{Change, Receipt, SignedMutation};
use sqlx::{Postgres, Transaction};

use super::{Error, Service};

fn database(_: sqlx::Error) -> Error {
    Error::Unavailable
}

impl Service {
    async fn human_fence(
        &self,
        signed: &SignedMutation,
    ) -> Result<Transaction<'static, Postgres>, Error> {
        let mut tx =
            crate::db::authorize_device(&self.pool, &signed.owner_id, &signed.device_id, true)
                .await
                .map_err(database)?
                .ok_or(Error::Unauthorized)?;
        let human = sqlx::query_scalar::<_, bool>(
            "SELECT kind = 'Person' AND owner IS NULL FROM identities WHERE id = $1 FOR SHARE",
        )
        .bind(&signed.owner_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(database)?;
        if human != Some(true) {
            return Err(Error::Unauthorized);
        }
        Ok(tx)
    }

    pub(super) async fn persist(
        &self,
        signed: &SignedMutation,
        object_key: Option<&str>,
        html: Option<&[u8]>,
    ) -> Result<Receipt, Error> {
        let mut tx = self.human_fence(signed).await?;
        // Serializes the first insert too. A hash collision only serializes
        // unrelated sites; all authorization/CAS comparisons use exact IDs.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(&signed.mutation.site_id)
            .execute(&mut *tx)
            .await
            .map_err(database)?;
        let current = sqlx::query_as::<_, (String, String)>(
            "SELECT owner_id, head FROM site_publications WHERE site_id = $1 FOR UPDATE",
        )
        .bind(&signed.mutation.site_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(database)?;
        if current
            .as_ref()
            .is_some_and(|(owner, _)| owner != &signed.owner_id)
        {
            return Err(Error::Unauthorized);
        }
        let intent = signed.intent_digest(&self.audience);
        let existing = sqlx::query_as::<_, (String, serde_json::Value)>(
            "SELECT intent_sha256, receipt FROM site_publication_receipts
             WHERE site_id = $1 AND request_id = $2",
        )
        .bind(&signed.mutation.site_id)
        .bind(&signed.mutation.request_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(database)?;
        if let Some((digest, receipt)) = existing {
            if digest != intent {
                return Err(Error::Conflict);
            }
            let receipt = serde_json::from_value(receipt).map_err(|_| Error::Unavailable)?;
            tx.commit().await.map_err(database)?;
            return Ok(receipt);
        }
        if current.as_ref().map(|(_, head)| head) != signed.mutation.expected_head.as_ref() {
            return Err(Error::Conflict);
        }
        if let (Some(key), Some(bytes)) = (object_key, html) {
            self.publication_kib
                .check_n(&signed.owner_id, (bytes.len() as u64).div_ceil(1024) as u32)
                .map_err(|_| Error::Limited)?;
            let path = Path::from(key);
            let put = self.store.put_opts(
                &path,
                PutPayload::from_bytes(bytes.to_vec().into()),
                PutOptions {
                    mode: PutMode::Create,
                    ..Default::default()
                },
            );
            match tokio::time::timeout(std::time::Duration::from_secs(5), put).await {
                Ok(Ok(_)) => {}
                Ok(Err(object_store::Error::AlreadyExists { .. })) => {
                    // A retained orphan from a failed earlier transaction is
                    // reusable only if storage still holds the exact bytes.
                    let existing = self.load_object(&path, bytes.len() as u64).await?;
                    if existing.as_ref() != bytes {
                        return Err(Error::Unavailable);
                    }
                }
                _ => return Err(Error::Unavailable),
            }
        }
        let receipt = Receipt::for_mutation(signed, &self.audience);
        let source = match &signed.mutation.change {
            Change::Publish { source } => Some(source),
            Change::Unpublish => None,
        };
        sqlx::query(
            "INSERT INTO site_publications
             (site_id, owner_id, head, active, object_key, byte_length, content_sha256)
             VALUES ($1,$2,$3,$4,$5,$6,$7)
             ON CONFLICT (site_id) DO UPDATE SET
               head = EXCLUDED.head, active = EXCLUDED.active, object_key = EXCLUDED.object_key,
               byte_length = EXCLUDED.byte_length, content_sha256 = EXCLUDED.content_sha256,
               updated_at = now()",
        )
        .bind(&receipt.site_id)
        .bind(&receipt.owner_id)
        .bind(&receipt.head)
        .bind(receipt.active)
        .bind(object_key)
        .bind(source.map(|source| source.byte_length as i64))
        .bind(source.map(|source| &source.sha256))
        .execute(&mut *tx)
        .await
        .map_err(database)?;
        sqlx::query(
            "INSERT INTO site_publication_receipts(site_id, request_id, intent_sha256, receipt)
             VALUES ($1,$2,$3,$4)",
        )
        .bind(&receipt.site_id)
        .bind(&receipt.request_id)
        .bind(&intent)
        .bind(serde_json::to_value(&receipt).map_err(|_| Error::Unavailable)?)
        .execute(&mut *tx)
        .await
        .map_err(database)?;
        tx.commit().await.map_err(database)?;
        Ok(receipt)
    }

    pub(super) async fn public_bytes(&self, site_id: &str) -> Result<Vec<u8>, Error> {
        if !relay_admission::sites::valid_site(site_id) {
            return Err(Error::Missing);
        }
        // Keep a shared head fence through the bounded fetch: unpublish cannot
        // confirm while a newly admitted public read still selects the old head.
        let mut tx = self.pool.begin().await.map_err(database)?;
        sqlx::query("SELECT set_config('statement_timeout', '5s', true)")
            .execute(&mut *tx)
            .await
            .map_err(database)?;
        let row = sqlx::query_as::<_, (String, i64, String)>(
            "SELECT object_key, byte_length, content_sha256 FROM site_publications
             WHERE site_id = $1 AND active FOR SHARE",
        )
        .bind(site_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(database)?
        .ok_or(Error::Missing)?;
        if !(1..=protocol::sites::MAX_HTML_BYTES as i64).contains(&row.1) {
            return Err(Error::Unavailable);
        }
        let bytes = self.load_object(&Path::from(row.0), row.1 as u64).await?;
        if bytes.len() as i64 != row.1 || event_log::sha256_hex(&bytes) != row.2 {
            return Err(Error::Unavailable);
        }
        tx.commit().await.map_err(database)?;
        Ok(bytes.to_vec())
    }

    async fn load_object(&self, path: &Path, length: u64) -> Result<bytes::Bytes, Error> {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let result = self.store.get(path).await.map_err(|_| Error::Unavailable)?;
            if result.meta.size != length || length > protocol::sites::MAX_HTML_BYTES {
                return Err(Error::Unavailable);
            }
            super::storage::bounded_bytes(result.into_stream(), length).await
        })
        .await
        .map_err(|_| Error::Unavailable)?
    }
}
