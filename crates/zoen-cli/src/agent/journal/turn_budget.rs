//! The ordinary source retry limit survives host death, in the existing journal.
use super::*;
use sha2::{Digest, Sha256};

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
        Self::begin_resume(path, &key, &binding, Some(&source.id))
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
