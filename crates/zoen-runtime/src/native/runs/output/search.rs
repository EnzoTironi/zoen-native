//! Only complete, genuinely captured model proposals may request a native read.
use super::*;

impl Execution {
    pub(in crate::native::runs) async fn reply_search_proposal_in(
        &self,
        tx: &Transaction,
        record: &RunRecord,
        original: &str,
        custody: &Custody,
    ) -> Result<Option<(String, Vec<OutputBlock>)>, RuntimeError> {
        if record.format != 3 {
            return Ok(None);
        }
        let retained = Box::pin(self.output_capture_in(tx, record, original, custody)).await?;
        let result = &retained.capture.result;
        let Ok(blocks) = &result.output else {
            return Ok(None);
        };
        let [OutputBlock::ToolCall {
            name, arguments, ..
        }] = blocks.as_slice()
        else {
            return Ok(None);
        };
        super::super::turn::query(name, arguments)?;
        if !result.evidence.complete
            || !result
                .receipt
                .http_status
                .is_some_and(|s| (200..300).contains(&s))
            || result.receipt.reported_model.as_deref()
                != Some(record.binding.price.chat()?.model.as_str())
        {
            return Err(RuntimeError::Denied);
        }
        Ok(Some((retained.association.digest, blocks.clone())))
    }
}
