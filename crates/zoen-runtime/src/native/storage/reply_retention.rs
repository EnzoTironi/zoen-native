//! Small immutable reference rows remain audit proofs after image collection.
use super::*;
use crate::custody::Custody;

pub(super) enum ReplyRetention {
    Pinned,
    Timed,
    Completed,
}

impl Execution {
    pub(super) async fn retired_reply_retention(
        &self,
        tx: &Transaction,
        root: &Root,
        custody: Option<&Custody>,
    ) -> Result<ReplyRetention, RuntimeError> {
        let prefix = self.native_references(root);
        let mut range = foundationdb::RangeOption::from(prefix.range());
        range.limit = Some(1);
        if tx.get_range(&range, 1, false).await?.is_empty() {
            return Ok(ReplyRetention::Timed);
        }
        let Some(custody) = custody else {
            return Ok(ReplyRetention::Pinned);
        };
        // A discovery batch has at most 16 runs; a publication adds one reference.
        let rows = bounded_rows(
            tx,
            foundationdb::RangeOption::from(prefix.range()),
            18,
            8192,
        )
        .await?;
        if rows.len() > 17 {
            return Ok(ReplyRetention::Pinned);
        }
        for (key, value) in rows {
            let (run, publication) = if let Ok((run,)) = prefix.unpack::<(String,)>(&key) {
                (run, false)
            } else if let Ok((kind, run)) = prefix.unpack::<(String, String)>(&key) {
                if kind != "publication/1" {
                    return Ok(ReplyRetention::Pinned);
                }
                (run, true)
            } else {
                // Other subsystems' references retain their original pin semantics.
                return Ok(ReplyRetention::Pinned);
            };
            if value != root.manifest_digest.as_bytes() {
                return Err(RuntimeError::InvalidBinding);
            }
            if !crate::id(&run) {
                return Err(RuntimeError::InvalidBinding);
            }
            let Some(completed) = Box::pin(self.completed_reply_in(tx, &run, custody)).await?
            else {
                return Ok(ReplyRetention::Pinned);
            };
            let expected = if publication {
                &completed.publication
            } else {
                &completed.original
            };
            if expected != root {
                return Err(RuntimeError::InvalidBinding);
            }
        }
        Ok(ReplyRetention::Completed)
    }
}

impl crate::RuntimeAuthority {
    pub(in crate::native) async fn collect_reply_retired(
        &self,
        principal: &Principal,
    ) -> Result<(), RuntimeError> {
        self.check_shared_runtime().await?;
        // Keep retention/evidence processing off each native caller's inline
        // future; image initialization and source hydration share this entry.
        Box::pin(
            self.execution
                .collect_native_retired_with_receipts(principal, &self.custody),
        )
        .await
    }
}
