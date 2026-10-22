//! Publication on the original Engine and canonical document state.

use super::*;

impl Engine {
    /// Applies an editor's state to this device's copy (not saved as a version yet).
    pub fn page_apply_from(
        &self,
        item: &str,
        mutation_id: &str,
        context: &str,
        order: &[String],
        changed: &[PageBlockDto],
    ) -> R<PageEditResult> {
        self.with_page(item, |it, session| {
            let accepted = session.apply_from(
                self,
                mutation_id,
                context,
                DocumentEdit::BinaryPage { order, changed },
            )?;
            Ok(PageEditResult {
                mutation_hash: accepted.request_hash,
                mutation_id: accepted.mutation_id,
                applied_content_hash: accepted.applied_content_hash,
                replayed: accepted.replayed,
                applied_edit_context: accepted.applied_edit_context,
                page: self.page_dto(it, accepted.current)?,
            })
        })
    }

    /// Queues one new version; an unchanged or already pending page returns false.
    pub fn page_commit(&mut self, item: &str, note: &str) -> R<bool> {
        self.page_write_author(item)?;
        if self.item_state(item)?.kind != ItemKind::Page {
            return Err(invalid(
                "este Item não é uma página",
                "this Item is not a page",
            ));
        }
        // Detach the one owner before append: an actual storage rollback calls reload,
        // which must be able to clear the cache without holding its mutex here.
        let cached = self
            .pages
            .sessions
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(item);
        let mut session = match cached {
            Some(session) => session,
            None => PageSession::open(self, item)?,
        };
        let note = if note.is_empty() {
            t("Editou", "Edited")
        } else {
            note.into()
        };
        let result = session.commit(self, &note);
        self.pages
            .sessions
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(item.into(), session);
        let queued = matches!(result?, CommitStatus::Queued(_));
        if queued {
            self.index_dirty = true;
        }
        Ok(queued)
    }

    /// Makes the page read as it did at version, as a new version.
    pub(crate) fn page_restore(&mut self, item: &str, version: u32, note: &str) -> R<()> {
        self.with_page(item, |_, session| session.restore_binary(self, version))?;
        self.page_commit(item, note)?;
        Ok(())
    }
}
