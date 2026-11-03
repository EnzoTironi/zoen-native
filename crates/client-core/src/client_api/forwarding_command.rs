//! Reviewed forwarding commands compose into the shared strict, flat JSON boundary.
use crate::forwarding::{
    ForwardBatchScope, ForwardBatchSource, ForwardOptions, ForwardScope, ForwardSource,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum ForwardingCommand {
    ForwardBatchStatus {
        source: ForwardBatchSource,
    },
    ForwardBatchReview {
        scope: ForwardBatchScope,
    },
    ConfirmForwardBatch {
        source: ForwardBatchSource,
        batch_id: String,
        review_revision: String,
    },
    AbandonForwardBatch {
        source: ForwardBatchSource,
        batch_id: String,
        review_revision: String,
    },
    FinishForwardBatch {
        source: ForwardBatchSource,
        batch_id: String,
    },
    ForwardReview {
        scope: ForwardScope,
    },
    ForwardStatus {
        source: ForwardSource,
    },
    ConfirmForward {
        scope: ForwardScope,
        operation_id: String,
        source_revision: String,
    },
    DismissForward {
        scope: ForwardScope,
        operation_id: String,
        accepted_message_id: Option<String>,
    },
    SetForwardOptions {
        scope: ForwardScope,
        operation_id: String,
        source_revision: String,
        options: ForwardOptions,
    },
    SetForwardBatchOptions {
        source: ForwardBatchSource,
        batch_id: String,
        review_revision: String,
        options: ForwardOptions,
    },
}
