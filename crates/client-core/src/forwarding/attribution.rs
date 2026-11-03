//! One attribution projection for every forwarding writer and receiving fold.
use crate::engine::{Engine, Entry, EntryBody};
use types::{EventBody, ItemContent};

pub(crate) fn metadata_valid(body: &EventBody) -> bool {
    match body {
        EventBody::MessagePosted {
            forwarded,
            forwarded_author,
            ..
        } => types::forwarding::author_valid(*forwarded, forwarded_author.as_deref()),
        EventBody::ItemCreated {
            content: ItemContent::File(file),
            ..
        }
        | EventBody::ItemVersioned {
            content: ItemContent::File(file),
            ..
        } => file.voice.as_ref().is_none_or(|meta| {
            types::forwarding::author_valid(meta.forwarded, meta.forwarded_author.as_deref())
        }),
        _ => true,
    }
}

impl Engine {
    pub(crate) fn forwarded_author(&self, source: &Entry) -> Option<String> {
        let EntryBody::Message { marks, .. } = &source.body else {
            return None;
        };
        if marks.deleted || !self.visible_message(source) {
            return None;
        }
        // Legacy/anonymous forwards must never acquire the intermediate author's
        // name. A direct source captures its current, reviewed display name.
        let name = if marks.forwarded {
            marks.forwarded_author.clone()
        } else {
            self.identities
                .get(&source.author)
                .map(|author| author.name.clone())
        };
        name.filter(|name| types::forwarding::author_valid(true, Some(name)))
    }
}
