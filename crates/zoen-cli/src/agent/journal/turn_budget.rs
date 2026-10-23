//! The ordinary source retry limit survives host death, in the existing journal.
use super::*;
use sha2::{Digest, Sha256};

impl JournalState {
    pub(in crate::agent) fn begin_turn(
        path: &Path,
        context: &Ctx,
        space: &str,
        source: &TimelineEntry,
    ) -> Result<bool, String> {
        let text = text_of(source).ok_or("The source is not a message")?;
        let digest = |value: serde_json::Value| {
            hex::encode(Sha256::digest(
                serde_json::to_vec(&value).expect("JSON value"),
            ))
        };
        // Separate from signed request IDs: this record budgets an unreviewed
        // source turn, never a continuation permit or a never-started witness.
        let key = format!(
            "turn_{}",
            digest(json!([
                "zoen.agent.source-budget.v1",
                context.me,
                context.device,
                context.owner,
                space,
                source.id
            ]))
        );
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
}
