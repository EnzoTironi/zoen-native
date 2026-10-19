//! Files as Items (ADR 0040): bytes cut into content-defined chunks, each encrypted for
//! the relay under a key derived from the file's own key and the chunk's hash.
//!
//! A new version re-uses every chunk that didn't change (same plaintext, same key, same
//! ciphertext, same address), so editing a page of a 40 MB PDF uploads one chunk, not
//! forty megabytes. Equality is visible only inside one file's own history: another file
//! has another key. Previews (thumbnails) are made by the device and travel the same way.

use chacha20poly1305::{
    aead::{Aead, KeyInit},
    XChaCha20Poly1305, XNonce,
};
use std::collections::HashMap;

use hkdf::Hkdf;
use sha2::{Digest, Sha256};
use types::*;

use crate::dto::{TimelineEntry, VoiceNoteDto};
use crate::engine::{now_ms, Engine, R};
use crate::i18n::t;
use crate::media::Wanted;
use crate::CoreError;

/// Chunk sizes: 1 MiB minimum, 4 MiB average, 16 MiB maximum (the relay takes 25 MiB).
const MIN_CHUNK: usize = 1 << 20;
const AVG_CHUNK: usize = 4 << 20;
const MAX_CHUNK: usize = 16 << 20;
/// Largest file the core takes in one call.
pub const MAX_FILE_BYTES: usize = 512 << 20;
pub const MAX_FILE_CAPTION_BYTES: usize = 16 << 10;
const MAX_THUMBNAIL_BYTES: usize = 2 << 20;
const MAX_FILE_CHUNKS: usize = MAX_FILE_BYTES / MIN_CHUNK + 1;
const MAX_FILE_METADATA_BYTES: usize = 128 << 10;

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct FileDto {
    pub name: String,
    pub path: String,
    pub mime: String,
    pub bytes: u64,
    pub sha256: String,
    /// Every piece is on this device, so the file can open.
    pub ready: bool,
    pub chunks: u32,
    pub chunks_here: u32,
    pub has_thumbnail: bool,
}

enum FileEditStatus {
    Allowed,
    Changed,
    Forbidden,
}

fn invalid(pt: &str, en: &str) -> CoreError {
    CoreError::Invalid { reason: t(pt, en) }
}

fn invalid_file() -> CoreError {
    invalid(
        "arquivo inválido ou grande demais para abrir",
        "file invalid or too large to open",
    )
}

