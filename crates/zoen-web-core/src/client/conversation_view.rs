//! Whole operations on the existing Client; no additional state owner.
use super::*;

impl<S: Signer> Client<S> {
    // Shared ownership-checked manifest fold for cards and attachment posts.
    pub(super) fn folded_files<'a>(
        &'a self,
        space: &str,
    ) -> BTreeMap<&'a str, (&'a str, Vec<&'a types::FileDoc>)> {
        let events = self
            .logs
            .get(space)
            .into_iter()
            .flat_map(|log| log.events().iter())
            .chain(
                self.saved
                    .outbox
                    .values()
                    .filter_map(|pending| pending.inner.as_ref())
                    .filter(|event| event.space == space),
            );
        let mut files: BTreeMap<&str, (&str, Vec<&types::FileDoc>)> = BTreeMap::new();
        for event in events {
            match &event.body {
                EventBody::ItemCreated {
                    item,
                    content: types::ItemContent::File(doc),
                    ..
                } => {
                    files
                        .entry(item)
                        .or_insert((event.author.as_str(), vec![doc]));
                }
                EventBody::ItemVersioned {
                    item,
                    content: types::ItemContent::File(doc),
                    ..
                } => {
                    if let Some((author, versions)) = files.get_mut(item.as_str()) {
                        // Like native, any voice version keeps the original author's ownership.
                        if *author == event.author
                            || !versions
                                .iter()
                                .copied()
                                .chain([doc])
                                .any(types::FileDoc::is_voice)
                        {
                            versions.push(doc);
                        }
                    }
                }
                EventBody::ItemReverted {
                    item, to_version, ..
                } => {
                    if let Some((author, versions)) = files.get_mut(item.as_str()) {
                        if *author != event.author && versions.iter().any(|doc| doc.is_voice()) {
                            continue;
                        }
                        if let Some(doc) = to_version
                            .checked_sub(1)
                            .and_then(|v| versions.get(v as usize))
                            .copied()
                        {
                            versions.push(doc);
                        }
                    }
                }
                _ => {}
            }
        }
        files
    }

    pub(super) fn conversation_profile(&self, id: &str) -> Option<ConversationProfile> {
        let identity = self.saved.profiles.get(id)?.clone();
        let photo = self.fields_of(id).and_then(|(_, fields)| fields.photo);
        Some(ConversationProfile {
            photo_sha256: photo.as_ref().map(|photo| photo.sha256.clone()),
            photo_ready: photo
                .as_ref()
                .is_some_and(|photo| self.profile_photo_cached(photo)),
            photo_mime: photo.as_ref().map(|photo| photo.mime.clone()),
            identity,
        })
    }

    pub fn conversations(&self) -> Vec<ConversationView> {
        let preferences = self.chat_preferences();
        // The existing document fold enforces author roles, item ownership and valid
        // restores. Build one disposable index; never infer activity from note text.
        let (item_edits, item_projection_error, media_items) = match self.document_items() {
            Ok(items) => (documents::item_edits(&items), None, items),
            Err(reason) => (BTreeMap::new(), Some(reason), BTreeMap::new()),
        };
        let mut conversations = self.logs.keys().filter_map(|space| {
            let (title, kind, privacy) = self.metadata(space)?;
            let roster = self.roster(space);
            if kind == SpaceKind::Personal || !roster.contains_key(&self.account.identity) { return None; }
            let events = self.logs.get(space)?.events();
            let me = &self.account.identity;
            let files = self.folded_files(space);
            let accepted_media = self.received_media_posts(space, &media_items);
            // A direct chat is named after the other person, like every messenger.
            let title = match roster.keys().find(|m| *m != me) {
                Some(other) if kind == SpaceKind::Direct => self.saved.profiles.get(other)
                    .map(|p| if p.name.is_empty() { format!("@{}", p.handle) } else { p.name.clone() })
                    .unwrap_or_else(|| if title.is_empty() { "Conversa".into() } else { title.clone() }),
                _ => title,
            };
            // Fold annotations over their targets. Readers enforce authorship.
            let mut authors: BTreeMap<&str, (&str, &str)> = events.iter().filter(|e| is_post(&e.body))
                .map(|e| (e.hash.as_str(), (e.author.as_str(), e.hash.as_str()))).collect();
            for event in events.iter().filter(|e| is_post(&e.body)) {
                authors.entry(event.client_id.as_str()).or_insert((event.author.as_str(), event.hash.as_str()));
            }
            let mut folded: BTreeMap<String, Folded> = BTreeMap::new();
            for event in events {
                let EventBody::MessageAnnotated { target, note } = &event.body else { continue };
                let Some((author, hash)) = authors.get(target.as_str()) else { continue };
                if note.author_only() && *author != event.author { continue; }
                let f = folded.entry((*hash).to_owned()).or_default();
                match note {
                    types::Annotation::Edit { text } if !f.deleted => { f.text = Some(text.clone()); f.text_revision = Some(event_log::content_hash_of(event)); },
                    types::Annotation::Delete => { f.deleted = true; f.text = None; f.reactions.clear(); }
                    types::Annotation::React { emoji } if !f.deleted && !emoji.trim().is_empty() && emoji.len() <= 32 => { f.reactions.insert(event.author.clone(), emoji.clone()); }
                    types::Annotation::Unreact => { f.reactions.remove(&event.author); }
                    _ => {}
                }
            }
            let receipts = self.saved.receipts.get(space);
            let others: Vec<&String> = roster.keys().filter(|m| *m != me).collect();
            let watermark = |pick: fn(&(u64, u64)) -> u64| -> u64 {
                others.iter().map(|m| receipts.and_then(|r| r.get(*m)).map(pick).unwrap_or(0)).min().unwrap_or(0)
            };
            let (all_delivered, all_read) = (watermark(|r| r.0.max(r.1)), watermark(|r| r.1));
            let my_read = receipts.and_then(|r| r.get(me)).map(|r| r.1).unwrap_or(0);
            let mut messages = events.iter().filter_map(|event| {
                let mut view = self.message_view_with_media(event, "confirmed", None, &item_edits, &accepted_media)?;
                view.entry = Some(event.hash.clone());
                view.seq = Some(event.seq);
                if let EventBody::ItemCreated { item, .. } = &event.body {
                    if let Some((_, versions)) = files.get(item.as_str()) {
                        view.file = versions.last().map(|doc| (*doc).clone());
                        view.file_version = u32::try_from(versions.len()).ok();
                        view.edited = versions.len() > 1;
                    }
                }
                if event.author == *me {
                    view.ticks = Some(if others.is_empty() || event.seq > all_delivered { "sent" } else if event.seq > all_read { "delivered" } else { "read" });
                }
                if let Some(f) = folded.get(&event.hash) {
                    view.deleted = f.deleted;
                    view.text_revision = if f.deleted { None } else { f.text_revision.clone().or(view.text_revision) };
                    if f.deleted { view.text.clear(); view.media = None; view.file = None; view.file_item_id = None; view.file_version = None; view.card = None; }
                    if let Some(text) = &f.text { view.text = text.clone(); view.edited = true; }
                    let mut groups: BTreeMap<&str, Vec<ReactionParticipant>> = BTreeMap::new();
                    for (who, emoji) in &f.reactions {
                        let name = if who == me { self.account.name.clone() } else {
                            self.saved.profiles.get(who).map(|profile| {
                                if profile.name.is_empty() { format!("@{}", profile.handle) } else { profile.name.clone() }
                            }).unwrap_or_else(|| "Alguém".into())
                        };
                        groups.entry(emoji.as_str()).or_default().push(ReactionParticipant { identity: who.clone(), name });
                    }
                    view.reactions = groups.into_iter().map(|(emoji, participants)| Reaction {
                        emoji: emoji.into(), count: participants.len(),
                        mine: participants.iter().any(|participant| participant.identity == *me), participants,
                    }).collect();
                }
                Some(view)
            }).collect::<Vec<_>>();
            messages.extend(self.saved.outbox.values().filter_map(|pending| {
                let inner = pending.inner.as_ref()?;
                if inner.space != *space { return None; }
                self.message_view_with_media(inner, if pending.failure.is_some() { "failed" } else { "sending" }, pending.failure.clone(), &item_edits, &accepted_media)
            }));
            // Native references may use client ids. Normalize before folding replies,
            // preserving real hash targets if an alias collides.
            for message in &mut messages {
                if let Some(quote) = &mut message.quote {
                    if let Some((_, hash)) = authors.get(quote.id.as_str()) { quote.id = (*hash).into(); }
                }
                if let Some(thread) = &mut message.thread {
                    if let Some((_, hash)) = authors.get(thread.as_str()) { *thread = (*hash).into(); }
                }
            }
            // Quotes and thread summaries resolve against the folded views.
            let by_entry: BTreeMap<String, (String, String, i64, String)> = messages.iter().filter_map(|m| m.entry.clone().map(|e| (e, (m.name.clone(), if m.deleted { String::new() } else { m.text.clone() }, m.at_ms, m.author.clone())))).collect();
            let mut threads: BTreeMap<String, (usize, Vec<String>, i64)> = BTreeMap::new();
            for m in &messages {
                if let Some(root) = &m.thread {
                    let t = threads.entry(root.clone()).or_default();
                    t.0 += 1; t.2 = t.2.max(m.at_ms);
                    if !t.1.contains(&m.name) { t.1.push(m.name.clone()); }
                }
            }
            for m in &mut messages {
                if let Some(Quote { id, name, text }) = &mut m.quote {
                    if let Some((n, t, _, _)) = by_entry.get(id) { *name = n.clone(); *text = t.clone(); }
                }
                if let Some(entry) = &m.entry {
                    if let Some((count, who, last)) = threads.get(entry) {
                        m.replies = *count; m.repliers = who.iter().take(3).cloned().collect(); m.last_reply_ms = Some(*last);
                    }
                }
            }
            let mention = format!("@{}", self.account.name.to_lowercase());
            let incoming = messages.iter().filter(|m| m.author != *me && !m.deleted && m.seq.is_some_and(|s| s > my_read) && (my_read > 0 || m.seq.is_some()));
            let (unread, mentions) = incoming.fold((0, 0), |(u, n), m| (u + 1, n + usize::from(!self.account.name.is_empty() && m.text.to_lowercase().contains(&mention))));
            let waiting = self.document_writer(space).err();
            let unreadable = events.iter().filter(|event| matches!(&event.body, EventBody::Sealed { kind } if kind == "Application")).count();
            let prefs = preferences.get(space).cloned().unwrap_or_default();
            let mut people: BTreeSet<_> = roster.keys().cloned().chain(events.iter().map(|event| event.author.clone())).collect();
            let owners: Vec<_> = people.iter().filter_map(|id| self.saved.profiles.get(id)?.owner.clone()).collect();
            people.extend(owners);
            let profiles = people.iter().filter_map(|id| self.conversation_profile(id)).collect();
            Some(ConversationView { id: space.clone(), title, kind: match kind { SpaceKind::Direct=>"direct",SpaceKind::Group=>"group",SpaceKind::Community=>"community",SpaceKind::Personal=>"personal" },
                privacy, profiles, prefs: ChatPreferencesView { pinned: prefs.pinned, archived: prefs.archived, muted_until_ms: prefs.muted_until_ms.map(|time| time.to_string()) },
                event_count: self.logs.get(space)?.next_seq().to_string(),
                can_send: waiting.is_none(), waiting,
                integrity_error: self.saved.integrity_errors.get(space).cloned().or_else(|| item_projection_error.clone()),
                unreadable, unread, mentions, members: roster.len(), roster, messages })
        }).collect::<Vec<_>>();
        conversations.extend(self.creating_views().into_iter().map(|mut view| {
            view.integrity_error = item_projection_error.clone();
            view
        }));
        conversations
    }

    /// Preferences belong to my own personal log. A chat participant cannot set
    /// my preferences by posting the same body in another Space.
    pub(super) fn chat_preferences(&self) -> BTreeMap<String, types::ChatPrefs> {
        let mut preferences = BTreeMap::new();
        for (space, log) in &self.logs {
            if !matches!(self.metadata(space), Some((_, SpaceKind::Personal, _)))
                || !log
                    .events()
                    .first()
                    .is_some_and(|event| event.author == self.account.identity)
            {
                continue;
            }
            for event in log.events() {
                if event.author != self.account.identity {
                    continue;
                }
                if let EventBody::ChatPrefsSet { space, prefs, .. } = &event.body {
                    preferences.insert(space.clone(), prefs.clone());
                }
            }
        }
        preferences
    }
}
