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

#[cfg(test)]
thread_local! {
    /// Counts actual historical fallback comparisons, not elapsed test time.
    pub(crate) static FILE_VERSION_HISTORY_PROBES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

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
fn seal_chunk(file_key: &[u8; 32], plain: &[u8]) -> R<(String, [u8; 32], Vec<u8>)> {
    let sha = Sha256::digest(plain);
    let (key, nonce) = chunk_secret(file_key, &sha);
    let cipher = XChaCha20Poly1305::new_from_slice(&key).map_err(|_| invalid_file())?;
    let ct = cipher
        .encrypt(&XNonce::from(nonce), plain)
        .map_err(|_| invalid_file())?;
    let mut out = Vec::with_capacity(24 + ct.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    Ok((hex::encode(sha), key, out))
}

fn random_key() -> R<[u8; 32]> {
    let mut k = [0u8; 32];
    getrandom::getrandom(&mut k).map_err(|_| invalid_file())?;
    Ok(k)
}

mod native;
pub use native::NativeFileBlob;
mod demand;
mod editing;
mod publication;
mod reading;
mod storage;
