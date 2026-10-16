//! Threads, Slack/Ando style, on top of `roda_types::reply`.
//!
//! - Following: automatic when I started the thread, replied in it or was @mentioned in it;
//!   an explicit follow/unfollow (synced through my Personal Space) overrides that.
//! - Read state is per thread ("read up to" its last reply I saw), synced the same way, so
//!   a thread read on the phone is read on the Mac.
//! - A thread reply stays out of the chat's unread count unless it was also sent to the
//!   chat; replies in threads I follow count in `threads_unread` and the Threads inbox.
//! - Mentions are their own state with their own count, apart from unread.

use roda_types::{EventBody, SpaceKind};

use crate::dto::{ThreadSummary, TimelineEntry};
use crate::engine::{Engine, Entry, EntryBody, R};
use crate::i18n::t;
use crate::{CoreError, RodaEngine};

/// A reply that lives only in its thread (not also sent to the chat).
pub(crate) fn thread_only(e: &Entry) -> bool {
    matches!(&e.body, EntryBody::Message { reply: Some(r), .. } if r.thread && !r.also_in_chat)
}

fn root_of(e: &Entry) -> Option<&str> {
    match &e.body {
        EntryBody::Message { reply: Some(r), .. } if r.thread => Some(r.to.as_str()),
        _ => None,
    }
}

/// `@handle` as a whole word (letters, digits, `_` and `.` continue a handle).
fn mentions(text: &str, handle: &str) -> bool {
    if handle.is_empty() {
        return false;
    }
    let lower = text.to_lowercase();
    let needle = format!("@{}", handle.to_lowercase());
    lower.match_indices(&needle).any(|(i, _)| {
        let before = lower[..i].chars().next_back();
        let after = lower[i + needle.len()..].chars().next();
        !before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.')
            && !after.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.')
    })
}

impl Engine {
    pub(crate) fn mentions_me(&self, e: &Entry) -> bool {
        let Some(me) = self.me.as_deref() else {
            return false;
        };
        let handle = self
            .identities
            .get(me)
            .map(|i| i.handle.as_str())
            .unwrap_or("");
        match &e.body {
            EntryBody::Message { text, marks, .. } => !marks.deleted && mentions(text, handle),
            _ => false,
        }
    }

