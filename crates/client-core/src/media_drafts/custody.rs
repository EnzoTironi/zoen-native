//! Custody on the original Engine and canonical document state.

use super::{
    invalid, storage_error, validate, Digest, Engine, Hkdf, MediaDraftAsset, MediaDraftScope,
    OptionalExtension, Record, Sha256, Storage, Zeroizing, DOMAIN, MAX_ASSET_BYTES,
    MAX_RECORD_BYTES, MAX_RETAINED_BYTES, MAX_SCOPES, MEDIA_DRAFT_REVISION_ZERO, PREFIX, R,
};

impl Engine {
    pub(super) fn media_draft_scope(&self, scope: &MediaDraftScope) -> R<Storage> {
        self.validate_private_draft_scope(&scope.normal())?;
        let author = self.author_for(&scope.expected_identity_id)?;
        let aad = serde_json::to_vec(&(
            "zoen.media.draft.record.v1",
            &scope.expected_identity_id,
            &scope.expected_device_id,
            &scope.space_id,
            &scope.thread_root_id,
        ))
        .map_err(|_| storage_error())?;
        let mut key = Zeroizing::new([0u8; 32]);
        Hkdf::<Sha256>::new(Some(DOMAIN), &author.key.secret())
            .expand(b"local media draft encryption", &mut *key)
            .map_err(|_| storage_error())?;
        Ok(Storage {
            id: format!("{PREFIX}record.{}", hex::encode(Sha256::digest(&aad))),
            aad,
            key,
        })
    }

    pub(super) fn media_draft_row(&self, key: &str, max_plain: usize) -> R<Option<String>> {
        let maximum = (max_plain + 40).div_ceil(3) * 4 + 3;
        let row: Option<Option<String>> = self.store.conn().query_row(
            "SELECT CASE WHEN length(CAST(value AS BLOB)) <= ?2 THEN value ELSE NULL END FROM meta WHERE key=?1",
            rusqlite::params![key, maximum], |r| r.get(0)).optional().map_err(|_| storage_error())?;
        row.map(|value| value.ok_or_else(storage_error)).transpose()
    }

    pub(super) fn load_media_draft(&self, storage: &Storage) -> R<Record> {
        let Some(encoded) = self.media_draft_row(&storage.id, MAX_RECORD_BYTES)? else {
            return Ok(Record::default());
        };
        let plain = Zeroizing::new(
            crate::local_sealed::open(&storage.key, &storage.aad, &encoded, MAX_RECORD_BYTES)
                .map_err(|()| storage_error())?,
        );
        let record: Record = serde_json::from_slice(&plain).map_err(|_| storage_error())?;
        if record.revision == MEDIA_DRAFT_REVISION_ZERO {
            return Err(storage_error());
        }
        validate(&record).map_err(|_| storage_error())?;
        Ok(record)
    }

    pub(super) fn persist_media_record(&self, storage: &Storage, record: &Record) -> R<()> {
        validate(record)?;
        let plain = Zeroizing::new(serde_json::to_vec(record).map_err(|_| storage_error())?);
        if plain.len() > MAX_RECORD_BYTES {
            return Err(invalid());
        }
        let encrypted = crate::local_sealed::seal(&storage.key, &storage.aad, &plain)
            .map_err(|()| storage_error())?;
        self.store.set_meta(&storage.id, &encrypted)?;
        let (scopes, bytes): (usize, usize) = self.store.conn().query_row(
            "SELECT COUNT(CASE WHEN key LIKE 'media.draft.record.%' THEN 1 END), COALESCE(SUM(length(CAST(value AS BLOB))),0) FROM meta WHERE key LIKE 'media.draft.%'",
            [], |r| Ok((r.get(0)?, r.get(1)?))).map_err(|_| storage_error())?;
        if scopes > MAX_SCOPES || bytes > MAX_RETAINED_BYTES {
            return Err(invalid());
        }
        Ok(())
    }

    /// Shared encrypted replacement. The caller owns the transaction; this
    /// helper cannot append events, mutate the projection, or run transport.
    pub(super) fn persist_media_replacement(
        &self,
        storage: &Storage,
        previous: &Record,
        record: &Record,
        fresh: &[(MediaDraftAsset, Zeroizing<Vec<u8>>)],
    ) -> R<()> {
        for (asset, bytes) in fresh {
            let encrypted =
                crate::local_sealed::seal(&storage.key, &storage.asset_aad(asset)?, bytes)
                    .map_err(|()| storage_error())?;
            self.store
                .set_meta(&storage.asset_key(&asset.id), &encrypted)?;
        }
        for asset in &previous.assets {
            if !record.assets.iter().any(|next| next.id == asset.id) {
                self.store.meta_delete(&storage.asset_key(&asset.id))?;
            }
        }
        self.persist_media_record(storage, record)
    }

    pub(super) fn load_media_asset(
        &self,
        storage: &Storage,
        asset: &MediaDraftAsset,
    ) -> R<Zeroizing<Vec<u8>>> {
        let encoded = self
            .media_draft_row(&storage.asset_key(&asset.id), MAX_ASSET_BYTES)?
            .ok_or_else(storage_error)?;
        let bytes = Zeroizing::new(
            crate::local_sealed::open(
                &storage.key,
                &storage.asset_aad(asset)?,
                &encoded,
                MAX_ASSET_BYTES,
            )
            .map_err(|()| storage_error())?,
        );
        if bytes.len() as u64 != asset.size || hex::encode(Sha256::digest(&*bytes)) != asset.sha256
        {
            return Err(storage_error());
        }
        Ok(bytes)
    }
}
