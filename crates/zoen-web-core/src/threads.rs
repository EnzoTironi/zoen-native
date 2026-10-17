//! Personal Space thread preferences share the native protocol and sequence watermarks.
use super::*;

impl<S: Signer> Client<S> {
    pub(super) fn thread_root(&self, space: &str, root: &str) -> String {
        self.logs
            .get(space)
            .and_then(|l| {
                l.events()
                    .iter()
                    .find(|e| e.hash == root || e.client_id == root)
            })
            .map(|e| {
                if e.client_id.is_empty() {
                    e.hash.clone()
                } else {
                    e.client_id.clone()
                }
            })
            .unwrap_or_else(|| root.into())
    }

    fn thread_state(&self, space: &str, root: &str) -> (Option<bool>, u64) {
        let canonical = |id: &str| self.thread_root(space, id);
        self.thread_states(space, &canonical)
            .get(&canonical(root))
            .copied()
            .unwrap_or((None, 0))
    }

    fn thread_states(
        &self,
        space: &str,
        canonical: &impl Fn(&str) -> String,
    ) -> BTreeMap<String, (Option<bool>, u64)> {
        let personal = |id: &str| {
            matches!(self.metadata(id), Some((_, SpaceKind::Personal, _)))
                && self.roster(id).contains_key(&self.account.identity)
        };
        let mut events: Vec<&Event> = self
            .logs
            .iter()
            .filter(|(id, _)| personal(id))
            .flat_map(|(_, log)| log.events())
            .chain(
                self.saved
                    .outbox
                    .values()
                    .filter(|pending| pending.failure.is_none())
                    .filter_map(|pending| pending.inner.as_ref())
                    .filter(|event| personal(&event.space)),
            )
            .filter(|event| event.author == self.account.identity)
            .collect();
        // Personal logs have independent sequence numbers. Match native's total
        // LWW order for follow, while reads only ever move forward.
        events.sort_by(|a, b| (a.at_ms, a.seq, &a.client_id).cmp(&(b.at_ms, b.seq, &b.client_id)));
        let mut states = BTreeMap::new();
        for event in events {
            if let EventBody::ThreadStateSet {
                space: s,
                root,
                follow,
                read_up_to,
            } = &event.body
            {
                if s == space {
                    let state = states.entry(canonical(root)).or_insert((None, 0u64));
                    *state = (*follow, state.1.max(*read_up_to));
                }
            }
        }
        states
    }

    fn put_thread_state(
        &mut self,
        space: &str,
        root: &str,
        follow: Option<bool>,
        read_up_to: u64,
        now: i64,
    ) -> Result<Update, String> {
        self.posted(space, root)
            .ok_or("That message is no longer available.")?;
        let personal = self
            .logs
            .keys()
            .find(|id| {
                matches!(self.metadata(id), Some((_, SpaceKind::Personal, _)))
                    && self.roster(id).contains_key(&self.account.identity)
            })
            .cloned()
            .ok_or("Personal Space is not available yet.")?;
        if !self.synced {
            return Err("Catching up with the conversation…".into());
        }
        if let Some(error) = self.saved.integrity_errors.get(&personal) {
            return Err(error.clone());
        }
        // Preferences follow the same encrypted Personal contract as native.
        let epoch = self.ready_epoch(&personal)?;
        let pending = self.clear_event(
            &personal,
            EventBody::ThreadStateSet {
                space: space.into(),
                root: self.thread_root(space, root),
                follow,
                read_up_to,
            },
            now,
        )?;
        // Keep the exact signed event for optimistic projection and retries.
        let inner = pending
            .envelope()?
            .plain_event()
            .ok_or("invalid thread state")?;
        let pending = self.seal_inner(&inner, epoch)?;
        self.saved.outbox.insert(inner.client_id.clone(), pending);
        self.update(Vec::new())
    }

    pub fn set_thread_follow(
        &mut self,
        space: &str,
        root: &str,
        follow: Option<bool>,
        now: i64,
    ) -> Result<Update, String> {
        let read = self.thread_state(space, root).1;
        self.put_thread_state(space, root, follow, read, now)
    }

    pub fn mark_thread_read(
        &mut self,
        space: &str,
        root: &str,
        now: i64,
    ) -> Result<Update, String> {
        let canonical = self.thread_root(space, root);
        let last = self.logs.get(space).ok_or("That message is no longer available.")?.events().iter()
            .filter(|e| matches!(&e.body, EventBody::MessagePosted { reply: Some(r), .. } if r.thread && self.thread_root(space, &r.to) == canonical))
            .map(|e| e.seq).max().unwrap_or(0);
        let (follow, read) = self.thread_state(space, root);
        if last <= read {
            return self.update(Vec::new());
        }
        self.put_thread_state(space, root, follow, last, now)
    }

    pub(super) fn fill_threads(&self, space: &str, messages: &mut [MessageView]) {
        let aliases: BTreeMap<&str, &str> = self
            .logs
            .get(space)
            .into_iter()
            .flat_map(|log| log.events())
            .flat_map(|e| {
                let canonical = if e.client_id.is_empty() {
                    e.hash.as_str()
                } else {
                    e.client_id.as_str()
                };
                [
                    (e.hash.as_str(), canonical),
                    (e.client_id.as_str(), canonical),
                ]
            })
            .collect();
        let canonical = |id: &str| aliases.get(id).copied().unwrap_or(id).to_string();
        let mut by_root: BTreeMap<String, Vec<&MessageView>> = BTreeMap::new();
        for message in messages.iter() {
            if let Some(root) = &message.thread {
                by_root.entry(canonical(root)).or_default().push(message);
            }
        }
        let preferences = self.thread_states(space, &canonical);
        let states: BTreeMap<String, (bool, usize, usize)> = messages
            .iter()
            .filter_map(|root| {
                let entry = root.entry.as_ref()?;
                let id = canonical(entry);
                let replies = by_root.get(&id).map(Vec::as_slice).unwrap_or_default();
                let (follow, read) = preferences.get(&id).copied().unwrap_or((None, 0));
                let following = follow.unwrap_or_else(|| {
                    root.author == self.account.identity
                        || (!root.deleted && mentions(&root.text, &self.account.handle))
                        || replies.iter().any(|m| {
                            m.author == self.account.identity
                                || (!m.deleted && mentions(&m.text, &self.account.handle))
                        })
                });
                let fresh: Vec<_> = replies
                    .iter()
                    .filter(|m| {
                        m.author != self.account.identity && m.seq.is_some_and(|s| s > read)
                    })
                    .collect();
                let unread = fresh.len();
                let mention_count = fresh
                    .iter()
                    .filter(|m| !m.deleted && mentions(&m.text, &self.account.handle))
                    .count();
                Some((entry.clone(), (following, unread, mention_count)))
            })
            .collect();
        for m in messages {
            if let Some((following, unread, mention_count)) =
                m.entry.as_ref().and_then(|id| states.get(id))
            {
                m.thread_following = *following;
                m.thread_unread = *unread;
                m.thread_mentions = *mention_count;
            }
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "thread_tests.rs"]
mod tests;

#[test]
fn mention_suffix_dot_is_part_of_a_handle() {
    assert!(!mentions("@fixture.other", "fixture"));
    assert!(mentions("@fixture.other", "fixture.other"));
    assert!(mentions("Hi @fixture!", "fixture"));
}

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
