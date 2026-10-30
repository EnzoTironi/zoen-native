//! Selected replies are signed snapshots, not another message or history store.
use crate::{
    engine::{Engine, EntryBody, State, R},
    CoreError,
};
use types::{Event, EventBody, SelectedQuote};

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct QuoteSpan {
    pub start_utf16: u32,
    pub end_utf16: u32,
}

/// Literal native selection. Half-open UTF-16 coordinates address canonical source;
/// syntax gaps are allowed, skipped visible text is not. Maximum 4096 text units /
/// 128 spans (Zoen bounds). Unsupported normalized/layout ranges are unavailable.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct ReplySelection {
    pub source_id: String,
    pub revision: String,
    pub spans: Vec<QuoteSpan>,
    pub text: String,
}
impl From<&SelectedQuote> for ReplySelection {
    fn from(q: &SelectedQuote) -> Self {
        Self {
            source_id: q.source_id.clone(),
            revision: q.revision.clone(),
            spans: q
                .spans
                .iter()
                .map(|s| QuoteSpan {
                    start_utf16: s.start_utf16,
                    end_utf16: s.end_utf16,
                })
                .collect(),
            text: q.text.clone(),
        }
    }
}
impl ReplySelection {
    pub(crate) fn signed(&self) -> SelectedQuote {
        SelectedQuote {
            source_id: self.source_id.clone(),
            revision: self.revision.clone(),
            spans: self
                .spans
                .iter()
                .map(|s| types::QuoteSpan {
                    start_utf16: s.start_utf16,
                    end_utf16: s.end_utf16,
                })
                .collect(),
            text: self.text.clone(),
        }
    }
}
fn invalid() -> CoreError {
    CoreError::Stale {
        reason: crate::i18n::t(
            "o trecho selecionado mudou ou não está disponível",
            "the selected text changed or is unavailable",
        ),
    }
}
fn matches(text: &str, quote: &SelectedQuote) -> bool {
    documents::message_quote::validate(
        text,
        &quote
            .spans
            .iter()
            .map(|s| (s.start_utf16, s.end_utf16))
            .collect::<Vec<_>>(),
        &quote.text,
    )
    .is_ok()
}
fn logical_id(event: &Event) -> &str {
    if event.client_id.is_empty() {
        &event.hash
    } else {
        &event.client_id
    }
}

impl Engine {
    pub(crate) fn review_reply_selection(
        &self,
        scope: &crate::message_drafts::DraftScope,
        selection: ReplySelection,
    ) -> R<ReplySelection> {
        self.validate_private_draft_scope(scope)?;
        if !self
            .state
            .writer(&scope.space_id, &scope.expected_identity_id)
        {
            return Err(CoreError::Forbidden {
                reason: crate::i18n::t("só autores respondem", "only writers can reply"),
            });
        }
        self.draft_target(scope, Some(&selection.source_id))?;
        self.validate_reply_selection(
            &scope.space_id,
            Some(&selection.source_id),
            Some(&selection),
        )?;
        Ok(selection)
    }
    pub(crate) fn validate_reply_selection(
        &self,
        space: &str,
        target: Option<&str>,
        selection: Option<&ReplySelection>,
    ) -> R<()> {
        let Some(q) = selection else {
            return Ok(());
        };
        if target != Some(q.source_id.as_str()) {
            return Err(invalid());
        }
        let source = self
            .space_state(space)?
            .entries
            .iter()
            .find(|e| e.id() == q.source_id)
            .ok_or_else(invalid)?;
        let EntryBody::Message { text, marks, .. } = &source.body else {
            return Err(invalid());
        };
        if marks.deleted
            || q.revision != marks.text_revision.as_deref().unwrap_or(source.id())
            || !matches(text, &q.signed())
        {
            return Err(invalid());
        }
        Ok(())
    }
}

/// Use the authenticated existing prefix to validate an old observed source even if
/// a concurrent edit was ordered before this reply. Never persist a parallel history.
pub(crate) fn received_valid(state: &State, event: &Event, prefix: &[Event]) -> bool {
    if let EventBody::MessagePosted { reply: Some(r), .. } = &event.body {
        if let Some(shared) = &r.shared {
            return shared.is_well_formed()
                && shared.space_id != event.space
                && !r.to.is_empty()
                && r.to.len() <= 256
                && !r.to.chars().any(char::is_control)
                && !r.thread
                && r.quote.is_none()
                && r.artifact.is_none()
                && state.writer(&event.space, &event.author);
        }
    }
    let (reply, quote) = match &event.body {
        EventBody::MessagePosted { reply: Some(r), .. } => match &r.quote {
            Some(q) => (r, q),
            None => return true,
        },
        EventBody::AppEditRequested { reply, .. } => {
            return reply.quote.is_none() && reply.shared.is_none()
        }
        _ => return true,
    };
    if reply.artifact.is_some() || !state.writer(&event.space, &event.author) {
        return false;
    }
    let Some(source) = state
        .spaces
        .get(&event.space)
        .and_then(|s| s.entries.iter().find(|e| e.id() == quote.source_id))
    else {
        return false;
    };
    let EntryBody::Message {
        text,
        marks,
        reply: source_reply,
        ..
    } = &source.body
    else {
        return false;
    };
    let expected_to = source_reply
        .as_ref()
        .filter(|r| reply.thread && r.thread)
        .map_or(source.id(), |r| r.to.as_str());
    if reply.to != expected_to {
        return false;
    }
    if !marks.deleted && quote.revision == marks.text_revision.as_deref().unwrap_or(source.id()) {
        return matches(text, quote);
    }
    let Some(original) = prefix.iter().find(|e| logical_id(e) == quote.source_id) else {
        return false;
    };
    let initial = match &original.body {
        EventBody::MessagePosted { text, .. } | EventBody::AppEditRequested { text, .. } => text,
        _ => return false,
    };
    if original.space != event.space || original.author != source.author {
        return false;
    }
    if quote.revision == event_log::content_hash_of(original) {
        return matches(initial, quote);
    }
    let mut deleted = false;
    for edit in prefix {
        if edit.space != event.space || edit.author != source.author {
            continue;
        }
        let EventBody::MessageAnnotated { target, note } = &edit.body else {
            continue;
        };
        if target != &quote.source_id || deleted {
            continue;
        }
        match note {
            types::Annotation::Delete => deleted = true,
            types::Annotation::Edit { text }
                if event_log::content_hash_of(edit) == quote.revision =>
            {
                return matches(text.trim(), quote)
            }
            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests;
