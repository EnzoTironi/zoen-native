//! Pages and files for the apps (ADR 0040).

use crate::files::FileDto;
use crate::pages::{MarkdownFileDto, PageBlockDto, PageDto};
use crate::{CoreError, ItemDetail, RodaEngine};

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
        self.lock().page_apply(&item_id, &order, &changed)
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
        let mut e = self.lock();
        let id = e.file_add(&space_id, &path, &name, &mime, &bytes, thumbnail.as_deref())?;
        e.item(&id)
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

    /// The file's bytes (latest version, or `version`), or `None` while pieces are missing.
    pub fn file_bytes(
        &self,
        item_id: String,
        version: Option<u32>,
    ) -> Result<Option<Vec<u8>>, CoreError> {
        self.lock().file_bytes(&item_id, version)
    }

    pub fn file_thumbnail(&self, item_id: String) -> Result<Option<Vec<u8>>, CoreError> {
        self.lock().file_thumbnail(&item_id)
    }

    /// The file's description, including how much of it is on this device.
    pub fn file_info(&self, item_id: String) -> Result<Option<FileDto>, CoreError> {
        Ok(self.lock().item(&item_id)?.file)
    }
}
