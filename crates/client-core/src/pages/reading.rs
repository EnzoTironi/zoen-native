//! Reading on the original Engine and canonical document state.

use super::*;

impl Engine {
    pub(super) fn page_payload(&self, c: &PageContent) -> Option<Vec<u8>> {
        if let Some(b) = &c.blob {
            return self.media(&b.sha256).ok().flatten();
        }
        base64::engine::general_purpose::STANDARD
            .decode(&c.loro)
            .ok()
    }

    /// Brings the session for `item` up to date with the log; returns false if not a page.
    pub(super) fn with_page<T>(
        &self,
        item: &str,
        f: impl FnOnce(&ItemState, &mut PageSession) -> R<T>,
    ) -> R<T> {
        self.with_document(item, ItemKind::Page, f)
    }

    pub(super) fn with_document<T>(
        &self,
        item: &str,
        kind: ItemKind,
        f: impl FnOnce(&ItemState, &mut PageSession) -> R<T>,
    ) -> R<T> {
        let it = self.item_state(item)?;
        if it.kind != kind {
            return Err(invalid(
                "este Item não é uma página",
                "this Item is not a page",
            ));
        }
        let mut sessions = self
            .pages
            .sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let session = match sessions.entry(item.to_string()) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(PageSession::open(self, item)?)
            }
        };
        f(it, session)
    }

    pub(super) fn page_dto(&self, it: &ItemState, read: SessionRead) -> R<PageDto> {
        let DocumentView::BinaryPage { blocks } = read.view else {
            return Err(invalid(
                "este Item não é uma página",
                "this Item is not a page",
            ));
        };
        if read.tag != session::DocumentTag::BinaryPageV1 {
            return Err(invalid(
                "este Item não é uma página",
                "this Item is not a page",
            ));
        }
        let title = match &it.versions.current().content {
            ItemContent::Page(content) => content.title.clone(),
            _ => String::new(),
        };
        let path = match &it.versions.initial().content {
            ItemContent::Page(content) => content.path.clone(),
            _ => String::new(),
        };
        Ok(PageDto {
            item_id: it.id.clone(),
            space_id: it.space.clone(),
            title,
            path,
            version: it.versions.len() as u32,
            ready: read.ready,
            unsaved: read.unsaved,
            input_pending: read.input_pending,
            input_revision: read.input_revision.to_string(),
            pending_sync: read.pending_sync,
            save_error: read.save_error,
            can_edit: read.can_edit,
            content_hash: read.content_hash,
            edit_context: read.edit_context,
            blocks: blocks.iter().map(block_dto).collect(),
        })
    }

    pub fn page(&self, item: &str) -> R<PageDto> {
        self.with_page(item, |it, session| self.page_dto(it, session.read(self)?))
    }

    /// The page as it read at `version` (for looking back before restoring).
    pub fn page_at(&self, item: &str, version: u32) -> R<PageDto> {
        self.with_page(item, |it, session| {
            let read = session.read(self)?;
            let old = session.binary_at(version)?;
            let mut dto = self.page_dto(it, read)?;
            dto.version = version;
            // binary_at decoded this exact frontier; newer unavailable media
            // and private input do not make the historical preview unavailable.
            dto.ready = true;
            dto.input_pending = false;
            dto.input_revision = "0".into();
            dto.unsaved = false;
            dto.can_edit = false;
            dto.content_hash = old.content_hash();
            dto.edit_context = String::new();
            dto.pending_sync = false;
            dto.save_error = None;
            if let Some(ItemContent::Page(content)) = it
                .versions
                .get((version as usize).saturating_sub(1))
                .map(|v| &v.content)
            {
                dto.title = content.title.clone();
            }
            dto.blocks = old.blocks().iter().map(block_dto).collect();
            Ok(dto)
        })
    }

    pub fn page_markdown(&self, item: &str) -> R<String> {
        self.with_page(item, |_, session| session.binary_markdown(self))
    }

    pub(crate) fn page_text(&self, item: &str) -> String {
        self.with_page(item, |_, session| session.binary_text(self))
            .unwrap_or_default()
    }
}
