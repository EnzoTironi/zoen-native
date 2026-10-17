//! Pages and files for the apps (ADR 0040).

use crate::files::FileDto;
use crate::pages::{MarkdownFileDto, PageBlockDto, PageDto, PageEditResult, PlanPageDto};
use crate::{CoreError, ItemDetail, RodaEngine, TimelineEntry};

#[uniffi::export]
impl RodaEngine {
    /// A page made from Markdown. Exported untouched, it gives back the same bytes.
    pub fn page_import_markdown(
        &self,
        space_id: String,
        path: String,
        markdown: String,
    ) -> Result<ItemDetail, CoreError> {
        let mut e = self.lock();
        let id = e.page_import_markdown(&space_id, &path, &markdown)?;
        e.item(&id)
    }

    /// A new, empty page with a title.
    /// Many Markdown files at once (a folder, a picked batch). They go in together, so the
    /// connection seals and sends them as one batch instead of waking up for each page.
    /// Stops at the first file that can't be imported; the ones before it stay.
    pub fn pages_import_markdown(
        &self,
        space_id: String,
        files: Vec<MarkdownFileDto>,
    ) -> Result<Vec<ItemDetail>, CoreError> {
        let mut e = self.lock();
        let mut ids = Vec::with_capacity(files.len());
        for f in &files {
            ids.push(e.page_import_markdown(&space_id, &f.path, &f.markdown)?);
        }
        ids.iter().map(|id| e.item(id)).collect()
    }

    pub fn page_create(&self, space_id: String, title: String) -> Result<ItemDetail, CoreError> {
        let mut e = self.lock();
        let id = e.page_create(&space_id, &title)?;
        e.item(&id)
    }

    /// The page's blocks as this device has them (saved versions plus unsaved edits).
    pub fn page(&self, item_id: String) -> Result<PageDto, CoreError> {
        self.lock().page(&item_id)
    }

    /// Lossless typed Plan blocks from this device's shared document session.
    /// Private drafts are labelled; this read grants no editing capability.
    pub fn plan_page(&self, item_id: String) -> Result<PlanPageDto, CoreError> {
        self.lock().plan_page(&item_id)
    }

    /// The page as it read at an earlier version.
    pub fn page_at(&self, item_id: String, version: u32) -> Result<PageDto, CoreError> {
        self.lock().page_at(&item_id, version)
    }

    /// Makes this device's copy match the editor: `order` lists every block id top to
    /// bottom; `changed` holds the blocks whose kind, text or formatting changed.
    pub fn page_apply(
        &self,
        item_id: String,
        order: Vec<String>,
        changed: Vec<PageBlockDto>,
    ) -> Result<(), CoreError> {
        let engine = self.lock();
        let current = engine.page(&item_id)?;
        engine.page_apply_from(
            &item_id,
            &roda_types::new_id("page_edit"),
            &current.edit_context,
            &order,
            &changed,
        )?;
        Ok(())
    }

    /// Applies to the context actually observed by this editor. The mutation receipt
    /// allows retries without applying the edit again or overwriting unseen changes.
    pub fn page_apply_from(
        &self,
        item_id: String,
        mutation_id: String,
        edit_context: String,
        order: Vec<String>,
        changed: Vec<PageBlockDto>,
    ) -> Result<PageEditResult, CoreError> {
        self.lock()
            .page_apply_from(&item_id, &mutation_id, &edit_context, &order, &changed)
    }

    /// Saves unsaved edits as a new version for everyone. False if nothing changed.
    pub fn page_commit(&self, item_id: String, note: String) -> Result<bool, CoreError> {
        self.lock().page_commit(&item_id, &note)
    }

    /// The page as Markdown (untouched blocks exactly as imported).
    pub fn page_markdown(&self, item_id: String) -> Result<String, CoreError> {
        self.lock().page_markdown(&item_id)
    }

    /// Adds a file to a Space. `thumbnail` is a small PNG made on this device.
    pub fn file_add(
        &self,
        space_id: String,
        path: String,
        name: String,
        mime: String,
        bytes: Vec<u8>,
        thumbnail: Option<Vec<u8>>,
    ) -> Result<ItemDetail, CoreError> {
        let _language = crate::i18n::enter(self.lang);
        let mut e = self.lock();
        let id = e.file_add(&space_id, &path, &name, &mime, &bytes, thumbnail.as_deref())?;
        e.item(&id)
    }

