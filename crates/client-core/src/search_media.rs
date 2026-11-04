//! Media classification shares the exact visible/replacement source with search.
use crate::engine::{Engine, Entry, EntryBody};
use store::SearchMediaKind;
use types::{AttachmentPresentation, ItemContent};
impl Engine {
    pub(crate) fn searchable_attachment_file<'a>(
        &'a self,
        entry: &Entry,
    ) -> Option<&'a types::FileDoc> {
        let EntryBody::Message {
            attaches: Some(id),
            marks,
            ..
        } = &entry.body
        else {
            return None;
        };
        if marks.deleted || !self.visible_message(entry) || self.state.is_recovery_item(id) {
            return None;
        }
        if let Some(replacement) = marks.replacement() {
            return self.space_projections.get(&entry.space)?.replacement_file(
                &entry.space,
                &entry.author,
                replacement,
            );
        }
        let item = self
            .state
            .items
            .get(id)
            .filter(|item| item.space == entry.space)?;
        let ItemContent::File(file) = &item.current().content else {
            return None;
        };
        Some(file)
    }

    pub(crate) fn search_media_kind(
        &self,
        entry: &Entry,
        file: &types::FileDoc,
    ) -> SearchMediaKind {
        if self.entry_voice(entry).is_some() {
            return SearchMediaKind::Voice;
        }
        if file.mime.starts_with("audio/") {
            return SearchMediaKind::Audio;
        }
        if let EntryBody::Message { marks, .. } = &entry.body {
            if marks
                .media
                .as_ref()
                .is_some_and(|media| media.presentation == AttachmentPresentation::File)
            {
                return SearchMediaKind::File;
            }
        }
        if file.mime.starts_with("image/") {
            SearchMediaKind::Photo
        } else if file.mime.starts_with("video/") {
            SearchMediaKind::Video
        } else {
            SearchMediaKind::File
        }
    }
}
