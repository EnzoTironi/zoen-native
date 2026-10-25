//! Bounded encrypted delivery effects. No HTML or second document is stored here.
use super::{
    forbidden, invalid, stale, CoreError, Deserialize, Engine, Serialize, SignedMutation,
    SiteReview, SiteScope, R,
};
use hkdf::Hkdf;
use protocol::sites::Receipt;
use rusqlite::OptionalExtension;
use sha2::Sha256;
use zeroize::Zeroizing;

const DOMAIN: &[u8] = b"zoen.site.publication-journal.v1";
const MAX_PLAIN: usize = 24 * 1024;
const MAX_CIPHER: usize = (MAX_PLAIN + 40).div_ceil(3) * 4 + 3;
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    pending: Option<SiteReview>,
    receipt: Option<Receipt>,
}
fn storage() -> CoreError {
    CoreError::Storage {
        message: crate::i18n::t(
            "Não foi possível recuperar a publicação. Nenhuma nova tentativa foi criada.",
            "Publication recovery failed. No new attempt was created.",
        ),
    }
}

impl Engine {
    fn site_journal_key(
        &self,
        scope: &SiteScope,
        site: &str,
    ) -> R<(String, Vec<u8>, Zeroizing<[u8; 32]>)> {
        self.site_scope(scope, true)?;
        let author = self.net.author.as_ref().ok_or_else(forbidden)?;
        let aad = serde_json::to_vec(&(
            "zoen.site.publication-journal.v1",
            &scope.identity_id,
            &scope.device_id,
            &scope.space_id,
            site,
        ))
        .map_err(|_| storage())?;
        let id = format!("site.publication.{}", event_log::sha256_hex(&aad));
        let mut key = Zeroizing::new([0u8; 32]);
        Hkdf::<Sha256>::new(Some(DOMAIN), &author.key.secret())
            .expand(b"local effect encryption", &mut *key)
            .map_err(|_| storage())?;
        Ok((id, aad, key))
    }
    fn site_journal_load(&self, scope: &SiteScope, site: &str) -> R<Record> {
        let (id, aad, key) = self.site_journal_key(scope, site)?;
        let encoded: Option<Option<String>> = self.store.conn().query_row(
            "SELECT CASE WHEN length(CAST(value AS BLOB))<=?2 THEN value ELSE NULL END FROM meta WHERE key=?1",
            rusqlite::params![id, MAX_CIPHER], |row| row.get(0)).optional().map_err(|_| storage())?;
        let Some(encoded) = encoded else {
            return Ok(Record::default());
        };
        let plain = Zeroizing::new(
            crate::local_sealed::open(&key, &aad, &encoded.ok_or_else(storage)?, MAX_PLAIN)
                .map_err(|()| storage())?,
        );
        let record: Record = serde_json::from_slice(&plain).map_err(|_| storage())?;
        if record
            .pending
            .as_ref()
            .is_some_and(|r| r.scope != *scope || r.mutation.site_id != site)
            || record
                .receipt
                .as_ref()
                .is_some_and(|r| r.owner_id != scope.identity_id || r.site_id != site)
        {
            return Err(storage());
        }
        Ok(record)
    }
    fn site_journal_store(&self, scope: &SiteScope, site: &str, record: &Record) -> R<()> {
        let (id, aad, key) = self.site_journal_key(scope, site)?;
        let plain = Zeroizing::new(serde_json::to_vec(record).map_err(|_| storage())?);
        if plain.len() > MAX_PLAIN {
            return Err(storage());
        }
        let encoded = crate::local_sealed::seal(&key, &aad, &plain).map_err(|()| storage())?;
        let (count, bytes, old): (usize, usize, usize) = self.store.conn().query_row(
            "SELECT COUNT(*),COALESCE(SUM(length(CAST(value AS BLOB))),0),COALESCE(MAX(CASE WHEN key=?1 THEN length(CAST(value AS BLOB)) END),0) FROM meta WHERE key LIKE 'site.publication.%'",
            [&id], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?))).map_err(|_| storage())?;
        if (old == 0 && count >= 4096)
            || bytes
                .checked_sub(old)
                .and_then(|b| b.checked_add(encoded.len()))
                .is_none_or(|b| b > 64 << 20)
        {
            return Err(storage());
        }
        self.store.set_meta(&id, &encoded)?;
        Ok(())
    }
    pub(super) fn site_pending(&self, scope: &SiteScope, site: &str) -> R<Option<SiteReview>> {
        Ok(self.site_journal_load(scope, site)?.pending)
    }
    pub(super) fn site_begin_publication(&self, review: &SiteReview) -> R<()> {
        let mut record = self.site_journal_load(&review.scope, &review.mutation.site_id)?;
        if record.pending.as_ref().is_some_and(|r| r != review) {
            return Err(stale());
        }
        record.pending = Some(review.clone());
        self.site_journal_store(&review.scope, &review.mutation.site_id, &record)
    }
    pub(super) fn site_publication_result(
        &self,
        review: &SiteReview,
        receipt: Option<&Receipt>,
    ) -> R<()> {
        let mut record = self.site_journal_load(&review.scope, &review.mutation.site_id)?;
        if record.pending.is_none() && receipt.is_some() && record.receipt.as_ref() == receipt {
            return Ok(());
        }
        if record.pending.as_ref() != Some(review) {
            return Err(stale());
        }
        if let Some(receipt) = receipt {
            let identity = SignedMutation {
                owner_id: review.scope.identity_id.clone(),
                device_id: review.scope.device_id.clone(),
                at_ms: 0,
                mutation: review.mutation.clone(),
                signature: String::new(),
            };
            let expected = Receipt::for_mutation(&identity, &review.configuration.audience)
                .map_err(|_| invalid())?;
            if receipt != &expected {
                return Err(storage());
            }
            record.receipt = Some(receipt.clone());
        }
        record.pending = None;
        self.site_journal_store(&review.scope, &review.mutation.site_id, &record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encrypted_intent_survives_reload_and_never_replaces_an_ambiguous_request() {
        let (client, scope, source) = super::super::tests::fixture();
        let mut e = client.lock();
        let r = super::super::tests::review(&scope, &source).unwrap();
        e.site_begin_publication(&r).unwrap();
        e.reload_sync().unwrap();
        assert_eq!(
            e.site_pending(&scope, &r.mutation.site_id).unwrap(),
            Some(r.clone())
        );
        let mut competing = r.clone();
        competing.mutation.request_id = types::new_secret_id("pub").unwrap();
        assert!(e.site_begin_publication(&competing).is_err());
        let raw: String = e
            .store
            .conn()
            .query_row(
                "SELECT value FROM meta WHERE key LIKE 'site.publication.%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            raw.starts_with("v1:")
                && !raw.contains(&r.mutation.request_id)
                && !raw.contains("Private")
        );
        let prepared = e.site_prepare(&r, &r.relay_url, &r.configuration).unwrap();
        let receipt = Receipt::for_mutation(&prepared.signed, &r.configuration.audience).unwrap();
        let mut wrong = receipt.clone();
        wrong.head = "0".repeat(64);
        assert!(e.site_publication_result(&r, Some(&wrong)).is_err());
        assert!(e
            .site_pending(&scope, &r.mutation.site_id)
            .unwrap()
            .is_some());
        e.site_publication_result(&r, Some(&receipt)).unwrap();
        assert!(e
            .site_pending(&scope, &r.mutation.site_id)
            .unwrap()
            .is_none());
    }
}
