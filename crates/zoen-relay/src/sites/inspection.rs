use super::{Error, Service};
use protocol::sites::{Inspection, Receipt, SignedInspection};

impl Service {
    pub async fn inspect(
        &self,
        signed: &SignedInspection,
        now_ms: i64,
    ) -> Result<Inspection, Error> {
        let _permit = self.in_flight.try_acquire().map_err(|_| Error::Limited)?;
        relay_admission::sites::authorize_inspection(signed, &self.configuration.audience, now_ms)
            .map_err(|_| Error::Unauthorized)?;
        if signed.public_origin != self.configuration.public_origin {
            return Err(Error::Invalid);
        }
        let mut tx = self
            .human_fence(&signed.owner_id, &signed.device_id)
            .await?;
        let current = sqlx::query_as::<_, (String, String, bool)>(
            "SELECT owner_id, head, active FROM site_publications WHERE site_id=$1 FOR SHARE",
        )
        .bind(&signed.site_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| Error::Unavailable)?;
        let Some((owner, head, active)) = current else {
            if signed.before.is_some() {
                return Err(Error::Missing);
            }
            tx.commit().await.map_err(|_| Error::Unavailable)?;
            return Ok(Inspection {
                site_id: signed.site_id.clone(),
                head: None,
                active: false,
                receipts: vec![],
                next_before: None,
            });
        };
        if owner != signed.owner_id {
            return Err(Error::Unauthorized);
        }
        let start = signed.before.as_ref().unwrap_or(&head);
        let rows = sqlx::query_as::<_, (String, serde_json::Value)>(
            "WITH RECURSIVE chain AS (
                SELECT intent_sha256, receipt, 1 AS depth FROM site_publication_receipts
                WHERE site_id=$1 AND intent_sha256=$2
                UNION ALL
                SELECT r.intent_sha256, r.receipt, c.depth+1 FROM site_publication_receipts r
                JOIN chain c ON r.intent_sha256=c.receipt->>'previous_head'
                WHERE r.site_id=$1 AND c.depth<101
             ) SELECT intent_sha256, receipt FROM chain ORDER BY depth",
        )
        .bind(&signed.site_id)
        .bind(start)
        .fetch_all(&mut *tx)
        .await
        .map_err(|_| Error::Unavailable)?;
        if rows.is_empty() {
            return Err(Error::Missing);
        }
        let mut receipts = Vec::with_capacity(rows.len());
        let mut expected = Some(start.clone());
        for (digest, value) in rows {
            let receipt: Receipt = serde_json::from_value(value).map_err(|_| Error::Unavailable)?;
            if receipt.site_id != signed.site_id
                || receipt.owner_id != signed.owner_id
                || receipt.head != digest
                || expected.as_ref() != Some(&digest)
            {
                return Err(Error::Unavailable);
            }
            expected = receipt.previous_head.clone();
            receipts.push(receipt);
        }
        if receipts.len() <= 100 && expected.is_some() {
            return Err(Error::Unavailable);
        }
        let next_before = if receipts.len() > 100 {
            receipts.pop().map(|receipt| receipt.head)
        } else {
            None
        };
        tx.commit().await.map_err(|_| Error::Unavailable)?;
        Ok(Inspection {
            site_id: signed.site_id.clone(),
            head: Some(head),
            active,
            receipts,
            next_before,
        })
    }
}
