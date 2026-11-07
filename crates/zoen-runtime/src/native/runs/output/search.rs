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
        if !matches!(record.format, 3 | 5) {
            return Ok(None);
        }
        let retained = Box::pin(self.output_capture_in(tx, record, original, custody)).await?;
        search_proposal(record, &retained)
    }
}

/// Validate an already authenticated capture without loading its payload twice.
pub(in crate::native::runs) fn search_proposal(
    record: &RunRecord,
    retained: &RetainedCapture,
) -> Result<Option<(String, Vec<OutputBlock>)>, RuntimeError> {
    if !matches!(record.format, 3 | 5) {
        return Ok(None);
    }
    let result = &retained.capture.result;
    let Ok(blocks) = &result.output else {
        return Ok(None);
    };
    let calls = super::super::turn::search_calls(blocks);
    let limit = if record.format == 3 {
        1
    } else {
        3 - record.tool_reads()?
    };
    if calls.is_empty() || calls.len() > usize::from(limit) {
        return Ok(None);
    }
    for call in calls {
        super::super::turn::query(call.name, call.arguments, record.format)?;
    }
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
    Ok(Some((retained.association.digest.clone(), blocks.clone())))
}
