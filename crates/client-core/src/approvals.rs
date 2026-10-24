use std::collections::HashMap;

use event_log::content_hash;
use grants::{standing_allow_permitted, standing_key};
use types::{
    new_id, ActionClass, AgentRequest, Capability, Event, EventBody, Grant, GrantScope, Identity,
    IdentityId, IdentityKind, ItemContent, ItemKind, ItemProposal, PageContent, PageManifest,
    PlanDoc, RequestResolution, Role, ToolDecision, TrustLevel,
};

use crate::dto::{ApproveOutcome, DecisionKind, PlanOutcome};
use crate::engine::{
    Engine, Entry, EntryBody, GrantState, ItemState, ReqStatus, RequestState, State, Version,
};
use crate::i18n::t;
use crate::pages::{BeforeEvent, Currentness, PlanRead, Projection};
use crate::{tr, CoreError};

type R<T> = Result<T, CoreError>;

fn content_matches(kind: ItemKind, content: &ItemContent) -> bool {
    matches!(
        (kind, content),
        (ItemKind::Plan, ItemContent::Plan(_))
            | (ItemKind::Task | ItemKind::Note, ItemContent::Text { .. })
            | (ItemKind::App, ItemContent::App(_))
            | (ItemKind::Page, ItemContent::Page(_))
            | (ItemKind::File, ItemContent::File(_))
    )
}

impl ItemState {
    pub(crate) fn voice_edit_allowed(&self, author: &str, content: Option<&ItemContent>) -> bool {
        // Keep ownership after a rename or MIME change hides the voice tag, and when
        // an ordinary file is being turned into a voice note.
        !self
            .versions
            .iter()
            .map(|v| &v.content)
            .chain(content)
            .any(|c| matches!(c, ItemContent::File(f) if f.is_voice()))
            || self.versions.first().is_some_and(|v| v.author == author)
    }
}

#[cfg(test)]
mod tests;

mod currentness;
mod decisions;
mod projection;
