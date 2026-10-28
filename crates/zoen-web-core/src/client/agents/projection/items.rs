//! Document versions derived from authenticated agent events.
use super::super::{
    content_kind, documents, writer, Event, EventBody, ItemContent, ItemKind, Projection,
};

pub(super) fn fold_event(state: &mut Projection, space: &str, event: &Event, pending: bool) {
    match &event.body {
        EventBody::ItemCreated {
            item,
            kind,
            content,
            origin,
        } if writer(state, space, &event.author)
            && !item.is_empty()
            && content_kind(*kind, content) =>
        {
            state
                .items
                .entry(item.clone())
                .or_insert_with(|| documents::Item {
                    id: item.clone(),
                    space: space.into(),
                    kind: *kind,
                    origin: origin.clone(),
                    versions: documents::VersionHistory::new(documents::Version {
                        content: content.clone(),
                        event: event.clone(),
                        note: origin.clone(),
                        is_undo: false,
                        pending,
                        failure: None,
                    }),
                });
        }
        EventBody::ItemVersioned {
            item,
            content,
            note,
        } if writer(state, space, &event.author) => {
            if let Some(item) = state.items.get_mut(item).filter(|item| {
                item.space == space
                    && (content_kind(item.kind, content)
                        || matches!((item.kind, content), (ItemKind::Plan, ItemContent::Page(_))))
            }) {
                item.versions.push(documents::Version {
                    content: content.clone(),
                    event: event.clone(),
                    note: note.clone(),
                    is_undo: false,
                    pending,
                    failure: None,
                });
            }
        }
        EventBody::ItemReverted {
            item,
            to_version,
            note,
        } if writer(state, space, &event.author) => {
            if let Some(item) = state.items.get_mut(item).filter(|item| item.space == space) {
                if let Some(content) = to_version
                    .checked_sub(1)
                    .and_then(|index| usize::try_from(index).ok())
                    .and_then(|index| item.versions.get(index))
                    .map(|version| version.content.clone())
                {
                    item.versions.push(documents::Version {
                        content,
                        event: event.clone(),
                        note: note.clone(),
                        is_undo: true,
                        pending,
                        failure: None,
                    });
                }
            }
        }
        _ => {}
    }
}
