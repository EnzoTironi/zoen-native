//! Whole operations on the existing Client; no additional state owner.
use super::{
    documents, is_post, item_edit_key, BTreeMap, BTreeSet, Client, Event, EventBody, ItemEditKey,
    ItemEditView, MessageView, Quote, Role, Signer,
};

mod media_replay;

pub(super) struct PostedMessage {
    pub(super) reply: Option<types::ReplyRef>,
}

impl<S: Signer> Client<S> {
    fn accepted_page_post(&self, event: &Event) -> bool {
        let EventBody::ItemCreated {
            item,
            kind: types::ItemKind::Page,
            content: types::ItemContent::Page(_),
            ..
        } = &event.body
        else {
            return false;
        };
        self.document_item(item).is_ok_and(|source| {
            source.space == event.space
                && source.versions.first().failure.is_none()
                && event_log::content_hash_of(&source.versions.first().event)
                    == event_log::content_hash_of(event)
        })
    }

    // Hashes are authoritative. An untrusted client id must never shadow one.
    pub(super) fn resolve_post(&self, space: &str, reference: &str) -> Option<&Event> {
        let events = self.logs.get(space)?.events();
        events
            .iter()
            .find(|e| e.hash == reference)
            .or_else(|| events.iter().find(|e| e.client_id == reference))
            .filter(|e| is_post(&e.body))
            .filter(|event| {
                !matches!(
                    &event.body,
                    EventBody::ItemCreated {
                        kind: types::ItemKind::Page,
                        ..
                    }
                ) || self.accepted_page_post(event)
            })
    }

    /// The reply link of the message with this entry id in `space`, if it is a message.
    pub(super) fn posted(&self, space: &str, entry: &str) -> Option<PostedMessage> {
        self.logs
            .get(space)?
            .events()
            .iter()
            .find(|e| e.hash == entry)
            .or_else(|| self.resolve_post(space, entry))
            .and_then(|e| match &e.body {
                EventBody::MessagePosted { reply, .. } => Some(PostedMessage {
                    reply: reply.clone(),
                }),
                EventBody::ItemCreated {
                    content: types::ItemContent::File(doc),
                    ..
                } => Some(PostedMessage {
                    reply: doc.voice.as_ref().and_then(|v| v.reply.clone()),
                }),
                EventBody::ItemCreated {
                    kind: types::ItemKind::Page,
                    content: types::ItemContent::Page(_),
                    ..
                } if self.accepted_page_post(e) => Some(PostedMessage { reply: None }),
                _ => None,
            })
    }

    /// Replay received metadata in signed order, using the same membership cursor
    /// as native. Final file caches alone cannot authorize an earlier album claim.
    pub(super) fn received_media_posts(
        &self,
        space: &str,
        items: &BTreeMap<String, documents::Item>,
    ) -> BTreeSet<String> {
        let mut replay = media_replay::MediaReplay::default();
        let mut events = self
            .logs
            .get(space)
            .into_iter()
            .flat_map(event_log::SpaceLog::events)
            .map(|event| (event, false, false))
            .collect::<Vec<_>>();
        let confirmed: BTreeSet<_> = events
            .iter()
            .map(|(event, _, _)| event.client_id.as_str())
            .collect();
        let mut pending = self
            .saved
            .outbox
            .values()
            .filter_map(|pending| {
                pending
                    .inner
                    .as_ref()
                    .filter(|event| {
                        event.space == space && !confirmed.contains(event.client_id.as_str())
                    })
                    .map(|event| (pending, event))
            })
            .collect::<Vec<_>>();
        pending.sort_by_key(|(pending, _)| (pending.order.unwrap_or(0), &pending.content));
        events.extend(
            pending
                .into_iter()
                .map(|(pending, event)| (event, true, pending.failure.is_some())),
        );
        for (event, pending, failed) in events {
            replay.record(space, items, &self.account.identity, event, pending, failed);
        }
        replay.accepted
    }

    #[cfg(test)]
    pub(super) fn message_view(
        &self,
        event: &Event,
        delivery: &'static str,
        failure: Option<String>,
        item_edits: &BTreeMap<ItemEditKey, ItemEditView>,
    ) -> Option<MessageView> {
        let accepted =
            self.received_media_posts(&event.space, &self.document_items().unwrap_or_default());
        self.message_view_with_media(event, delivery, failure, item_edits, &accepted)
    }