fn hash_is_valid(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

fn validate_chunk(chunk: &ChunkRef, max_bytes: usize) -> R<usize> {
    if chunk.bytes == 0
        || chunk.bytes > max_bytes as u64
        || !hash_is_valid(&chunk.sha256)
        || !hash_is_valid(&chunk.blob)
        || !hash_is_valid(&chunk.key)
    {
        return Err(invalid_file());
    }
    usize::try_from(chunk.bytes).map_err(|_| invalid_file())
}

pub(crate) fn validate_file(file: &FileDoc, max_bytes: u64) -> R<usize> {
    if file.bytes == 0
        || file.bytes > max_bytes
        || file.bytes > MAX_FILE_BYTES as u64
        || file.chunks.is_empty()
        || file.chunks.len() > MAX_FILE_CHUNKS
        || !hash_is_valid(&file.sha256)
    {
        return Err(invalid_file());
    }
    let metadata_bytes = [&file.name, &file.path, &file.mime, &file.sha256]
        .iter()
        .try_fold(0usize, |size, value| size.checked_add(value.len()))
        .and_then(|size| {
            size.checked_add((file.chunks.len() + usize::from(file.thumb.is_some())) * (3 * 64 + 8))
        })
        .ok_or_else(invalid_file)?;
    if metadata_bytes > MAX_FILE_METADATA_BYTES {
        return Err(invalid_file());
    }
    let mut total = 0u64;
    for chunk in &file.chunks {
        validate_chunk(chunk, MAX_CHUNK)?;
        total = total.checked_add(chunk.bytes).ok_or_else(invalid_file)?;
    }
    if total != file.bytes {
        return Err(invalid_file());
    }
    if let Some(thumb) = &file.thumb {
        validate_chunk(thumb, MAX_THUMBNAIL_BYTES)?;
    }
    usize::try_from(file.bytes).map_err(|_| invalid_file())
}

/// Key and nonce for one chunk: HKDF-SHA256(file key, salt = chunk hash).
fn chunk_secret(file_key: &[u8; 32], chunk_sha: &[u8]) -> ([u8; 32], [u8; 24]) {
    let hk = Hkdf::<Sha256>::new(Some(chunk_sha), file_key);
    let mut okm = [0u8; 56];
    hk.expand(b"zoen-file-chunk/1", &mut okm)
        .expect("56 bytes is a valid HKDF length");
    let mut key = [0u8; 32];
    let mut nonce = [0u8; 24];
    key.copy_from_slice(&okm[..32]);
    nonce.copy_from_slice(&okm[32..]);
    (key, nonce)
}

/// `nonce ‖ ciphertext+tag`, the same layout `media::open` reads.
fn seal_chunk(file_key: &[u8; 32], plain: &[u8]) -> (String, [u8; 32], Vec<u8>) {
    let sha = Sha256::digest(plain);
    let (key, nonce) = chunk_secret(file_key, &sha);
    let cipher = XChaCha20Poly1305::new_from_slice(&key).expect("32-byte key");
    let ct = cipher
        .encrypt(&XNonce::from(nonce), plain)
        .expect("encrypt");
    let mut out = Vec::with_capacity(24 + ct.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    (hex::encode(sha), key, out)
}

fn random_key() -> [u8; 32] {
    let mut k = [0u8; 32];
    getrandom::getrandom(&mut k).expect("system randomness");
    k
}

impl Engine {
    fn file_item(&self, item: &str) -> R<(&crate::engine::ItemState, FileDoc)> {
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
        Ok(random_key())
    }

    /// Stores `plain` locally and, in a relay-synced Space, queues its encrypted copy.
    fn put_chunk(
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
        let (sha, key, sealed) = seal_chunk(file_key, plain);
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
        let item = new_id("it");
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

    /// Stores an ordinary file and posts its attachment as one durable operation.
    /// A receipt binds retries to the original intent, including reply/thread scope.
    #[allow(clippy::too_many_arguments)]
    pub fn send_file_message(
        &mut self,
        space: &str,
        name: &str,
        mime: &str,
        bytes: &[u8],
        thumbnail: Option<&[u8]>,
        reply_to: Option<&str>,
        thread: bool,
        mutation_id: &str,
    ) -> R<TimelineEntry> {
        self.send_file_message_with_caption(
            space,
            name,
            mime,
            bytes,
            thumbnail,
            reply_to,
            thread,
            mutation_id,
            "",
        )
    }

    /// The caption and file share one signed message and one retry receipt.
    #[allow(clippy::too_many_arguments)]
    pub fn send_file_message_with_caption(
        &mut self,
        space: &str,
        name: &str,
        mime: &str,
        bytes: &[u8],
        thumbnail: Option<&[u8]>,
        reply_to: Option<&str>,
        thread: bool,
        mutation_id: &str,
        caption: &str,
    ) -> R<TimelineEntry> {
        let me = self.me_id()?;
        if !self.state.writer(space, &me) {
            return Err(CoreError::Forbidden {
                reason: t("só autores enviam arquivos", "only writers can send files"),
            });
        }
        if mutation_id.is_empty()
            || mutation_id.len() > 128
            || !mutation_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            || name.trim().is_empty()
            || name.len() > 1024
            || name.contains(['/', '\\', '\0'])
            || mime.is_empty()
            || mime.len() > 255
            || !mime.contains('/')
            || mime.chars().any(char::is_control)
            || bytes.is_empty()
            || bytes.len() > MAX_FILE_BYTES
            || thumbnail.is_some_and(|t| t.is_empty() || t.len() > MAX_THUMBNAIL_BYTES)
            || caption.len() > MAX_FILE_CAPTION_BYTES
            || caption.contains('\0')
            || (thread && reply_to.is_none())
        {
            return Err(invalid("envio de arquivo inválido", "invalid file send"));
        }
        // Resolve the target before file keys or chunks are written. Never fall back
        // to the main conversation when a thread target disappeared or is foreign.
        let reply = reply_to
            .map(|to| self.reply_ref(space, to, thread))
            .transpose()?;
        let caption = if caption.trim().is_empty() {
            ""
        } else {
            caption
        };
        let file_intent = (
            space,
            name,
            mime,
            hex::encode(Sha256::digest(bytes)),
            thumbnail.map(|t| hex::encode(Sha256::digest(t))),
            reply_to,
            thread,
        );
        // Preserve the original blank-caption fingerprint for accepted sends
        // made before caption support. Nonempty captions cannot reuse it.
        let intent = if caption.is_empty() {
            serde_json::to_vec(&file_intent)
        } else {
            serde_json::to_vec(&(file_intent, caption))
        }
        .map_err(|_| invalid_file())?;
        let fingerprint = hex::encode(Sha256::digest(&intent));
        let key = format!("file_send:{me}:{mutation_id}");
        if let Some(receipt) = self.store.meta(&key)? {
            let (accepted, message): (String, String) =
                serde_json::from_str(&receipt).map_err(|_| invalid_file())?;
            if accepted != fingerprint {
                return Err(invalid(
                    "identificador de envio já utilizado",
                    "send identifier already used",
                ));
            }
            let entry = self
                .space_state(space)?
                .entries
                .iter()
                .find(|entry| entry.id() == message)
                .ok_or_else(invalid_file)?;
            return Ok(self.entry_dto(entry));
        }
        self.atomic_agent_action(|engine| {
            let item = engine.file_add(space, name, name, mime, bytes, thumbnail)?;
            let posted = engine.append(
                space,
                &me,
                EventBody::MessagePosted {
                    message: new_id("msg"),
                    text: caption.to_string(),
                    attaches: Some(item),
                    reply,
                    forwarded: false,
                },
            )?;
            // Timeline identity is the signed event client ID, not MessagePosted.message.
            let message = if posted.client_id.is_empty() {
                posted.hash
            } else {
                posted.client_id
            };
            let receipt =
                serde_json::to_string(&(fingerprint, &message)).map_err(|_| invalid_file())?;
            engine.store.set_meta(&key, &receipt)?;
            let entry = engine
                .space_state(space)?
                .entries
                .iter()
                .find(|entry| entry.id() == message)
                .ok_or_else(invalid_file)?;
            Ok(engine.entry_dto(entry))
        })
    }

    /// Sends a voice note: the audio as an end-to-end encrypted file Item (the same chunked
    /// path as any file, so every client, the web included, can fetch and play it), tagged
    /// with its length, waveform and transcript. With `reply_to` it answers that message
    /// (inline, or in its thread).
    #[allow(clippy::too_many_arguments)]
    pub fn send_voice_note(
        &mut self,
        space: &str,
        bytes: &[u8],
        mime: &str,
        ms: u64,
        levels: &[f32],
        transcript: &str,
        reply_to: Option<&str>,
        thread: bool,
    ) -> R<TimelineEntry> {
        if !mime.starts_with("audio/") {
            return Err(invalid("não é um áudio", "not audio"));
        }
        let me = self.me_id()?;
        if !self.state.writer(space, &me) {
            return Err(CoreError::Forbidden {
                reason: t(
                    "só membros mandam mensagens de voz",
                    "only members can send voice messages",
                ),
            });
        }
        let reply = reply_to
            .map(|to| self.reply_ref(space, to, thread))
            .transpose()?;
        self.create_voice_note(
            space,
            bytes,
            mime,
            VoiceMeta {
                ms,
                levels: VoiceMeta::encode(levels),
                transcript: transcript.trim().to_string(),
                reply,
                forwarded: false,
            },
        )
    }

    /// Copies one current verified file, freshly encrypted for its new audience.
    pub(crate) fn forward_file_message(
        &mut self,
        from_space: &str,
        item_id: &str,
        to_space: &str,
        caption: &str,
        voice: Option<&VoiceNoteDto>,
    ) -> R<TimelineEntry> {
        let me = self.me_id()?;
        if !self.is_member(from_space, &me) || !self.state.writer(to_space, &me) {
            return Err(CoreError::Forbidden {
                reason: t(
                    "encaminhamento de arquivo não autorizado",
                    "file forwarding is not authorized",
                ),
            });
        }
        let (item, file) = self.file_item(item_id)?;
        if item.space != from_space || file.is_voice() != voice.is_some() {
            return Err(invalid_file());
        }
        if voice.is_some_and(|voice| {
            voice.item_id != item_id || file.sha256 != voice.sha256 || file.mime != voice.mime
        }) {
            return Err(invalid_file());
        }
        let version = item.current().number;
        let bytes = self
            .file_bytes_bounded(item_id, Some(version), MAX_FILE_BYTES as u64)?
            .ok_or_else(|| {
                invalid(
                    "o arquivo ainda está chegando; tente novamente quando estiver disponível",
                    "the file is still arriving; try again when it is available",
                )
            })?;
        let thumbnail = match file.thumb.as_ref() {
            Some(chunk) => Some(
                self.file_chunk_bytes(chunk, MAX_THUMBNAIL_BYTES)?
                    .ok_or_else(|| {
                        invalid(
                            "a prévia do arquivo ainda está chegando; tente novamente quando estiver disponível",
                            "the file preview is still arriving; try again when it is available",
                        )
                    })?,
            ),
            None => None,
        };
        // Keys, chunks, upload queue and the signed file/message commit together.
        // On failure the shared atomic helper also restores the in-memory projection.
        self.atomic_agent_action(|engine| {
            if let Some(voice) = voice {
                return engine.create_voice_note(
                    to_space,
                    &bytes,
                    &file.mime,
                    VoiceMeta {
                        ms: voice.ms,
                        levels: VoiceMeta::encode(&voice.levels),
                        transcript: voice.transcript.clone(),
                        reply: None,
                        forwarded: true,
                    },
                );
            }
            // The destination starts at its own root. Treating a filename as a path
            // could turn ordinary audio named "VoiceNotes/…" into a legacy voice note.
            let item = engine.file_add(
                to_space,
                "",
                &file.name,
                &file.mime,
                &bytes,
                thumbnail.as_deref(),
            )?;
            let posted = engine.append(
                to_space,
                &me,
                EventBody::MessagePosted {
                    message: new_id("msg"),
                    text: caption.to_string(),
                    attaches: Some(item),
                    reply: None,
                    forwarded: true,
                },
            )?;
            let message = if posted.client_id.is_empty() {
                posted.hash
            } else {
                posted.client_id
            };
            let entry = engine
                .space_state(to_space)?
                .entries
                .iter()
                .find(|entry| entry.id() == message)
                .ok_or_else(invalid_file)?;
            Ok(engine.entry_dto(entry))
        })
    }

    fn create_voice_note(
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

    fn file_edit_status(
        &self,
        item: &str,
        expected_version: u32,
        expected_sha256: &str,
    ) -> R<FileEditStatus> {
        let me = self.me_id()?;
        let (it, current) = self.file_item(item)?;
        if !self.state.writer(&it.space, &me) || !it.voice_edit_allowed(&me, None) {
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

    fn file_new_version_inner(
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
        if !self.state.writer(&space, &me) || !it.voice_edit_allowed(&me, None) {
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

    /// The file's bytes at `version` (latest when `None`), if every chunk is here.
    pub fn file_bytes(&self, item: &str, version: Option<u32>) -> R<Option<Vec<u8>>> {
        self.file_bytes_bounded(item, version, MAX_FILE_BYTES as u64)
    }

    fn file_doc_at(&self, item: &str, version: Option<u32>) -> R<&FileDoc> {
        let it = self
            .state
            .items
            .get(item)
            .ok_or_else(|| CoreError::NotFound {
                what: "Item".into(),
            })?;
        if matches!(&it.current().content, ItemContent::App(_)) {
            self.app_document_identity(&it.space)?;
        }
        let v = match version {
            Some(n) => it.versions.iter().find(|v| v.number == n),
            None => it.versions.last(),
        };
        let Some(f) = v.and_then(|v| v.content.owned_file()) else {
            return Err(invalid(
                "este Item não é um arquivo",
                "this Item is not a file",
            ));
        };
        Ok(f)
    }

    pub fn file_info_at(&self, item: &str, version: Option<u32>) -> R<FileDto> {
        let file = self.file_doc_at(item, version)?;
        validate_file(file, MAX_FILE_BYTES as u64)?;
        Ok(self.file_dto(file))
    }

    fn file_chunk_bytes(&self, chunk: &ChunkRef, max_bytes: usize) -> R<Option<Vec<u8>>> {
        let expected = validate_chunk(chunk, max_bytes)?;
        let bytes = match self.store.media_bytes_bounded(&chunk.sha256, expected) {
            Ok(Some(bytes)) => bytes,
            Ok(None) => return Ok(None),
            Err(store::StoreError::MediaTooLarge) => return Err(invalid_file()),
            Err(err) => return Err(err.into()),
        };
        if bytes.len() != expected || hex::encode(Sha256::digest(&bytes)) != chunk.sha256 {
            return Err(invalid_file());
        }
        Ok(Some(bytes))
    }

    pub fn file_bytes_bounded(
        &self,
        item: &str,
        version: Option<u32>,
        max_bytes: u64,
    ) -> R<Option<Vec<u8>>> {
        let f = self.file_doc_at(item, version)?;
        self.owned_file_bytes(f, max_bytes)
    }

    pub(crate) fn owned_file_bytes(&self, f: &FileDoc, max_bytes: u64) -> R<Option<Vec<u8>>> {
        let size = validate_file(f, max_bytes)?;
        // Missing or oversized cached pieces cannot trigger a file-sized allocation.
        for chunk in &f.chunks {
            match self.store.media_bytes_len(&chunk.sha256)? {
                None => return Ok(None),
                Some(len) if len == chunk.bytes => {}
                Some(_) => return Err(invalid_file()),
            }
        }
        let mut out = Vec::new();
        out.try_reserve_exact(size).map_err(|_| invalid_file())?;
        for c in &f.chunks {
            match self.file_chunk_bytes(c, MAX_CHUNK)? {
                Some(b) => out.extend_from_slice(&b),
                None => return Ok(None),
            }
        }
        if out.len() != size || hex::encode(Sha256::digest(&out)) != f.sha256 {
            return Err(invalid_file());
        }
        Ok(Some(out))
    }

    pub fn file_thumbnail(&self, item: &str) -> R<Option<Vec<u8>>> {
        let f = self.file_doc_at(item, None)?;
        validate_file(f, MAX_FILE_BYTES as u64)?;
        match &f.thumb {
            Some(t) => self.file_chunk_bytes(t, MAX_THUMBNAIL_BYTES),
            None => Ok(None),
        }
    }

    pub(crate) fn file_dto(&self, f: &FileDoc) -> FileDto {
        let here = f
            .chunks
            .iter()
            .filter(|c| self.store.media_bytes_len(&c.sha256).ok().flatten() == Some(c.bytes))
            .count() as u32;
        FileDto {
            name: f.name.clone(),
            path: f.path.clone(),
            mime: f.mime.clone(),
            bytes: f.bytes,
            sha256: f.sha256.clone(),
            ready: here as usize == f.chunks.len(),
            chunks: f.chunks.len() as u32,
            chunks_here: here,
            has_thumbnail: f.thumb.is_some(),
        }
    }

    /// A demand never inherits authority from JS or an old account. Recheck the
    /// exact canonical manifest and current read membership on every worker pass.
    pub(crate) fn file_demand_key(
        &self,
        item: &str,
        version: u32,
        max_bytes: u64,
    ) -> R<crate::file_demand::FileDemandKey> {
        let account = self
            .account()
            .filter(|account| {
                self.net.author.as_ref().is_some_and(|author| {
                    author.identity == account.identity
                        && author.device.as_deref() == Some(account.device.as_str())
                }) && self.me.as_deref() == Some(account.identity.as_str())
                    && !self.provider_setup_pending()
            })
            .ok_or_else(|| CoreError::Forbidden {
                reason: t(
                    "Entre novamente para baixar este arquivo.",
                    "Sign in again to download this file.",
                ),
            })?;
        let state = self.item_state(item)?;
        if !self.is_member(&state.space, &account.identity) {
            return Err(CoreError::Forbidden {
                reason: t(
                    "Você não tem mais acesso a esta conversa.",
                    "You no longer have access to this chat.",
                ),
            });
        }
        let file = self.file_doc_at(item, Some(version))?;
        if max_bytes == 0 || max_bytes > MAX_FILE_BYTES as u64 {
            return Err(invalid_file());
        }
        validate_file(file, max_bytes)?;
        Ok(crate::file_demand::FileDemandKey {
            item: item.into(),
            version,
            sha256: file.sha256.clone(),
            identity: account.identity.clone(),
            device: account.device.clone(),
        })
    }

    pub(crate) fn file_demand_info(
        &self,
        key: &crate::file_demand::FileDemandKey,
        max_bytes: u64,
    ) -> R<FileDto> {
        if self.file_demand_key(&key.item, key.version, max_bytes)? != *key {
            return Err(CoreError::Stale {
                reason: t(
                    "Este arquivo mudou. Abra a versão novamente.",
                    "This file changed. Open the version again.",
                ),
            });
        }
        self.file_info_at(&key.item, Some(key.version))
    }

    fn demanded_file_blobs(&self) -> Vec<Wanted> {
        let mut out = Vec::new();
        for (key, bound) in self.net.file_demands.active() {
            let Ok(info) = self.file_demand_info(&key, bound) else {
                self.net.file_demands.remove(&key);
                continue;
            };
            if info.ready {
                self.net.file_demands.remove(&key);
                continue;
            }
            let item = &self.state.items[&key.item];
            if !self.net.synced.contains(&item.space) {
                continue;
            }
            let file = self
                .file_doc_at(&key.item, Some(key.version))
                .expect("validated canonical file");
            for chunk in &file.chunks {
                if self.store.has_media(&chunk.sha256).unwrap_or(false) {
                    continue;
                }
                out.push(Wanted {
                    plain: chunk.sha256.clone(),
                    blob: chunk.blob.clone(),
                    key: chunk.key.clone(),
                    mime: "application/octet-stream".into(),
                    space: item.space.clone(),
                    profile: None,
                    maximum_sealed_bytes: Some(chunk.bytes + 40),
                });
            }
        }
        out
    }

    /// Chunks, previews and page blobs that relay-synced Spaces show and this device lacks.
    pub(crate) fn wanted_item_blobs(&self) -> Vec<Wanted> {
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for it in self.state.items.values() {
            if !self.net.synced.contains(&it.space) {
                continue;
            }
            if matches!(&it.current().content, ItemContent::App(_))
                && !self
                    .me
                    .as_deref()
                    .is_some_and(|me| self.is_member(&it.space, me))
            {
                continue;
            }
            let mut refs: Vec<(&ChunkRef, &str)> = Vec::new();
            match it.versions.last().map(|v| &v.content) {
                Some(content) if content.owned_file().is_some() => {
                    let f = content.owned_file().expect("matched owned file");
                    if validate_file(f, MAX_FILE_BYTES as u64).is_err() {
                        continue;
                    }
                    if let Some(t) = &f.thumb {
                        refs.push((t, "image/png"));
                    }
                    for c in &f.chunks {
                        refs.push((c, "application/octet-stream"));
                    }
                }
                Some(ItemContent::Page(_)) => {
                    for v in &it.versions {
                        if let ItemContent::Page(p) = &v.content {
                            if let Some(b) = &p.blob {
                                refs.push((b, "application/vnd.zoen.page"));
                            }
                        }
                    }
                }
                _ => {}
            }
            for (c, mime) in refs {
                let Some(maximum_sealed_bytes) = c.bytes.checked_add(40) else {
                    continue;
                };
                if !seen.insert(c.sha256.clone()) {
                    continue;
                }
                if self.store.has_media(&c.sha256).unwrap_or(false) {
                    continue;
                }
                out.push(Wanted {
                    plain: c.sha256.clone(),
                    blob: c.blob.clone(),
                    key: c.key.clone(),
                    mime: mime.into(),
                    space: it.space.clone(),
                    profile: None,
                    maximum_sealed_bytes: Some(maximum_sealed_bytes),
                });
            }
        }
        // Explicit, expiring historical requests only; unrelated history stays lazy.
        for wanted in self.demanded_file_blobs() {
            if seen.insert(wanted.plain.clone()) {
                out.push(wanted);
            }
        }
        out
    }

    /// Seals a page payload too big for an event as an encrypted blob.
    pub(crate) fn page_blob(&mut self, space: &str, bytes: &[u8]) -> R<ChunkRef> {
        let key = random_key();
        self.put_chunk(
            space,
            &key,
            bytes,
            "application/vnd.zoen.page",
            &HashMap::new(),
        )
    }
}
