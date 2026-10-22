//! Accepted and recoverable item/media projection state.
use super::*;

impl State {
    pub(super) fn create_item(
        &mut self,
        e: &Event,
        item: &str,
        kind: ItemKind,
        content: &ItemContent,
        origin: &str,
    ) {
        // A validated creation in the signed projection, rather than readable
        // failed-outbox bytes, establishes an Item's authority.
        self.recovery_items.remove(item);
        self.items.insert(
            item.to_string(),
            ItemState {
                id: item.to_string(),
                space: e.space.clone(),
                kind,
                origin: origin.to_string(),
                has_voice_history: matches!(content, ItemContent::File(file) if file.is_voice()),
                versions: vec![Version {
                    number: 1,
                    content: content.clone(),
                    author: e.author.clone(),
                    at_ms: e.at_ms,
                    note: t("Criado", "Created"),
                    is_undo: false,
                }],
            },
        );
        // A voice note is a message in the chat: a player, not a file in a list.
        if let ItemContent::File(doc) = content {
            if doc.is_voice() {
                if let Some(s) = self.spaces.get_mut(&e.space) {
                    s.entries.push(Entry {
                        hash: e.hash.clone(),
                        client_id: e.client_id.clone(),
                        space: e.space.clone(),
                        seq: e.seq,
                        author: e.author.clone(),
                        at_ms: e.at_ms,
                        body: EntryBody::Message {
                            text: String::new(),
                            attaches: Some(item.to_string()),
                            reply: doc.voice.as_ref().and_then(|meta| meta.reply.clone()),
                            marks: MessageMarks {
                                forwarded: doc.voice.as_ref().is_some_and(|meta| meta.forwarded),
                                ..Default::default()
                            },
                        },
                    });
                    s.last_at_ms = s.last_at_ms.max(e.at_ms);
                }
            }
        }
    }

    /// Local recovery only: a refused send stays readable even if its author lost
    /// membership while offline. It never enters the confirmed log or outbox transport.
    pub(crate) fn retain_failed_voice(&mut self, e: &Event) {
        if let EventBody::ItemCreated {
            item,
            kind: ItemKind::File,
            content: content @ ItemContent::File(doc),
            origin,
        } = &e.body
        {
            if doc.is_voice()
                && !self.items.contains_key(item)
                && self.spaces.contains_key(&e.space)
            {
                self.create_item(e, item, ItemKind::File, content, origin);
            }
        }
    }

    pub(crate) fn is_recovery_item(&self, item: &str) -> bool {
        self.recovery_items.contains(item)
    }

    /// Reserve a failed local identity before confirmed events are replayed. A
    /// legacy attachment claim must not acquire a card when recovery bytes are
    /// inserted later. A validated ItemCreated clears this derived reservation.
    pub(crate) fn reserve_failed_attachment(&mut self, event: &Event, local: &str, source: &Event) {
        let EventBody::MessagePosted {
            attaches: Some(item),
            media,
            forwarded,
            ..
        } = &event.body
        else {
            return;
        };
        let EventBody::ItemCreated {
            item: source_item,
            kind: ItemKind::File,
            content: ItemContent::File(doc),
            ..
        } = &source.body
        else {
            return;
        };
        if event.author == local
            && source.author == local
            && event.space == source.space
            && item == source_item
            && !doc.is_voice()
            && media
                .as_ref()
                .is_none_or(|media| media.is_valid() && !(*forwarded && media.group.is_some()))
            && event_log::verify_author(event).is_ok()
            && event_log::verify_author(source).is_ok()
        {
            self.recovery_items.insert(item.clone());
        }
    }

    /// New metadata cannot point at another space's file or splice into another
    /// author's album. Partial arrival is allowed, but membership is ordered and
    /// unique within the signed (space, author, album id) scope.
    pub(super) fn media_message_valid(&self, space: &str, author: &str, body: &EventBody) -> bool {
        self.media_message_dependencies_valid(space, author, body)
            && (!matches!(body, EventBody::MessagePosted { media: Some(_), .. })
                || self.writer(space, author))
    }

    /// Shared by projection admission and confirmed-only outbox dependencies.
    /// Membership remains a separate admission check: a newly revoked queued
    /// author still needs the existing relay refusal and local recovery path.
    pub(crate) fn media_message_dependencies_valid(
        &self,
        space: &str,
        author: &str,
        body: &EventBody,
    ) -> bool {
        let EventBody::MessagePosted {
            media,
            attaches,
            reply,
            forwarded,
            ..
        } = body
        else {
            return true;
        };
        if attaches
            .as_ref()
            .is_some_and(|item| self.recovery_items.contains(item))
        {
            return false;
        }
        let Some(media) = media else {
            return true;
        };
        if !self
            .media_groups
            .allows(space, author, reply, *forwarded, media)
        {
            return false;
        }
        let Some(file) = attaches
            .as_ref()
            .and_then(|id| self.items.get(id))
            .filter(|item| item.space == space)
        else {
            return false;
        };
        if !matches!(&file.current().content, ItemContent::File(doc) if !doc.is_voice()) {
            return false;
        }
        if media.group.is_none() {
            return true;
        }
        if file
            .versions
            .first()
            .is_none_or(|version| version.author != author)
        {
            return false;
        }
        true
    }

    pub(crate) fn retain_failed_attachment(
        &mut self,
        event: &Event,
        local: &str,
        source: Option<&Event>,
    ) -> bool {
        let EventBody::MessagePosted {
            text,
            attaches: Some(item),
            media,
            reply,
            forwarded,
            ..
        } = &event.body
        else {
            return false;
        };
        if event.author != local
            || event_log::verify_author(event).is_err()
            || media
                .as_ref()
                .is_some_and(|media| !media.is_valid() || (*forwarded && media.group.is_some()))
            || !self.spaces.contains_key(&event.space)
        {
            return false;
        }
        if !self.items.contains_key(item) {
            let Some(source) = source.filter(|source| {
                source.space == event.space
                    && source.author == local
                    && event_log::verify_author(source).is_ok()
            }) else {
                return false;
            };
            let EventBody::ItemCreated {
                item: source_item,
                kind: ItemKind::File,
                content: content @ ItemContent::File(doc),
                origin,
            } = &source.body
            else {
                return false;
            };
            if source_item != item || doc.is_voice() {
                return false;
            }
            self.create_item(source, item, ItemKind::File, content, origin);
            self.recovery_items.insert(item.clone());
        }
        if self
            .items
            .get(item)
            .filter(|file| {
                file.space == event.space
                    && matches!(&file.current().content, ItemContent::File(doc) if !doc.is_voice())
                    && (media.as_ref().is_none_or(|media| media.group.is_none())
                        || file
                            .versions
                            .first()
                            .is_some_and(|version| version.author == local))
            })
            .is_none()
        {
            return false;
        }
        let state = self.spaces.get_mut(&event.space).expect("checked above");
        if state
            .entries
            .iter()
            .any(|entry| entry.client_id == event.client_id)
        {
            return true;
        }
        state.entries.push(Entry {
            hash: event.hash.clone(),
            client_id: event.client_id.clone(),
            space: event.space.clone(),
            seq: event.seq,
            author: event.author.clone(),
            at_ms: event.at_ms,
            body: EntryBody::Message {
                text: text.clone(),
                attaches: Some(item.clone()),
                reply: reply.clone(),
                marks: MessageMarks {
                    media: media.clone(),
                    forwarded: *forwarded,
                    ..Default::default()
                },
            },
        });
        state.last_at_ms = state.last_at_ms.max(event.at_ms);
        true
    }
}