    /// Sends an ordinary attachment in the conversation, atomically with its file.
    /// Reuse mutation_id only to retry the exact same selected file and target.
    #[allow(clippy::too_many_arguments)]
    pub fn send_file_message(
        &self,
        space_id: String,
        name: String,
        mime: String,
        bytes: Vec<u8>,
        thumbnail: Option<Vec<u8>>,
        reply_to: Option<String>,
        thread: bool,
        mutation_id: String,
    ) -> Result<TimelineEntry, CoreError> {
        let _language = crate::i18n::enter(self.lang);
        self.lock().send_file_message(
            &space_id,
            &name,
            &mime,
            &bytes,
            thumbnail.as_deref(),
            reply_to.as_deref(),
            thread,
            &mutation_id,
        )
    }

    /// Sends a voice note as an encrypted audio file (see `Engine::send_voice_note`).
    /// `levels` are waveform bars 0…1; `reply_to` answers a message, in its thread if
    /// `thread`.
    #[allow(clippy::too_many_arguments)]
    pub fn send_voice_note(
        &self,
        space_id: String,
        bytes: Vec<u8>,
        mime: String,
        ms: u64,
        levels: Vec<f32>,
        transcript: String,
        reply_to: Option<String>,
        thread: bool,
    ) -> Result<TimelineEntry, CoreError> {
        let _language = crate::i18n::enter(self.lang);
        self.lock().send_voice_note(
            &space_id,
            &bytes,
            &mime,
            ms,
            &levels,
            &transcript,
            reply_to.as_deref(),
            thread,
        )
    }

    /// Saves new bytes for a file as its next version (markup, crop, trim…).
    pub fn file_new_version(
        &self,
        item_id: String,
        bytes: Vec<u8>,
        thumbnail: Option<Vec<u8>>,
        note: String,
    ) -> Result<ItemDetail, CoreError> {
        let mut e = self.lock();
        e.file_new_version(&item_id, &bytes, thumbnail.as_deref(), &note)?;
        e.item(&item_id)
    }

    /// Records the actual encoded name and format in the new file version.
    pub fn file_new_version_typed(
        &self,
        item_id: String,
        bytes: Vec<u8>,
        thumbnail: Option<Vec<u8>>,
        note: String,
        name: String,
        mime: String,
    ) -> Result<ItemDetail, CoreError> {
        let mut e = self.lock();
        e.file_new_version_typed(&item_id, &bytes, thumbnail.as_deref(), &note, &name, &mime)?;
        e.item(&item_id)
    }

    /// The file's bytes (latest version, or `version`), or `None` while pieces are missing.
    pub fn file_bytes(
        &self,
        item_id: String,
        version: Option<u32>,
    ) -> Result<Option<Vec<u8>>, CoreError> {
        self.lock().file_bytes(&item_id, version)
    }

    /// Validates the selected version and limits every cached payload before reading
    /// it. An oversized or damaged file is an error; missing pieces return `None`.
    pub fn file_bytes_bounded(
        &self,
        item_id: String,
        version: Option<u32>,
        max_bytes: u64,
    ) -> Result<Option<Vec<u8>>, CoreError> {
        let _language = crate::i18n::enter(self.lang);
        self.lock().file_bytes_bounded(&item_id, version, max_bytes)
    }

    pub fn file_thumbnail(&self, item_id: String) -> Result<Option<Vec<u8>>, CoreError> {
        self.lock().file_thumbnail(&item_id)
    }

    /// The file's description, including how much of it is on this device.
    pub fn file_info(&self, item_id: String) -> Result<Option<FileDto>, CoreError> {
        Ok(self.lock().item(&item_id)?.file)
    }

    /// The description of exactly the requested file version, without restoring it.
    pub fn file_info_at(
        &self,
        item_id: String,
        version: Option<u32>,
    ) -> Result<FileDto, CoreError> {
        let _language = crate::i18n::enter(self.lang);
        self.lock().file_info_at(&item_id, version)
    }
}