    fn replies_of<'a>(&self, entries: &'a [Entry], root: &str) -> Vec<&'a Entry> {
        entries
            .iter()
            .filter(|e| root_of(e) == Some(root))
            .collect()
    }

    fn thread_read(&self, space: &str, root: &str) -> u64 {
        let synced = self
            .state
            .thread_state
            .get(&(space.to_string(), root.to_string()))
            .map(|s| s.1)
            .unwrap_or(0);
        let here = self
            .store
            .meta(&format!("tread:{space}:{root}"))
            .ok()
            .flatten()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        synced.max(here)
    }

    fn following(&self, space: &str, root: &Entry, replies: &[&Entry]) -> bool {
        let key = (space.to_string(), root.id().to_string());
        if let Some(explicit) = self.state.thread_state.get(&key).and_then(|s| s.0) {
            return explicit;
        }
        let me = self.me.as_deref().unwrap_or_default();
        root.author == me
            || self.mentions_me(root)
            || replies
                .iter()
                .any(|r| r.author == me || self.mentions_me(r))
    }

    /// (unread, unread mentions) of one thread.
    fn thread_unread(&self, space: &str, root: &str, replies: &[&Entry]) -> (u32, u32) {
        let me = self.me.as_deref().unwrap_or_default();
        let read = self.thread_read(space, root);
        let fresh: Vec<_> = replies
            .iter()
            .filter(|r| r.author != me && r.seq > read)
            .collect();
        let mentions = fresh.iter().filter(|r| self.mentions_me(r)).count();
        (fresh.len() as u32, mentions as u32)
    }

    /// Fills the thread_* fields of every root in a timeline.
    pub(crate) fn fill_thread_roots(&self, space: &str, out: &mut [TimelineEntry]) {
        let Ok(s) = self.space_state(space) else {
            return;
        };
        for dto in out.iter_mut().filter(|d| d.thread_replies > 0) {
            let Some(root) = s.entries.iter().find(|e| e.id() == dto.id) else {
                continue;
            };
            let replies = self.replies_of(&s.entries, &dto.id);
            let mut people: Vec<String> = Vec::new();
            for r in &replies {
                if !people.contains(&r.author) {
                    people.push(r.author.clone());
                }
            }
            dto.thread_participants = people.iter().map(|p| self.persona(p)).collect();
            dto.thread_last_reply_ms = replies.iter().map(|r| r.at_ms).max().unwrap_or(0);
            dto.thread_following = self.following(space, root, &replies);
            dto.thread_unread = self.thread_unread(space, &dto.id, &replies).0;
        }
    }

    /// Unread replies across the threads I follow in a chat.
    pub(crate) fn threads_unread(&self, space: &str) -> u32 {
        self.followed_threads(space)
            .iter()
            .map(|(_, _, unread, _)| *unread)
            .sum()
    }

    /// (root, replies, unread, mentions) for every thread I follow in a chat.
    fn followed_threads(&self, space: &str) -> Vec<(&Entry, Vec<&Entry>, u32, u32)> {
        let Ok(s) = self.space_state(space) else {
            return Vec::new();
        };
        let mut roots: Vec<&str> = Vec::new();
        for e in &s.entries {
            if let Some(r) = root_of(e) {
                if !roots.contains(&r) {
                    roots.push(r);
                }
            }
        }
        roots
            .into_iter()
            .filter_map(|id| {
                let root = s.entries.iter().find(|e| e.id() == id)?;
                let replies = self.replies_of(&s.entries, id);
                self.following(space, root, &replies).then(|| {
                    let (unread, mentions) = self.thread_unread(space, id, &replies);
                    (root, replies, unread, mentions)
                })
            })
            .collect()
    }

    /// The Threads inbox: threads I follow, newest reply first.
    pub fn threads(&self) -> Vec<ThreadSummary> {
        let mut out = Vec::new();
        for space in &self.space_order {
            let Some(st) = self.state.spaces.get(space) else {
                continue;
            };
            if st.kind == SpaceKind::Personal {
                continue;
            }
            for (root, replies, _, mentions) in self.followed_threads(space) {
                let mut dto = vec![self.entry_dto(root)];
                dto[0].thread_replies = replies.len() as u32;
                self.fill_thread_roots(space, &mut dto);
                out.push(ThreadSummary {
                    space_id: space.clone(),
                    space_title: self
                        .space_summary(space)
                        .map(|s| s.title)
                        .unwrap_or_else(|_| st.title.clone()),
                    root: dto.remove(0),
                    last_reply: replies.last().map(|r| self.entry_dto(r)),
                    mentions,
                });
            }
        }
        out.sort_by_key(|t| std::cmp::Reverse(t.root.thread_last_reply_ms));
        out
    }

    fn put_thread_state(
        &mut self,
        space: &str,
        root: &str,
        follow: Option<bool>,
        read: u64,
    ) -> R<()> {
        let me = self.me_id()?;
        let personal = self
            .personal_space()
            .ok_or_else(|| crate::engine::not_found(&t("Espaço pessoal", "Personal Space")))?;
        self.append(
            &personal,
            &me,
            EventBody::ThreadStateSet {
                space: space.to_string(),
                root: root.to_string(),
                follow,
                read_up_to: read,
            },
        )?;
        Ok(())
    }

    /// Follow (`Some(true)`), unfollow (`Some(false)`) or back to automatic (`None`).
    pub fn set_thread_follow(&mut self, space: &str, root: &str, follow: Option<bool>) -> R<()> {
        self.space_state(space)?;
        let read = self.thread_read(space, root);
        self.put_thread_state(space, root, follow, read)
    }

    /// I saw the thread up to its last reply. Syncs to my devices only when it moved past
    /// something unread, so opening a quiet thread writes nothing.
    pub fn mark_thread_read(&mut self, space: &str, root: &str) -> R<()> {
        let s = self.space_state(space)?;
        let replies = self.replies_of(&s.entries, root);
        let Some(last) = replies.iter().map(|r| r.seq).max() else {
            return Ok(());
        };
        let (unread, _) = self.thread_unread(space, root, &replies);
        if last <= self.thread_read(space, root) {
            return Ok(());
        }
        // Publish the synced watermark first. If appending fails, a local read must not
        // hide the unread replies and prevent the next attempt from syncing them.
        if unread > 0 {
            let follow = self
                .state
                .thread_state
                .get(&(space.to_string(), root.to_string()))
                .and_then(|s| s.0);
            self.put_thread_state(space, root, follow, last)?;
        }
        self.store
            .set_meta(&format!("tread:{space}:{root}"), &last.to_string())?;
        Ok(())
    }

    /// A reply in the thread under `root`, optionally also shown in the chat. Replying
    /// follows the thread (automatic follow), unless I explicitly unfollowed it.
    pub fn send_thread_reply(
        &mut self,
        space: &str,
        text: &str,
        root: &str,
        also_in_chat: bool,
    ) -> R<TimelineEntry> {
        let me = self.me_id()?;
        self.reply_with_chat(space, &me, text, None, root, true, also_in_chat)
    }
}

#[uniffi::export]
impl RodaEngine {
    /// The Threads inbox: threads I follow across my chats, newest reply first.
    pub fn threads(&self) -> Vec<ThreadSummary> {
        self.lock().threads()
    }

    /// Follow (`Some(true)`), unfollow (`Some(false)`) or automatic (`None`).
    pub fn set_thread_follow(
        &self,
        space_id: String,
        root_id: String,
        follow: Option<bool>,
    ) -> Result<(), CoreError> {
        self.lock().set_thread_follow(&space_id, &root_id, follow)
    }

    pub fn mark_thread_read(&self, space_id: String, root_id: String) -> Result<(), CoreError> {
        self.lock().mark_thread_read(&space_id, &root_id)
    }

    /// A thread reply; `also_in_chat` shows it in the chat too (Slack's checkbox).
    pub fn send_thread_reply(
        &self,
        space_id: String,
        text: String,
        root_id: String,
        also_in_chat: bool,
    ) -> Result<TimelineEntry, CoreError> {
        self.lock()
            .send_thread_reply(&space_id, &text, &root_id, also_in_chat)
    }
}

#[cfg(test)]
mod tests {
    use super::mentions;

    #[test]
    fn a_mention_is_a_whole_handle() {
        assert!(mentions("oi @Enzo, tudo?", "enzo"));
        assert!(mentions("@enzo", "enzo"));
        assert!(!mentions("@enzofilho", "enzo"));
        assert!(!mentions("mail@enzo.com", "enzo"));
        assert!(!mentions("oi @enzo.silva", "enzo"));
        assert!(mentions("oi @enzo.silva!", "enzo.silva"));
    }
}
