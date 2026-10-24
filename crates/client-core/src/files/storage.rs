//! Storage operations on the original Engine state.

use super::*;
pub(crate) struct FileAddInput<'a> {
    pub space: &'a str,
    pub path: &'a str,
    pub name: &'a str,
    pub mime: &'a str,
    pub bytes: &'a [u8],
    pub thumbnail: Option<&'a [u8]>,
}

impl Engine {
    pub(super) fn file_item(&self, item: &str) -> R<(&crate::engine::ItemState, FileDoc)> {
        let it = self
            .state
            .items
            .get(item)
            .ok_or_else(|| CoreError::NotFound {
                what: "Item".into(),
            })?;
        match &it.versions.last().map(|v| &v.content) {
            Some(ItemContent::File(f)) => Ok((it, f.clone())),
            _ => Err(invalid(
                "este Item não é um arquivo",
                "this Item is not a file",
            )),
        }
    }

    /// The key shared by every version of a file (kept in the device database).
    pub(crate) fn file_key(&mut self, item: Option<&str>) -> R<[u8; 32]> {
        if let Some(item) = item {
            if let Some(k) = self.store.meta(&format!("file_key:{item}"))? {
                if let Ok(b) = hex::decode(k) {
                    if let Ok(arr) = <[u8; 32]>::try_from(b.as_slice()) {
                        return Ok(arr);
                    }
                }
            }
        }
        random_key()
    }

    /// Stores `plain` locally and, in a relay-synced Space, queues its encrypted copy.
    pub(super) fn put_chunk(
        &mut self,
        space: &str,
        file_key: &[u8; 32],
        plain: &[u8],
        mime: &str,
        prior: &HashMap<String, ChunkRef>,
    ) -> R<ChunkRef> {
        // A chunk an earlier version already has: same address, nothing new to upload.
        let sha = hex::encode(Sha256::digest(plain));
        if let Some(c) = prior.get(&sha) {
            if !self.store.has_media(&sha)? {
                self.store.put_media(&sha, mime, plain, now_ms())?;
            }
            return Ok(c.clone());
        }
        let (sha, key, sealed) = seal_chunk(file_key, plain)?;
        self.store.put_media(&sha, mime, plain, now_ms())?;
        let blob = hex::encode(Sha256::digest(&sealed));
        let key_hex = hex::encode(key);
        if self.net.synced.contains(space) {
            match self.store.media_key(&sha)? {
                Some((b, _)) if b == blob => {} // already uploaded or queued
                _ => self
                    .store
                    .seal_media(&sha, &blob, &key_hex, &sealed, now_ms())?,
            }
        } else {
            self.store.learn_media_key(&sha, &blob, &key_hex)?;
        }
        Ok(ChunkRef {
            sha256: sha,
            bytes: plain.len() as u64,
            blob,
            key: key_hex,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn file_doc(
        &mut self,
        space: &str,
        file_key: &[u8; 32],
        name: &str,
        path: &str,
        mime: &str,
        bytes: &[u8],
        thumbnail: Option<&[u8]>,
        prior: &HashMap<String, ChunkRef>,
    ) -> R<FileDoc> {
        if bytes.is_empty() || bytes.len() > MAX_FILE_BYTES {
            return Err(invalid(
                "arquivo vazio ou grande demais",
                "file empty or too large",
            ));
        }
        let mut chunks = Vec::new();
        for c in fastcdc::v2020::FastCDC::new(bytes, MIN_CHUNK, AVG_CHUNK, MAX_CHUNK) {
            chunks.push(self.put_chunk(
                space,
                file_key,
                &bytes[c.offset..c.offset + c.length],
                "application/octet-stream",
                prior,
            )?);
        }
        let thumb = match thumbnail {
            Some(t) if !t.is_empty() && t.len() <= MAX_THUMBNAIL_BYTES => {
                Some(self.put_chunk(space, file_key, t, "image/png", prior)?)
            }
            _ => None,
        };
        Ok(FileDoc {
            name: name.to_string(),
            path: path.to_string(),
            mime: mime.to_string(),
            bytes: bytes.len() as u64,
            sha256: hex::encode(Sha256::digest(bytes)),
            chunks,
            thumb,
            voice: None,
        })
    }

    /// Adds a file to a Space as a new Item (version 1).
    #[allow(clippy::too_many_arguments)]
    pub fn file_add(
        &mut self,
        space: &str,
        path: &str,
        name: &str,
        mime: &str,
        bytes: &[u8],
        thumbnail: Option<&[u8]>,
    ) -> R<String> {
        self.file_add_at(
            FileAddInput {
                space,
                path,
                name,
                mime,
                bytes,
                thumbnail,
            },
            &new_id("it"),
        )
    }

    /// A caller-bound canonical Item ID makes an unknown/missing receipt fail
    /// closed instead of creating a second file. Existing IDs are never reused.
    pub(crate) fn file_add_at(&mut self, input: FileAddInput<'_>, item: &str) -> R<String> {
        let FileAddInput {
            space,
            path,
            name,
            mime,
            bytes,
            thumbnail,
        } = input;
        if item.is_empty()
            || item.len() > 128
            || !item
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
            || self.state.items.contains_key(item)
        {
            return Err(invalid(
                "identificador de arquivo já utilizado",
                "file identifier already used",
            ));
        }
        let item = item.to_owned();
        let me = self.me_id()?;
        if !self.is_member(space, &me) {
            return Err(CoreError::Forbidden {
                reason: t(
                    "só membros adicionam arquivos",
                    "only members can add files",
                ),
            });
        }
        let key = self.file_key(None)?;
        let doc = self.file_doc(
            space,
            &key,
            name,
            path,
            mime,
            bytes,
            thumbnail,
            &HashMap::new(),
        )?;
        self.store
            .set_meta(&format!("file_key:{item}"), &hex::encode(key))?;
        self.append(
            space,
            &me,
            EventBody::ItemCreated {
                item: item.clone(),
                kind: ItemKind::File,
                content: ItemContent::File(doc),
                origin: t("Adicionado", "Added"),
            },
        )?;
        self.index_dirty = true;
        Ok(item)
    }

    pub(super) fn create_voice_note(
        &mut self,
        space: &str,
        bytes: &[u8],
        mime: &str,
        meta: VoiceMeta,
    ) -> R<TimelineEntry> {
        let me = self.me_id()?;
        let item = new_id("it");
        let ext = match mime.split(';').next().unwrap_or_default() {
            "audio/webm" => "webm",
            "audio/ogg" => "ogg",
            "audio/mpeg" => "mp3",
            "audio/wav" | "audio/x-wav" => "wav",
            _ => "m4a",
        };
        let key = self.file_key(None)?;
        let mut doc = self.file_doc(
            space,
            &key,
            &format!("voice-{item}.{ext}"),
            &format!("VoiceNotes/{item}.{ext}"),
            mime,
            bytes,
            None,
            &HashMap::new(),
        )?;
        doc.voice = Some(meta);
        self.store
            .set_meta(&format!("file_key:{item}"), &hex::encode(key))?;
        self.append(
            space,
            &me,
            EventBody::ItemCreated {
                item: item.clone(),
                kind: ItemKind::File,
                content: ItemContent::File(doc),
                origin: t("Mensagem de voz", "Voice message"),
            },
        )?;
        self.index_dirty = true;
        let s = self.space_state(space)?;
        Ok(self.entry_dto(s.entries.last().expect("just appended")))
    }
}
