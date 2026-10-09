//! Files as Items (ADR 0027): bytes cut into content-defined chunks, each encrypted for
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
use roda_types::*;
use sha2::{Digest, Sha256};

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

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
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

fn invalid(pt: &str, en: &str) -> CoreError {
    CoreError::Invalid { reason: t(pt, en) }
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
    fn file_key(&mut self, item: Option<&str>) -> R<[u8; 32]> {
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

    fn file_doc(
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
            Some(t) if !t.is_empty() && t.len() <= 2 << 20 => {
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

    /// A new version of a file (an edit, a markup, a trim). Unchanged chunks are re-used.
    pub fn file_new_version(
        &mut self,
        item: &str,
        bytes: &[u8],
        thumbnail: Option<&[u8]>,
        note: &str,
    ) -> R<()> {
        let me = self.me_id()?;
        let (it, cur) = self.file_item(item)?;
        let space = it.space.clone();
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
        let doc = self.file_doc(
            &space, &key, &cur.name, &cur.path, &cur.mime, bytes, thumbnail, &prior,
        )?;
        if doc.sha256 == cur.sha256
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
        let it = self
            .state
            .items
            .get(item)
            .ok_or_else(|| CoreError::NotFound {
                what: "Item".into(),
            })?;
        let v = match version {
            Some(n) => it.versions.iter().find(|v| v.number == n),
            None => it.versions.last(),
        };
        let Some(ItemContent::File(f)) = v.map(|v| &v.content) else {
            return Err(invalid(
                "este Item não é um arquivo",
                "this Item is not a file",
            ));
        };
        let mut out = Vec::with_capacity(f.bytes as usize);
        for c in &f.chunks {
            match self.media(&c.sha256)? {
                Some(b) => out.extend_from_slice(&b),
                None => return Ok(None),
            }
        }
        if hex::encode(Sha256::digest(&out)) != f.sha256 {
            return Ok(None);
        }
        Ok(Some(out))
    }

    pub fn file_thumbnail(&self, item: &str) -> R<Option<Vec<u8>>> {
        let (_, f) = self.file_item(item)?;
        match &f.thumb {
            Some(t) => self.media(&t.sha256),
            None => Ok(None),
        }
    }

    pub(crate) fn file_dto(&self, f: &FileDoc) -> FileDto {
        let here = f
            .chunks
            .iter()
            .filter(|c| self.store.has_media(&c.sha256).unwrap_or(false))
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

    /// Chunks, previews and page blobs that relay-synced Spaces show and this device lacks.
    pub(crate) fn wanted_item_blobs(&self) -> Vec<Wanted> {
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for it in self.state.items.values() {
            if !self.net.synced.contains(&it.space) {
                continue;
            }
            let mut refs: Vec<(&ChunkRef, &str)> = Vec::new();
            match it.versions.last().map(|v| &v.content) {
                Some(ItemContent::File(f)) => {
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
                });
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
