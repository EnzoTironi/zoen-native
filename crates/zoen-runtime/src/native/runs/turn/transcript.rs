//! Closed transcript shape; retained pages carry data, never tool authority.
use super::*;
use client_core::runtime_core::ReplyHistoryRead;

impl RunRecord {
    pub(in crate::native::runs) fn tool_reads(&self) -> Result<u8, RuntimeError> {
        match (self.format, self.continuation.as_ref()) {
            (5, None) => Ok(0),
            (5, Some(step)) if (1..3).contains(&step.used) => Ok(step.used),
            (6, Some(step)) if step.used == 3 => Ok(3),
            (1..=3, None) => Ok(0),
            (4, Some(step)) if step.used == 0 => Ok(1),
            _ => Err(RuntimeError::InvalidBinding),
        }
    }
    pub(in crate::native::runs) fn history_reads(
        &self,
    ) -> Result<Vec<ReplyHistoryRead>, RuntimeError> {
        let (_, reads) = transcript(self)?;
        Ok(reads)
    }
}
pub(super) fn base(record: &RunRecord) -> Result<&[InputMessage], RuntimeError> {
    transcript(record).map(|(base, _)| base)
}
fn transcript(
    record: &RunRecord,
) -> Result<(&[InputMessage], Vec<ReplyHistoryRead>), RuntimeError> {
    let used = record.tool_reads()?;
    let messages = &record.request.messages;
    if used == 0 {
        return Ok((messages, Vec::new()));
    }
    let offset = messages
        .iter()
        .position(|message| {
            matches!(message,
                InputMessage::Assistant {blocks} if !search_calls(blocks).is_empty()
            )
        })
        .ok_or(RuntimeError::InvalidBinding)?;
    let mut reads = Vec::new();
    let mut cursor = offset;
    let mut ids = std::collections::BTreeSet::new();
    while cursor < messages.len() {
        let InputMessage::Assistant { blocks } = &messages[cursor] else {
            return Err(RuntimeError::InvalidBinding);
        };
        let calls = search_calls(blocks);
        if calls.is_empty() || calls.len() > 3 || reads.len() + calls.len() > usize::from(used) {
            return Err(RuntimeError::InvalidBinding);
        }
        cursor += 1;
        for call in calls {
            let Some(InputMessage::ToolResult {
                call_id,
                name,
                text,
            }) = messages.get(cursor)
            else {
                return Err(RuntimeError::InvalidBinding);
            };
            if call_id != call.id
                || name != call.name
                || !ids.insert(call_id)
                || text.len() > 32 * 1024
            {
                return Err(RuntimeError::InvalidBinding);
            }
            query(call.name, call.arguments, record.format)?;
            let page = serde_json::from_str(text).map_err(|_| RuntimeError::InvalidBinding)?;
            reads.push(ReplyHistoryRead {
                arguments: call.arguments.clone(),
                page,
            });
            cursor += 1;
        }
    }
    if reads.len() != usize::from(used) {
        return Err(RuntimeError::InvalidBinding);
    }
    Ok((&messages[..offset], reads))
}
