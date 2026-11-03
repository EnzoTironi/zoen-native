//! Media custody borrows the same canonical reply contract as the text composer.
use super::*;
use crate::{
    replies::external::{DraftReplyContext, SharedReplyDraft},
    ReplySelection,
};

#[derive(PartialEq, Eq)]
pub(super) struct Reply<'a> {
    pub to: Option<&'a str>,
    pub selection: Option<&'a ReplySelection>,
    pub shared: Option<&'a SharedReplyDraft>,
    pub intent: Option<&'a str>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AcceptedReply {
    reply_to: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reply_quote: Option<ReplySelection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    shared_reply: Option<SharedReplyDraft>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reply_intent: Option<String>,
}
impl AcceptedReply {
    pub(super) fn valid_shape(&self) -> bool {
        Reply {
            to: self.reply_to.as_deref(),
            selection: self.reply_quote.as_ref(),
            shared: self.shared_reply.as_ref(),
            intent: self.reply_intent.as_deref(),
        }
        .valid_shape()
    }
}
impl Content {
    pub(super) fn reply(&self) -> Reply<'_> {
        let (to, selection, shared, intent) = match self {
            Self::Attachments {
                reply_to,
                reply_quote,
                shared_reply,
                reply_intent,
                ..
            }
            | Self::Voice {
                reply_to,
                reply_quote,
                shared_reply,
                reply_intent,
                ..
            }
            | Self::Poll {
                reply_to,
                reply_quote,
                shared_reply,
                reply_intent,
                ..
            } => (reply_to, reply_quote, shared_reply, reply_intent),
        };
        Reply {
            to: to.as_deref(),
            selection: selection.as_ref(),
            shared: shared.as_ref(),
            intent: intent.as_deref(),
        }
    }
}
impl Reply<'_> {
    pub(super) fn accepted(&self) -> AcceptedReply {
        AcceptedReply {
            reply_to: self.to.map(str::to_owned),
            reply_quote: self.selection.cloned(),
            shared_reply: self.shared.cloned(),
            reply_intent: self.intent.map(str::to_owned),
        }
    }
    pub(super) fn context<'a>(&'a self, scope: &'a DraftScope) -> DraftReplyContext<'a> {
        DraftReplyContext {
            scope,
            selection: self.selection,
            shared: self.shared,
        }
    }
    pub(super) fn valid_shape(&self) -> bool {
        if self.to.is_some_and(|id| !target_id(id))
            || self.intent.is_some_and(|id| !intent_id(id) || id == "0")
            || (self.intent.is_some() && self.to.is_none() && self.shared.is_none())
        {
            return false;
        }
        if let Some(shared) = self.shared {
            return self.to.is_none() && self.selection.is_none() && shared.well_formed();
        }
        self.selection.is_none_or(|quote| {
            self.to == Some(quote.source_id.as_str()) && quote.signed().is_well_formed()
        })
    }
}
