use super::{documents, BTreeMap, BTreeSet, Event, EventBody, Role};

#[derive(Default)]
pub(super) struct MediaReplay {
    roles: BTreeMap<String, Role>,
    available: BTreeMap<String, (String, bool)>,
    local_sources: BTreeSet<String>,
    groups: types::media::MediaGroupTracker,
    pub(super) accepted: BTreeSet<String>,
}

impl MediaReplay {
    pub(super) fn record(
        &mut self,
        space: &str,
        items: &BTreeMap<String, documents::Item>,
        identity: &str,
        event: &Event,
        pending: bool,
        failed: bool,
    ) {
        if failed
            && !matches!(
                &event.body,
                EventBody::ItemCreated {
                    content: types::ItemContent::File(_),
                    ..
                } | EventBody::MessagePosted { media: Some(_), .. }
            )
        {
            return;
        }
        let writer = self
            .roles
            .get(&event.author)
            .is_some_and(|role| *role != Role::Reader);
        match &event.body {
            EventBody::SpaceCreated { .. } => {
                self.roles.insert(event.author.clone(), Role::Owner);
            }
            EventBody::MemberAdded { identity, role } => {
                self.roles.insert(identity.clone(), *role);
            }
            EventBody::MemberRemoved { identity } => {
                self.roles.remove(identity);
            }
            EventBody::ItemCreated {
                item,
                content: types::ItemContent::File(doc),
                ..
            } => {
                if pending
                    && event.author == *identity
                    && !doc.is_voice()
                    && event_log::verify_author(event).is_ok()
                {
                    self.local_sources.insert(item.clone());
                }
                if !failed
                    && writer
                    && items
                        .get(item)
                        .filter(|file| file.space == space)
                        .map(|file| file.versions.first())
                        .is_some_and(|version| {
                            event_log::content_hash_of(&version.event)
                                == event_log::content_hash_of(event)
                        })
                {
                    self.available
                        .entry(item.clone())
                        .or_insert((event.author.clone(), doc.is_voice()));
                }
            }
            EventBody::ItemVersioned { item, .. } | EventBody::ItemReverted { item, .. } => {
                self.update_version(space, items, event, item, writer);
            }
            EventBody::MessagePosted {
                media: Some(_),
                attaches: Some(_),
                ..
            } => self.record_post(space, identity, event, pending, failed, writer),
            _ => {}
        }
    }
    fn record_post(
        &mut self,
        space: &str,
        identity: &str,
        event: &Event,
        pending: bool,
        failed: bool,
        writer: bool,
    ) {
        let EventBody::MessagePosted {
            media: Some(media),
            attaches: Some(item),
            reply,
            forwarded,
            ..
        } = &event.body
        else {
            return;
        };

        let file = self.available.get(item).filter(|(_, voice)| !voice);
        // Failed local rows remain recovery views only; they cannot
        // consume a cursor or authorize a later signed member.
        let recovery = pending
            && failed
            && event.author == *identity
            && event_log::verify_author(event).is_ok()
            && media.is_valid()
            && !(*forwarded && media.group.is_some())
            && (file.is_some_and(|(owner, _)| media.group.is_none() || owner == &event.author)
                || self.local_sources.contains(item));
        if recovery {
            self.accepted.insert(event_log::content_hash_of(event));
            return;
        }
        if failed
            || !writer
            || !file.is_some_and(|(owner, _)| media.group.is_none() || owner == &event.author)
        {
            return;
        }
        if self
            .groups
            .record(space, &event.author, reply, *forwarded, media)
        {
            self.accepted.insert(event_log::content_hash_of(event));
        }
    }

    fn update_version(
        &mut self,
        space: &str,
        items: &BTreeMap<String, documents::Item>,
        event: &Event,
        item: &str,
        writer: bool,
    ) {
        if writer {
            if let Some(version) =
                items
                    .get(item)
                    .filter(|file| file.space == space)
                    .and_then(|file| {
                        file.versions.iter().find(|version| {
                            event_log::content_hash_of(&version.event)
                                == event_log::content_hash_of(event)
                        })
                    })
            {
                if let types::ItemContent::File(doc) = &version.content {
                    if let Some((_, voice)) = self.available.get_mut(item) {
                        *voice = doc.is_voice();
                    }
                }
            }
        }
    }
}
