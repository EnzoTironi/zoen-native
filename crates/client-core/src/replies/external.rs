//! A quotation deliberately shared between conversations. Only the sending account
//! verifies the source. Recipients authenticate the sender, not inaccessible history.
use crate::{
    engine::{Engine, EntryBody, R},
    message_drafts::DraftScope,
    CoreError, ReplyQuote, ReplySelection, TimelineEntry,
};

#[derive(Debug, Clone, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub(crate) struct SharedReplyReview {
    pub expected_source_revision: String,
    pub source_space_id: String,
    pub message_id: String,
    #[serde(default)]
    #[cfg_attr(feature = "client-types", ts(optional))]
    pub selection: Option<ReplySelection>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct SharedReplyDraft {
    pub space_id: String,
    pub message_id: String,
    pub revision: String,
    pub author_id: String,
    pub author_name: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "client-types", ts(optional))]
    pub selection: Option<ReplySelection>,
}
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct ReplyOrigin {
    pub space_id: String,
    pub available: bool,
}
impl SharedReplyDraft {
    fn signed(&self) -> types::SharedReply {
        types::SharedReply {
            space_id: self.space_id.clone(),
            revision: self.revision.clone(),
            author_id: self.author_id.clone(),
            author_name: self.author_name.clone(),
            text: self.text.clone(),
            selection: self.selection.as_ref().map(ReplySelection::signed),
        }
    }
}
fn unavailable() -> CoreError {
    CoreError::Stale {
        reason: crate::i18n::t(
            "a mensagem original ou o acesso à conversa mudou",
            "the original message or conversation access changed",
        ),
    }
}
impl Engine {
    pub(crate) fn review_shared_reply(
        &self,
        scope: &DraftScope,
        source_space: &str,
        message_id: &str,
        selection: Option<&ReplySelection>,
    ) -> R<SharedReplyDraft> {
        self.validate_private_draft_scope(scope)?;
        if scope.thread_root_id.is_some()
            || source_space == scope.space_id
            || !self
                .state
                .writer(&scope.space_id, &scope.expected_identity_id)
            || !self.is_member(source_space, &scope.expected_identity_id)
        {
            return Err(unavailable());
        }
        let source = self
            .space_state(source_space)?
            .entries
            .iter()
            .find(|entry| entry.id() == message_id)
            .ok_or_else(unavailable)?;
        let EntryBody::Message { marks, .. } = &source.body else {
            return Err(unavailable());
        };
        if marks.deleted {
            return Err(unavailable());
        }
        self.validate_reply_selection(source_space, Some(message_id), selection)?;
        let quote = SharedReplyDraft {
            space_id: source_space.into(),
            message_id: source.id().into(),
            revision: marks.text_revision.as_deref().unwrap_or(source.id()).into(),
            author_id: source.author.clone(),
            author_name: self.persona(&source.author).name,
            text: selection.map_or_else(|| self.quote_text(source), |quote| quote.text.clone()),
            selection: selection.cloned(),
        };
        if !quote.signed().is_well_formed() {
            return Err(unavailable());
        }
        Ok(quote)
    }
    pub(crate) fn validate_shared_reply(
        &self,
        scope: &DraftScope,
        quote: &SharedReplyDraft,
    ) -> R<()> {
        if self.review_shared_reply(
            scope,
            &quote.space_id,
            &quote.message_id,
            quote.selection.as_ref(),
        )? != *quote
        {
            return Err(unavailable());
        }
        Ok(())
    }
    pub(crate) fn send_shared_reply(
        &mut self,
        scope: &DraftScope,
        text: &str,
        quote: &SharedReplyDraft,
    ) -> R<TimelineEntry> {
        self.validate_shared_reply(scope, quote)?;
        if text.trim().is_empty() {
            return Err(unavailable());
        }
        let mut reference = types::ReplyRef::inline(&quote.message_id);
        reference.shared = Some(Box::new(quote.signed()));
        let event = self.append(
            &scope.space_id,
            &scope.expected_identity_id,
            types::EventBody::MessagePosted {
                message: types::new_id("msg")?,
                text: text.trim().into(),
                attaches: None,
                reply: Some(reference),
                media: None,
                forwarded: false,
            },
        )?;
        let entry = self
            .space_state(&scope.space_id)?
            .entries
            .iter()
            .find(|entry| entry.id() == event.client_id)
            .ok_or_else(unavailable)?;
        Ok(self.entry_dto(entry))
    }
    pub(crate) fn shared_reply_quote(
        &self,
        message_id: &str,
        shared: &types::SharedReply,
    ) -> ReplyQuote {
        let mut author = self.persona(&shared.author_id);
        author.name.clone_from(&shared.author_name);
        let available = self
            .me
            .as_deref()
            .is_some_and(|me| self.is_member(&shared.space_id, me))
            && self.space_state(&shared.space_id).ok().is_some_and(|s| {
                s.entries.iter().any(|e| {
                    e.id() == message_id
                        && matches!(&e.body, EntryBody::Message { marks, .. } if !marks.deleted)
                })
            });
        ReplyQuote {
            id: message_id.into(),
            author,
            text: shared.text.clone(),
            selection: shared.selection.as_ref().map(ReplySelection::from),
            origin: Some(ReplyOrigin {
                space_id: shared.space_id.clone(),
                available,
            }),
        }
    }
}
