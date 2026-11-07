//! Captured tool decisions refine the same paid original's shared commitment.
mod delivery;
mod history;
mod record;
mod shared;
mod stage;
use super::output::{open_metadata, seal_metadata, RetainedCapture, SealedRecord};
use super::turn::{search_calls, SearchCall};
use super::*;
use crate::native::shared::SharedSource;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use foundationdb::Transaction;
use protocol::{ClientFrame, Envelope};
use record::{DecisionRecord, Review};
use relay_admission::managed_images::{Prepared, SubjectUpdate};
pub(in crate::native) use shared::{LiveToolDecision, SharedToolDecision};
use std::{collections::BTreeMap, sync::Arc};
use zoen_models::OutputBlock;

fn subject(run: &str, index: u8) -> String {
    format!("native-tool-decision/1/{run}/{index}")
}
fn key(execution: &Execution, run: &str, index: u8) -> Vec<u8> {
    execution.run(run, &format!("native-tool-decision/1/{index}"))
}
pub(crate) fn commitment(previous: &str, decision: &str) -> Result<String, RuntimeError> {
    Ok(hash(
        &serde_json::to_vec(&("runtime-capture-tool/1", previous, decision))
            .map_err(|_| RuntimeError::InvalidBinding)?,
    ))
}

fn tool_check(record: &RunRecord, call: &SearchCall<'_>) -> Result<gate::Check, RuntimeError> {
    let definition = record
        .request
        .tools
        .iter()
        .find(|tool| tool.name == call.name)
        .ok_or(RuntimeError::InvalidBinding)?;
    let user = turn::base_messages(record)?
        .iter()
        .rev()
        .find_map(|m| match m {
            InputMessage::User { text } => Some(text.clone()),
            _ => None,
        })
        .ok_or(RuntimeError::InvalidBinding)?;
    let mut check = gate::Check::new(
        gate::Point::ToolCall,
        format!(
            "conversation tool {} ({}) {}",
            call.name, definition.description, call.arguments
        ),
    );
    check.user_intent = Some(user);
    Ok(check)
}

struct ToolOriginal<'a> {
    record: &'a RunRecord,
    original: &'a str,
    capture: &'a RetainedCapture,
    blocks: &'a [OutputBlock],
}
impl Execution {
    async fn tool_decision_in(
        &self,
        runtime: &RuntimeAuthority,
        tx: &Transaction,
        captured: ToolOriginal<'_>,
        index: u8,
    ) -> Result<Option<SealedRecord<DecisionRecord>>, RuntimeError> {
        let ToolOriginal {
            record,
            original,
            capture,
            blocks,
        } = captured;
        let Some(stored) = self
            .retained_tool_decision_in(runtime, tx, &record.locator.run, index)
            .await?
        else {
            return Ok(None);
        };
        stored
            .value
            .validate(record, original, capture, blocks, index)?;
        Ok(Some(stored))
    }

    async fn retained_tool_decision_in(
        &self,
        runtime: &RuntimeAuthority,
        tx: &Transaction,
        run: &str,
        index: u8,
    ) -> Result<Option<SealedRecord<DecisionRecord>>, RuntimeError> {
        let Some(bytes) = tx.get(&key(self, run, index), false).await? else {
            return Ok(None);
        };
        let stored: SealedRecord<DecisionRecord> =
            open_metadata(&runtime.custody, &subject(run, index), &bytes)?;
        if stored.value.format != 1
            || stored.value.run != run
            || stored.value.index != index
            || stored.value.sequence == 0
        {
            return Err(RuntimeError::InvalidBinding);
        }
        let history = tx
            .get(
                &crate::continuity::history(self, stored.value.sequence),
                false,
            )
            .await?
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        let prepared: Prepared =
            serde_json::from_slice(&history).map_err(|_| RuntimeError::InvalidBinding)?;
        crate::native::shared::tool_transition(&runtime.custody, &prepared, run, index, &bytes)?;
        let observed = runtime
            .runtime_continuity
            .as_ref()
            .ok_or(RuntimeError::RestoreUnreconciled)?
            .reader()
            .observed_transition(&prepared.next)
            .map_err(crate::managed_images::error)?;
        if serde_json::to_vec(&observed)
            .map_err(|_| RuntimeError::InvalidBinding)?
            .as_slice()
            != history.as_ref()
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        if tx
            .get(
                &self
                    .native_references(&stored.value.root)
                    .pack(&(format!("tool/1/{index}"), run)),
                false,
            )
            .await?
            .as_deref()
            != Some(stored.value.root.manifest_digest.as_bytes())
        {
            return Err(RuntimeError::RestoreUnreconciled);
        }
        Ok(Some(stored))
    }

    pub(crate) async fn shared_tool_digests_in(
        &self,
        runtime: &RuntimeAuthority,
        tx: &Transaction,
        binding: &Binding,
        association: Option<&str>,
    ) -> Result<Vec<String>, RuntimeError> {
        let run = &binding.context.run_id;
        if tx.get(&key(self, run, 0), false).await?.is_none() {
            return Ok(Vec::new());
        }
        let (record, original) = self.reply_in(tx, run, &runtime.custody).await?;
        if record.binding != *binding {
            return Err(RuntimeError::InvalidBinding);
        }
        // Inventory proves correspondence, not permission to perform a read.
        // Its caller already authenticated the capture and association bytes.
        let association = association.ok_or(RuntimeError::RestoreUnreconciled)?;
        let mut result = Vec::new();
        let mut gap = false;
        for index in 0..3 {
            match self
                .retained_tool_decision_in(runtime, tx, run, index)
                .await?
            {
                Some(stored) if !gap => {
                    if stored.value.original != original
                        || stored.value.capture != association
                        || stored.value.root.context.principal != record.original.context.principal
                    {
                        return Err(RuntimeError::InvalidBinding);
                    }
                    result.push(stored.digest);
                }
                Some(_) => return Err(RuntimeError::RestoreUnreconciled),
                None => gap = true,
            }
        }
        Ok(result)
    }
}

impl Execution {
    pub(in crate::native) async fn retained_tool_root_in(
        &self,
        tx: &Transaction,
        run: &str,
        index: u8,
        custody: &Custody,
    ) -> Result<Root, RuntimeError> {
        let bytes = tx
            .get(&key(self, run, index), false)
            .await?
            .ok_or(RuntimeError::RestoreUnreconciled)?;
        let stored: SealedRecord<DecisionRecord> =
            open_metadata(custody, &subject(run, index), &bytes)?;
        let (record, original) = self.reply_in(tx, run, custody).await?;
        let capture = self
            .output_capture_in(tx, &record, &original, custody)
            .await?;
        let (_, blocks) =
            output::search_proposal(&record, &capture)?.ok_or(RuntimeError::InvalidBinding)?;
        stored
            .value
            .validate(&record, &original, &capture, &blocks, index)?;
        Ok(stored.value.root)
    }
}
