//! Delivery completion is derived from the existing authenticated receipt.
//! It is not a dispatch capability, a refund or another terminal-state ledger.
use super::*;

pub(in crate::native) struct CompletedReply {
    pub(in crate::native) original: Root,
    pub(in crate::native) publication: Root,
}

impl Execution {
    pub(in crate::native) async fn completed_reply_in(
        &self,
        tx: &Transaction,
        run: &str,
        custody: &Custody,
    ) -> Result<Option<CompletedReply>, RuntimeError> {
        if tx
            .get(&self.run(run, "native-receipt/1"), false)
            .await?
            .is_none()
        {
            return Ok(None);
        }
        let (record, original) = self.reply_in(tx, run, custody).await?;
        let capture = Box::pin(self.output_capture_in(tx, &record, &original, custody)).await?;
        let publication = self
            .publication_in(tx, &record, &original, &capture.association.digest, custody)
            .await?
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        let receipt = self
            .receipt_in(tx, &record, &publication, custody)
            .await?
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        let source = self.reply_source()?;
        let accepted = source
            .position_in(tx, record.intent.space(), receipt.value.accepted.seq)
            .await
            .map_err(|_| RuntimeError::RestoreUnreconciled)?;
        let through = source
            .position_in(tx, record.intent.space(), receipt.value.source.seq)
            .await
            .map_err(|_| RuntimeError::RestoreUnreconciled)?;
        if accepted.hash != receipt.value.accepted.hash
            || through.hash != receipt.value.source.hash
            || accepted.env.client_id() != publication.value.client_id
            || hash(&protocol::ClientFrame::Publish { env: accepted.env }.encode())
                != publication.value.publish
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        Ok(Some(CompletedReply {
            original: record.original,
            publication: publication.value.root,
        }))
    }
}
