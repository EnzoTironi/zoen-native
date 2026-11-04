//! Sender admission and receiving replay share the same message/file checks.
use crate::engine::{Entry, EntryBody, State};
use types::{Annotation, Event, EventBody, FileDoc, ItemContent, ItemKind, MediaReplacement};
impl State {
    pub(crate) fn replacement_file<'a>(
        &'a self,
        space: &str,
        author: &str,
        replacement: &MediaReplacement,
    ) -> Option<&'a FileDoc> {
        let item = self.items.get(&replacement.item_id)?;
        if item.space != space
            || item.kind != ItemKind::File
            || item.versions.initial().author != author
        {
            return None;
        }
        let index = usize::try_from(replacement.version.checked_sub(1)?).ok()?;
        let ItemContent::File(file) = &item.versions.get(index)?.content else {
            return None;
        };
        (!file.is_voice()
            && file.sha256 == replacement.sha256
            && crate::files::validate_file(file, crate::files::MAX_FILE_BYTES as u64).is_ok())
        .then_some(file)
    }
    pub(crate) fn message_replacement_valid(
        &self,
        space: &str,
        author: &str,
        body: &EventBody,
    ) -> bool {
        let EventBody::MessageAnnotated {
            target,
            note: Annotation::ReplaceMedia { edit },
        } = body
        else {
            return true;
        };
        if !super::valid_change(edit)
            || !self.writer(space, author)
            || self
                .replacement_file(space, author, &edit.replacement)
                .is_none()
        {
            return false;
        }
        let Some(source) = self
            .spaces
            .get(space)
            .and_then(|state| state.entries.iter().find(|entry| entry.id() == target))
        else {
            return false;
        };
        let EntryBody::Message {
            text,
            attaches,
            marks,
            ..
        } = &source.body
        else {
            return false;
        };
        if source.author != author
            || marks.deleted
            || marks.forwarded
            || marks.media_edit_receipt(&edit.mutation_id).is_some()
            || (marks.replacement().is_none() && attaches.is_none() && text.starts_with("⟦voice:"))
            || marks.text_revision.as_deref().unwrap_or(source.id()) != edit.expected_revision
        {
            return false;
        }
        attaches.as_ref().is_none_or(|id| {
            self.items.get(id).is_some_and(|item| {
                item.space == space && item.kind == ItemKind::File && match marks.replacement() {
                    Some(old) => self.replacement_file(space, author, old).is_some(),
                    None => matches!(&item.current().content, ItemContent::File(file) if !file.is_voice()),
                }
            })
        })
    }
    pub(crate) fn apply_message_annotation(
        &mut self,
        event: &Event,
        target: &str,
        note: &Annotation,
    ) {
        let Some(state) = self.spaces.get_mut(&event.space) else {
            return;
        };
        let Some(Entry {
            author,
            body:
                EntryBody::Message {
                    text,
                    attaches,
                    marks,
                    ..
                },
            ..
        }) = state
            .entries
            .iter_mut()
            .rev()
            .find(|entry| entry.id() == target)
        else {
            return;
        };
        if let Annotation::ReplaceMedia { edit } = note {
            // apply_checked admitted the complete replacement, including current source revision.
            *text = edit.text.clone();
            *attaches = Some(edit.replacement.item_id.clone());
            marks.edited = true;
            let revision = event_log::content_hash_of(event);
            marks.record_media_edit(&edit.mutation_id, &revision, edit.replacement.clone());
            marks.text_revision = Some(revision);
        } else if let Some(new_text) = marks.apply(author, &event.author, note) {
            *text = new_text;
            marks.text_revision = Some(event_log::content_hash_of(event));
            if marks.deleted {
                *attaches = None;
            }
        }
    }
}
