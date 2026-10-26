//! The ordinary source retry limit survives host death, in the existing journal.
use super::*;
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::agent) enum ModelOutcome {
    Unknown,
    NotDispatched,
}

fn digest(value: serde_json::Value) -> String {
    hex::encode(Sha256::digest(
        serde_json::to_vec(&value).expect("JSON value"),
    ))
}

fn key(context: &Ctx, space: &str, source: &str) -> String {
    format!(
        "turn_{}",
        digest(json!([
            "zoen.agent.source-budget.v1",
            context.me,
            context.device,
            context.owner,
            space,
            source
        ]))
    )
}

impl JournalState {
    pub(in crate::agent) fn begin_turn(
        path: &Path,
        context: &Ctx,
        space: &str,
        source: &TimelineEntry,
    ) -> Result<bool, String> {
        let text = text_of(source).ok_or("The source is not a message")?;
        // Separate from signed request IDs: this record budgets an unreviewed
        // source turn, never a continuation permit or a never-started witness.
        let key = key(context, space, &source.id);
        let binding = digest(json!([
            "zoen.agent.source-budget-content.v1",
            source.author.id,
            text
        ]));
        // The original atomic write/fsync/rename and exclusive temp reservation
        // apply before any Runtime/Gate/model/tool IO. An interrupted reservation
        // consumes the attempt; a missing ACK cannot restore it after restart.
        Self::update(path, |state| {
            if state
                .continuations
                .get(&key)
                .is_some_and(|prior| prior.model_outcome != Some(ModelOutcome::NotDispatched))
            {
                // Historical reservations without this witness are unknown too.
                // A retry cannot manufacture evidence that inference did not run.
                return Err("The source model outcome needs reconciliation".into());
            }
            let reserved = state.reserve_resume(&key, &binding, Some(&source.id))?;
            if reserved {
                state.continuations.get_mut(&key).unwrap().model_outcome =
                    Some(ModelOutcome::Unknown);
            }
            Ok(reserved)
        })
    }

    /// Only an initial, explicitly non-dispatched Brain error reaches this port.
    /// It does not erase the source binding or restore the operational budget.
    pub(in crate::agent) fn model_not_dispatched(
        path: &Path,
        context: &Ctx,
        space: &str,
        source: &TimelineEntry,
    ) -> Result<(), String> {
        let key = key(context, space, &source.id);
        let binding = digest(json!([
            "zoen.agent.source-budget-content.v1",
            source.author.id,
            text_of(source).ok_or("The source is not a message")?
        ]));
        Self::update(path, |state| {
            let prior = state
                .continuations
                .get_mut(&key)
                .ok_or("The source attempt is missing")?;
            if prior.content_hash != binding
                || prior.source.as_deref() != Some(&source.id)
                || prior.model_outcome != Some(ModelOutcome::Unknown)
                || prior.attempts == 0
            {
                return Err("The source model witness changed".into());
            }
            prior.model_outcome = Some(ModelOutcome::NotDispatched);
            Ok(true)
        })
        .map(|_| ())
    }

    /// Only the normal host calls this AFTER the relay acknowledges its cursor.
    /// That canonical cursor owns completion; this journal never duplicates it.
    /// Failed turns/unknown advance outcomes keep their reservation across restart.
    pub(in crate::agent) fn retire_turn(
        path: &Path,
        context: &Ctx,
        space: &str,
        source: &str,
    ) -> Result<(), String> {
        let key = key(context, space, source);
        // No temp file or legacy-format rewrite for messages with no reservation.
        if !Self::load(path)?.continuations.contains_key(&key) {
            return Ok(());
        }
        Self::update(path, |state| Ok(state.continuations.remove(&key).is_some())).map(|_| ())
    }
}
