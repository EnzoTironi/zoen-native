//! Private changes to the exact reviewed output, with a fresh confirmation fence.
use super::*;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct ForwardOptions {
    pub hide_author: bool,
    pub hide_caption: bool,
}

impl Engine {
    pub(crate) fn set_forward_options(
        &mut self,
        scope: &ForwardScope,
        operation: &str,
        revision: &str,
        options: ForwardOptions,
    ) -> R<ForwardReview> {
        self.forward_access(scope)?;
        let mut record = self.load_forward(&scope.source())?.ok_or_else(stale)?;
        if record.target_space != scope.target_space
            || record.operation_id != operation
            || record.source_revision != revision
            || record.accepted_message.is_some()
            || !self
                .state
                .writer(&scope.target_space, &scope.expected_identity_id)
            || self.forward_source(scope)?.1 != revision
        {
            return Err(stale());
        }
        if record.options != options {
            record.options = options;
            record.operation_id = types::new_id("forward")?;
            self.persist_forward(scope, &record)?;
        }
        self.forward_result(scope, record)
    }
}
