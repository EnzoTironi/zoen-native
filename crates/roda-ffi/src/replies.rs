//! Replies and threads (protocol: `roda_types::reply`).
//!
//! - `send_reply(space, text, to, thread: false)`: an inline reply. It stays in the main
//!   timeline and carries a quote of `to` (`TimelineEntry.reply_to`).
//! - `send_reply(space, text, to, thread: true)`: a reply in the thread under `to`. It is
//!   marked `in_thread`; the root counts it in `thread_replies`; `thread(space, root)`
//!   returns the root followed by its replies, oldest first.
//!
//! Threads are one level deep: replying in a thread to a thread reply re-points at the root.

use std::collections::HashMap;

use roda_types::{EventBody, ReplyRef};

use crate::dto::{ReplyQuote, TimelineEntry};
use crate::engine::{Engine, Entry, EntryBody, R};
use crate::i18n::t;
use crate::{CoreError, RodaEngine};

/// How much of the quoted message an inline reply carries to the UI.
const QUOTE_CHARS: usize = 140;

impl Engine {
    /// The quote (inline) or thread root (thread) of a timeline entry.
    pub(crate) fn reply_parts(&self, e: &Entry) -> (Option<ReplyQuote>, Option<String>) {
        let EntryBody::Message { reply: Some(r), .. } = &e.body else {
            return (None, None);
        };
        if r.thread {
            return (None, Some(r.to.clone()));
        }
        let quote = self.space_state(&e.space).ok().and_then(|s| {
            let target = s.entries.iter().find(|x| x.id() == r.to)?;
            Some(ReplyQuote {
                id: r.to.clone(),
                author: self.persona(&target.author),
                text: self.quote_text(target),
            })
        });
        (quote, None)
    }

    fn quote_text(&self, e: &Entry) -> String {
        let EntryBody::Message { text, attaches, .. } = &e.body else {
            return String::new();
        };
        let text = if text.is_empty() {
            attaches
                .as_deref()
                .and_then(|i| self.card(i))
                .map(|c| format!("{} · {}", c.kind_label, c.title))
                .unwrap_or_default()
        } else {
            text.clone()
        };
        if text.chars().count() > QUOTE_CHARS {
            let mut s: String = text.chars().take(QUOTE_CHARS).collect();
            s.push('…');
            s
        } else {
            text
        }
    }

    pub fn send_reply(
        &mut self,
        space: &str,
        text: &str,
        to: &str,
        thread: bool,
    ) -> R<TimelineEntry> {
        let me = self.me_id()?;
        self.reply_as(space, &me, text, to, thread)
    }

    pub(crate) fn reply_as(
        &mut self,
        space: &str,
        author: &str,
        text: &str,
        to: &str,
        thread: bool,
    ) -> R<TimelineEntry> {
        if !self.is_member(space, author) {
            return Err(CoreError::Forbidden {
                reason: t(
                    "só membros escrevem neste Espaço",
                    "only members can write in this Space",
                ),
            });
        }
        let text = text.trim();
        if text.is_empty() {
            return Err(CoreError::Invalid {
                reason: t("mensagem vazia", "empty message"),
            });
        }
        let s = self.space_state(space)?;
        let Some(target) = s.entries.iter().find(|e| e.id() == to) else {
            return Err(CoreError::NotFound {
                what: t("mensagem", "message"),
            });
        };
        let EntryBody::Message {
            reply: target_reply,
            ..
        } = &target.body
        else {
            return Err(CoreError::Invalid {
                reason: t(
                    "só dá para responder a mensagens",
                    "you can only reply to messages",
                ),
            });
        };
        // One level deep: a thread reply to a thread reply belongs to the same root.
        let to = match target_reply {
            Some(r) if thread && r.thread => r.to.clone(),
            _ => to.to_string(),
        };
        let reply = if thread {
            ReplyRef::thread(to)
        } else {
            ReplyRef::inline(to)
        };
        self.append(
            space,
            author,
            EventBody::MessagePosted {
                message: roda_types::new_id("msg"),
                text: text.to_string(),
                attaches: None,
                reply: Some(reply),
            },
        )?;
        let s = self.space_state(space)?;
        Ok(self.entry_dto(s.entries.last().expect("just appended")))
    }

    /// The thread under `root`: the root message, then its replies oldest first.
    pub fn thread(&self, space: &str, root: &str) -> R<Vec<TimelineEntry>> {
        let s = self.space_state(space)?;
        let Some(r) = s.entries.iter().find(|e| e.id() == root) else {
            return Err(CoreError::NotFound {
                what: t("mensagem", "message"),
            });
        };
        let mut out = vec![self.entry_dto(r)];
        out.extend(
            s.entries
                .iter()
                .filter(|e| matches!(&e.body, EntryBody::Message { reply: Some(x), .. } if x.thread && x.to == root))
                .map(|e| self.entry_dto(e)),
        );
        out[0].thread_replies = (out.len() - 1) as u32;
        Ok(out)
    }
}

/// Fills `thread_replies` on every root in a timeline.
pub(crate) fn count_thread_replies(entries: &mut [TimelineEntry]) {
    let mut counts: HashMap<String, u32> = HashMap::new();
    for e in entries.iter() {
        if let Some(root) = &e.in_thread {
            *counts.entry(root.clone()).or_default() += 1;
        }
    }
    if counts.is_empty() {
        return;
    }
    for e in entries.iter_mut() {
        if let Some(n) = counts.get(&e.id) {
            e.thread_replies = *n;
        }
    }
}

#[uniffi::export]
impl RodaEngine {
    /// Replies to a message: inline (`thread == false`, quoted in the chat) or in its
    /// thread (`thread == true`).
    pub fn send_reply(
        &self,
        space_id: String,
        text: String,
        to: String,
        thread: bool,
    ) -> Result<TimelineEntry, CoreError> {
        self.lock().send_reply(&space_id, &text, &to, thread)
    }

    /// A thread: the root message followed by its replies, oldest first.
    pub fn thread(
        &self,
        space_id: String,
        root_id: String,
    ) -> Result<Vec<TimelineEntry>, CoreError> {
        self.lock().thread(&space_id, &root_id)
    }
}
