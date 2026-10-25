//! Replies and threads (protocol: `types::reply`).
//!
//! - `send_reply(space, text, to, thread: false)`: an inline reply. It stays in the main
//!   timeline and carries a quote of `to` (`TimelineEntry.reply_to`).
//! - `send_reply(space, text, to, thread: true)`: a reply in the thread under `to`. It is
//!   marked `in_thread`; the root counts it in `thread_replies`; `thread(space, root)`
//!   returns the root followed by its replies, oldest first.
//!
//! Threads are one level deep: replying in a thread to a thread reply re-points at the root.

use std::collections::HashMap;

use types::{EventBody, ReplyRef};

use crate::dto::{ReplyQuote, TimelineEntry};
use crate::engine::{Engine, Entry, EntryBody, R};
use crate::i18n::t;
use crate::{Client, CoreError};

/// How much of the quoted message an inline reply carries to the UI.
const QUOTE_CHARS: usize = 140;

/// A read of the existing artifact thread, never another comments store.
#[derive(serde::Serialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub(crate) struct ItemDiscussionDto {
    pub item_id: String,
    pub space_id: String,
    pub root: Option<TimelineEntry>,
}

impl Engine {
    pub(crate) fn item_discussion(&self, space: &str, item: &str) -> R<ItemDiscussionDto> {
        let me = self.me_id()?;
        if !self.is_member(space, &me) {
            return Err(CoreError::Forbidden {
                reason: t(
                    "só membros leem esta discussão",
                    "only members can read this discussion",
                ),
            });
        }
        let it = self.item_state(item)?;
        if it.space != space
            || self.state.is_recovery_item(item)
            || self.card_in_space(space, item).is_none()
        {
            return Err(CoreError::NotFound {
                what: "Item".into(),
            });
        }
        let entries = &self.space_state(space)?.entries;
        let root = entries
            .iter()
            .find(|entry| {
                matches!(
                    &entry.body,
                    EntryBody::Message { attaches: Some(id), reply: None, marks, .. }
                        if id == item && !marks.forwarded
                )
            })
            .map(|entry| {
                let mut root = self.entry_dto(entry);
                root.thread_replies = entries
                    .iter()
                    .filter(|reply| {
                        matches!(
                            &reply.body,
                            EntryBody::Message { reply: Some(reference), .. }
                                if reference.thread && reference.to == root.id
                        )
                    })
                    .count() as u32;
                root
            })
            .or_else(|| self.shared_app_root(space, item));
        Ok(ItemDiscussionDto {
            item_id: item.to_owned(),
            space_id: space.to_owned(),
            root,
        })
    }
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
        self.reply_with(space, author, text, None, to, thread)
    }

    /// A reply that may carry an Item (a voice note) instead of text.
    pub(crate) fn reply_with(
        &mut self,
        space: &str,
        author: &str,
        text: &str,
        attaches: Option<String>,
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
        if text.is_empty() && attaches.is_none() {
            return Err(CoreError::Invalid {
                reason: t("mensagem vazia", "empty message"),
            });
        }
        let reply = self.reply_ref(space, to, thread)?;
        self.append(
            space,
            author,
            EventBody::MessagePosted {
                message: types::new_id("msg")?,
                text: text.to_string(),
                attaches,
                reply: Some(reply),
                media: None,
                forwarded: false,
            },
        )?;
        let s = self.space_state(space)?;
        Ok(self.entry_dto(s.entries.last().expect("just appended")))
    }

    /// Validate before storing a voice file, so a bad target cannot post it to the main chat.
    pub(crate) fn reply_ref(&self, space: &str, to: &str, thread: bool) -> R<ReplyRef> {
        let s = self.space_state(space)?;
        let Some(target) = s.entries.iter().find(|e| e.id() == to) else {
            if self.shared_app_root_by_anchor(space, to).is_some() {
                return Ok(if thread {
                    ReplyRef::thread(to)
                } else {
                    ReplyRef::inline(to)
                });
            }
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
        Ok(if thread {
            ReplyRef::thread(to)
        } else {
            ReplyRef::inline(to)
        })
    }

    /// The thread under `root`: the root message, then its replies oldest first.
    pub fn thread(&self, space: &str, root: &str) -> R<Vec<TimelineEntry>> {
        let s = self.space_state(space)?;
        let r = s
            .entries
            .iter()
            .find(|e| e.id() == root)
            .map(|r| self.entry_dto(r))
            .or_else(|| self.shared_app_root_by_anchor(space, root))
            .ok_or_else(|| CoreError::NotFound {
                what: t("mensagem", "message"),
            })?;
        let mut out = vec![r];
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
impl Client {
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

#[cfg(test)]
mod discussion_tests;