    pub(super) fn message_view_with_media(
        &self,
        event: &Event,
        delivery: &'static str,
        failure: Option<String>,
        item_edits: &BTreeMap<ItemEditKey, ItemEditView>,
        accepted_media: &BTreeSet<String>,
    ) -> Option<MessageView> {
        if matches!(&event.body, EventBody::MessagePosted { media: Some(_), .. })
            && !accepted_media.contains(&event_log::content_hash_of(event))
        {
            return None;
        }
        let item_edit = item_edits.get(&item_edit_key(event)).cloned();
        let (text, reply, forwarded, file) = match &event.body {
            EventBody::MessagePosted {
                text,
                reply,
                forwarded,
                attaches,
                ..
            } => {
                // Resolve only from this conversation's readable events, never the
                // global media cache (which may contain another space's files).
                let files = self.folded_files(&event.space);
                let file = attaches
                    .as_ref()
                    .and_then(|item| files.get(item.as_str()))
                    .and_then(|(_, versions)| versions.last())
                    .map(|doc| (*doc).clone());
                (text.clone(), reply.clone(), *forwarded, file)
            }
            EventBody::ItemCreated {
                content: types::ItemContent::File(doc),
                ..
            } if doc.is_voice() || delivery == "failed" => (
                String::new(),
                doc.voice.as_ref().and_then(|v| v.reply.clone()),
                doc.voice.as_ref().is_some_and(|v| v.forwarded),
                Some(doc.clone()),
            ),
            EventBody::ItemCreated {
                kind: types::ItemKind::Page,
                content: types::ItemContent::Page(_),
                ..
            } if delivery != "failed" && self.accepted_page_post(event) => {
                (String::new(), None, false, None)
            }
            EventBody::ItemVersioned { .. } | EventBody::ItemReverted { .. } => {
                let edit = item_edit.as_ref()?;
                (edit.note.clone(), None, false, None)
            }
            _ => return None,
        };
        let media = self.displayed_media(event, file.as_ref());
        let name = self.displayed_author_name(&event.author);
        let file_item_id = match &event.body {
            EventBody::MessagePosted { attaches, .. } => attaches.clone(),
            EventBody::ItemCreated {
                item,
                content: types::ItemContent::File(_),
                ..
            } => Some(item.clone()),
            _ => None,
        }
        .filter(|_| file.is_some());
        let file_version = file_item_id.as_ref().and_then(|item| {
            self.folded_files(&event.space)
                .get(item.as_str())
                .and_then(|(_, versions)| u32::try_from(versions.len()).ok())
        });
        let card = self.displayed_card(event, file.is_some());
        Some(MessageView {
            id: event.client_id.clone(),
            entry: None,
            seq: None,
            author: event.author.clone(),
            name,
            text,
            at_ms: event.at_ms,
            delivery,
            failure,
            ticks: None,
            edited: false,
            deleted: false,
            forwarded,
            reactions: Vec::new(),
            quote: reply.as_ref().filter(|r| !r.thread).map(|r| Quote {
                id: r.to.clone(),
                name: String::new(),
                text: String::new(),
            }),
            thread: reply.as_ref().filter(|r| r.thread).map(|r| r.to.clone()),
            replies: 0,
            repliers: Vec::new(),
            last_reply_ms: None,
            file,
            media,
            file_item_id,
            file_version,
            card,
            item_edit,
        })
    }
    fn displayed_media(
        &self,
        event: &Event,
        file: Option<&types::FileDoc>,
    ) -> Option<types::MessageMedia> {
        match &event.body {
            EventBody::MessagePosted { media, .. } if file.is_some() => media
                .as_ref()
                .filter(|value| value.is_valid() && !file.is_some_and(types::FileDoc::is_voice))
                .filter(|value| {
                    value.group.is_none()
                        || match &event.body {
                            EventBody::MessagePosted {
                                attaches: Some(item),
                                ..
                            } => self
                                .folded_files(&event.space)
                                .get(item.as_str())
                                .is_some_and(|(author, _)| *author == event.author),
                            _ => false,
                        }
                })
                .cloned(),
            _ => None,
        }
    }
    fn displayed_card(&self, event: &Event, has_file: bool) -> Option<serde_json::Value> {
        match &event.body {
            EventBody::MessagePosted {
                attaches: Some(item),
                ..
            } if !has_file => self.item_card_value(&event.space, item).ok().flatten(),
            EventBody::ItemCreated {
                item,
                kind: types::ItemKind::Page,
                content: types::ItemContent::Page(_),
                ..
            } => self.item_card_value(&event.space, item).ok().flatten(),
            _ => None,
        }
    }
    pub(super) fn displayed_author_name(&self, author: &str) -> String {
        if author == self.account.identity {
            self.account.name.clone()
        } else {
            self.saved.profiles.get(author).map_or_else(
                || "Alguém".into(),
                |profile| {
                    if profile.name.is_empty() {
                        format!("@{}", profile.handle)
                    } else {
                        profile.name.clone()
                    }
                },
            )
        }
    }
}
