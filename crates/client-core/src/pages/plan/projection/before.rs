//! Event-bound accessors; construction stays with the authenticated replay context.
use super::*;

impl BeforeEvent {
    pub(crate) fn selected_quote_valid(&self, event: &Event) -> bool {
        self.selected_quote_valid
            && match &event.body {
                EventBody::MessagePosted {
                    reply: Some(reply), ..
                }
                | EventBody::AppEditRequested { reply, .. }
                    if reply.quote.is_some() =>
                {
                    self.matches(event)
                }
                _ => true,
            }
    }
    pub(crate) fn share_head(&self) -> Option<&crate::app_current_share::SharedVersionInfo> {
        self.share_head.as_ref()
    }
    pub(crate) fn app_share_refs_valid(&self) -> bool {
        self.app_share_refs_valid
    }
    pub(crate) fn scope_valid(&self) -> bool {
        self.scope_valid
    }

    pub(crate) fn matches(&self, event: &Event) -> bool {
        binding(event).is_ok_and(|hash| self.event == hash)
            && self
                .ordered_address
                .as_ref()
                .is_none_or(|(seq, prev, hash)| {
                    *seq == event.seq && prev == &event.prev && hash == &event.hash
                })
    }

    pub(crate) fn plan(&self) -> &PlanRead {
        &self.plan
    }

    pub(crate) fn ordered(&self) -> bool {
        !matches!(self.mode, Mode::Pending)
    }

    pub(crate) fn creation_origin(&self, event: &Event, item: &str) -> PlanOrigin {
        match self.mode {
            Mode::Confirmed | Mode::Local => {
                PlanOrigin::Explicit(source(&event.space, item, event, true).event_id)
            }
            Mode::Pending => PlanOrigin::Pending,
        }
    }
}
