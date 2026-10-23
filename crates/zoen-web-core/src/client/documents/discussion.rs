//! Read the existing artifact post. No new comment log or publication on open.
use super::*;

impl<S: Signer> Client<S> {
    pub(super) fn item_discussion_value(&self, space: &str, id: &str) -> Result<Value, String> {
        if !self.roster(space).contains_key(&self.account.identity) {
            return Err(t(
                "Só membros leem esta discussão.",
                "Only members can read this discussion.",
            ));
        }
        if let Some(reason) = self.saved.integrity_errors.get(space) {
            return Err(reason.clone());
        }
        let item = self.document_item(id)?;
        if item.space != space || item.versions[0].failure.is_some() {
            return Err(t(
                "Arquivo indisponível nesta conversa.",
                "Artifact unavailable in this conversation.",
            ));
        }
        let conversation = self
            .conversations()
            .into_iter()
            .find(|view| view.id == space)
            .ok_or_else(|| t("Conversa indisponível.", "Conversation unavailable."))?;
        let origin = &item.versions[0].event;
        let original = |event: &Event| match &event.body {
            EventBody::ItemCreated {
                item: source,
                kind: ItemKind::Page,
                content: ItemContent::Page(_),
                ..
            } => {
                source == id
                    && event_log::content_hash_of(event) == event_log::content_hash_of(origin)
            }
            EventBody::MessagePosted {
                attaches: Some(source),
                reply: None,
                forwarded: false,
                ..
            } => source == id,
            _ => false,
        };
        let mut events = self
            .logs
            .get(space)
            .into_iter()
            .flat_map(|log| log.events())
            .collect::<Vec<_>>();
        let confirmed = events
            .iter()
            .map(|event| event.client_id.as_str())
            .collect::<BTreeSet<_>>();
        let mut pending = self
            .saved
            .outbox
            .values()
            .filter_map(|pending| {
                let event = pending.inner.as_ref()?;
                (event.space == space
                    && pending.failure.is_none()
                    && !confirmed.contains(event.client_id.as_str()))
                .then_some((pending.order.unwrap_or(0), event))
            })
            .collect::<Vec<_>>();
        pending.sort_by_key(|(order, event)| (*order, &event.client_id));
        events.extend(pending.into_iter().map(|(_, event)| event));
        let root = events
            .into_iter()
            .filter(|event| original(event))
            .find_map(|event| {
                conversation.messages.iter().find(|message| {
                    message.thread.is_none()
                        && !message.forwarded
                        && message.id == event.client_id
                        && if event.seq > 0 {
                            message.entry.as_deref() == Some(&event.hash)
                        } else {
                            message.entry.is_none()
                        }
                })
            });
        // The browser adapter resolves this stable id through its existing typed
        // timeline projection, including deletion/reactions/reply counts.
        Ok(json!({ "item_id": id, "space_id": space, "root_id": root.map(|root| &root.id) }))
    }
}
