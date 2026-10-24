//! Editing operations on the original Engine state.

use super::{
    invalid, t, CoreError, Digest, Engine, EventBody, FileEditStatus, HashMap, ItemContent,
    KeyInit, R,
};

impl Engine {
    pub(super) fn file_edit_status(
        &self,
        item: &str,
        expected_version: u32,
        expected_sha256: &str,
    ) -> R<FileEditStatus> {
        let me = self.me_id()?;
        let (it, current) = self.file_item(item)?;
        if self.state.is_recovery_item(item)
            || !self.state.writer(&it.space, &me)
            || !it.voice_edit_allowed(&me, None)
        {
            return Ok(FileEditStatus::Forbidden);
        }
        if it.current().number != expected_version || current.sha256 != expected_sha256 {
            return Ok(FileEditStatus::Changed);
        }
        Ok(FileEditStatus::Allowed)
    }

    /// A current, writable file at exactly the version and hash the editor saw.
    /// This is a read-only affordance; the checked mutation revalidates it.
    pub fn file_edit_allowed(
        &self,
        item: &str,
        expected_version: u32,
        expected_sha256: &str,
    ) -> R<bool> {
        Ok(matches!(
            self.file_edit_status(item, expected_version, expected_sha256)?,
            FileEditStatus::Allowed
        ))
    }

    /// Commit only against the observed base. The FFI keeps this check and the
    /// mutation under one engine lock; chunks and the signed version commit together.
    pub fn file_new_version_checked(
        &mut self,
        item: &str,
        expected_version: u32,
        expected_sha256: &str,
        bytes: &[u8],
        thumbnail: Option<&[u8]>,
        note: &str,
    ) -> R<()> {
        match self.file_edit_status(item, expected_version, expected_sha256)? {
            FileEditStatus::Allowed => {}
            FileEditStatus::Changed => {
                return Err(CoreError::Stale {
                    reason: t(
                        "o arquivo mudou; reabra a versão atual antes de salvar",
                        "the file changed; reopen the current version before saving",
                    ),
                });
            }
            FileEditStatus::Forbidden => {
                return Err(CoreError::Forbidden {
                    reason: t("edição de arquivo não autorizada", "unauthorized file edit"),
                });
            }
        }
        self.atomic_agent_action(|engine| {
            engine.file_new_version_inner(item, bytes, thumbnail, note, None)
        })
    }

    /// A new version of a file (an edit, a markup, a trim). Unchanged chunks are re-used.
    pub fn file_new_version(
        &mut self,
        item: &str,
        bytes: &[u8],
        thumbnail: Option<&[u8]>,
        note: &str,
    ) -> R<()> {
        self.file_new_version_inner(item, bytes, thumbnail, note, None)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn file_new_version_typed(
        &mut self,
        item: &str,
        bytes: &[u8],
        thumbnail: Option<&[u8]>,
        note: &str,
        name: &str,
        mime: &str,
    ) -> R<()> {
        if name.is_empty()
            || name.len() > 255
            || name.contains(['/', '\\', '\0'])
            || !mime.contains('/')
            || mime.len() > 128
        {
            return Err(invalid(
                "nome ou tipo de arquivo inválido",
                "invalid file name or type",
            ));
        }
        self.file_new_version_inner(item, bytes, thumbnail, note, Some((name, mime)))
    }

    pub(super) fn file_new_version_inner(
        &mut self,
        item: &str,
        bytes: &[u8],
        thumbnail: Option<&[u8]>,
        note: &str,
        format: Option<(&str, &str)>,
    ) -> R<()> {
        let me = self.me_id()?;
        let (it, cur) = self.file_item(item)?;
        let space = it.space.clone();
        if self.state.is_recovery_item(item)
            || !self.state.writer(&space, &me)
            || !it.voice_edit_allowed(&me, None)
        {
            return Err(CoreError::Forbidden {
                reason: t("edição de arquivo não autorizada", "unauthorized file edit"),
            });
        }
        // Every chunk any version already has is re-used as is (its key travels in the
        // event), so this works on a device that didn't add the file too.
        let mut prior = HashMap::new();
        for v in &it.versions {
            if let ItemContent::File(f) = &v.content {
                for c in f.chunks.iter().chain(f.thumb.iter()) {
                    prior.insert(c.sha256.clone(), c.clone());
                }
            }
        }
        let key = self.file_key(Some(item))?;
        self.store
            .set_meta(&format!("file_key:{item}"), &hex::encode(key))?;
        let (name, mime) = format.unwrap_or((&cur.name, &cur.mime));
        let mut doc = self.file_doc(
            &space, &key, name, &cur.path, mime, bytes, thumbnail, &prior,
        )?;
        // Renaming or changing a preview keeps the same audio. A byte edit invalidates
        // its timings and transcript (removed words must not survive in metadata).
        if doc.sha256 == cur.sha256 && doc.mime.starts_with("audio/") {
            doc.voice = cur.voice.clone();
        }
        if doc.sha256 == cur.sha256
            && doc.name == cur.name
            && doc.mime == cur.mime
            && doc.thumb.as_ref().map(|t| &t.sha256) == cur.thumb.as_ref().map(|t| &t.sha256)
        {
            return Ok(());
        }
        self.append(
            &space,
            &me,
            EventBody::ItemVersioned {
                item: item.into(),
                content: ItemContent::File(doc),
                note: note.into(),
            },
        )?;
        Ok(())
    }
}
