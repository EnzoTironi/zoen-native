//! Files, photos, videos and voice notes, sealed exactly like native (ADR 0040).
//!
//! The bytes are cut into content-defined chunks; each chunk is encrypted under a key
//! derived from the file's own random key and the chunk's hash, stored on the relay under
//! the sha256 of its ciphertext, and named in the signed `ItemCreated` event that travels
//! inside MLS. Readers fetch the ciphertext, check both hashes and open it with the key.
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    XChaCha20Poly1305, XNonce,
};
use hkdf::Hkdf;
use serde::Serialize;
use sha2::{Digest, Sha256};
use types::{ChunkRef, FileDoc};

const MIN_CHUNK: usize = 1 << 20;
const AVG_CHUNK: usize = 4 << 20;
const MAX_CHUNK: usize = 16 << 20;
pub const MAX_FILE_BYTES: usize = 512 << 20;
const MAX_THUMBNAIL_BYTES: usize = 2 << 20;

#[derive(Serialize)]
pub struct Upload {
    pub blob: String,
    pub sealed: Vec<u8>,
}

#[derive(Serialize)]
pub struct Sealed {
    pub doc: FileDoc,
    pub uploads: Vec<Upload>,
}

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

fn seal_chunk(file_key: &[u8; 32], plain: &[u8], uploads: &mut Vec<Upload>) -> ChunkRef {
    let sha = Sha256::digest(plain);
    let (key, nonce) = chunk_secret(file_key, &sha);
    let cipher = XChaCha20Poly1305::new_from_slice(&key).expect("32-byte key");
    let ct = cipher
        .encrypt(&XNonce::from(nonce), plain)
        .expect("encrypt");
    let mut sealed = Vec::with_capacity(24 + ct.len());
    sealed.extend_from_slice(&nonce);
    sealed.extend_from_slice(&ct);
    let blob = hex::encode(Sha256::digest(&sealed));
    if !uploads.iter().any(|u| u.blob == blob) {
        uploads.push(Upload {
            blob: blob.clone(),
            sealed,
        });
    }
    ChunkRef {
        sha256: hex::encode(sha),
        bytes: plain.len() as u64,
        blob,
        key: hex::encode(key),
    }
}

/// One page payload, with the same authenticated chunk format as native pages.
pub(crate) fn seal_page(bytes: &[u8]) -> Result<(ChunkRef, Upload), String> {
    if bytes.is_empty() || bytes.len() > 32 << 20 {
        return Err("page payload empty or too large".into());
    }
    let mut key = [0; 32];
    getrandom::getrandom(&mut key).map_err(|error| error.to_string())?;
    let mut uploads = Vec::new();
    let reference = seal_chunk(&key, bytes, &mut uploads);
    Ok((reference, uploads.remove(0)))
}

/// Seals a file for a relay-synced chat. The caller uploads every `Upload`, then queues
/// the `ItemCreated` carrying `doc`.
pub fn seal_file(
    name: &str,
    mime: &str,
    bytes: &[u8],
    thumb: Option<&[u8]>,
) -> Result<Sealed, String> {
    if bytes.is_empty() || bytes.len() > MAX_FILE_BYTES {
        return Err("file empty or too large".into());
    }
    if name.is_empty()
        || name.len() > 255
        || name.contains(['/', '\\', '\0'])
        || !mime.contains('/')
        || mime.len() > 128
    {
        return Err("invalid file name or type".into());
    }
    let mut file_key = [0u8; 32];
    getrandom::getrandom(&mut file_key).map_err(|e| e.to_string())?;
    let mut uploads = Vec::new();
    let chunks = fastcdc::v2020::FastCDC::new(bytes, MIN_CHUNK, AVG_CHUNK, MAX_CHUNK)
        .map(|c| {
            seal_chunk(
                &file_key,
                &bytes[c.offset..c.offset + c.length],
                &mut uploads,
            )
        })
        .collect();
    let thumb = thumb
        .filter(|t| !t.is_empty() && t.len() <= MAX_THUMBNAIL_BYTES)
        .map(|t| seal_chunk(&file_key, t, &mut uploads));
    Ok(Sealed {
        doc: FileDoc {
            name: name.into(),
            path: String::new(),
            mime: mime.into(),
            bytes: bytes.len() as u64,
            sha256: hex::encode(Sha256::digest(bytes)),
            chunks,
            thumb,
            voice: None,
        },
        uploads,
    })
}

/// Reuse already-signed immutable chunks when their plaintext is unchanged.
/// Every fresh chunk still uses the same Rust encryption and FileDoc format.
pub(crate) fn seal_file_reusing(
    name: &str,
    mime: &str,
    bytes: &[u8],
    prior: &[ChunkRef],
) -> Result<Sealed, String> {
    let mut sealed = seal_file(name, mime, bytes, None)?;
    for chunk in &mut sealed.doc.chunks {
        if let Some(old) = prior
            .iter()
            .find(|old| old.sha256 == chunk.sha256 && old.bytes == chunk.bytes)
        {
            *chunk = old.clone();
        }
    }
    sealed.uploads.retain(|upload| {
        sealed
            .doc
            .chunks
            .iter()
            .any(|chunk| chunk.blob == upload.blob)
    });
    Ok(sealed)
}

pub(crate) fn validate_owned_file(file: &FileDoc, max_bytes: usize) -> Result<(), String> {
    let valid_hash =
        |value: &str| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit());
    if file.bytes == 0
        || file.bytes > max_bytes as u64
        || file.chunks.is_empty()
        || file.chunks.len() > MAX_FILE_BYTES / MIN_CHUNK + 1
        || !valid_hash(&file.sha256)
        || file.voice.is_some()
    {
        return Err("invalid owned file".into());
    }
    let metadata = [&file.name, &file.path, &file.mime, &file.sha256]
        .iter()
        .try_fold(0usize, |size, value| size.checked_add(value.len()))
        .and_then(|size| {
            size.checked_add((file.chunks.len() + usize::from(file.thumb.is_some())) * (3 * 64 + 8))
        })
        .ok_or("invalid owned file")?;
    if metadata > 128 << 10 {
        return Err("invalid owned file".into());
    }
    let mut total = 0u64;
    for chunk in &file.chunks {
        if chunk.bytes == 0
            || chunk.bytes > MAX_CHUNK as u64
            || !valid_hash(&chunk.sha256)
            || !valid_hash(&chunk.blob)
            || !valid_hash(&chunk.key)
        {
            return Err("invalid owned file".into());
        }
        total = total.checked_add(chunk.bytes).ok_or("invalid owned file")?;
    }
    if total != file.bytes {
        return Err("invalid owned file".into());
    }
    if let Some(chunk) = &file.thumb {
        if chunk.bytes == 0
            || chunk.bytes > MAX_THUMBNAIL_BYTES as u64
            || !valid_hash(&chunk.sha256)
            || !valid_hash(&chunk.blob)
            || !valid_hash(&chunk.key)
        {
            return Err("invalid owned file".into());
        }
    }
    Ok(())
}

/// Opens one downloaded chunk only if it is exactly what the event promised.
pub fn open_chunk(chunk: &ChunkRef, sealed: &[u8]) -> Result<Vec<u8>, String> {
    if hex::encode(Sha256::digest(sealed)) != chunk.blob || sealed.len() < 24 + 16 {
        return Err("The file changed on the way.".into());
    }
    let key = hex::decode(&chunk.key).map_err(|_| "invalid key")?;
    let cipher = XChaCha20Poly1305::new_from_slice(&key).map_err(|_| "invalid key")?;
    let nonce: [u8; 24] = sealed[..24].try_into().map_err(|_| "invalid nonce")?;
    let plain = cipher
        .decrypt(&XNonce::from(nonce), &sealed[24..])
        .map_err(|_| "The file could not be opened.")?;
    if hex::encode(Sha256::digest(&plain)) != chunk.sha256 {
        return Err("The file changed on the way.".into());
    }
    Ok(plain)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sealed_chunks_open_and_tampering_is_refused() {
        let bytes: Vec<u8> = (0..3_000_000u32).map(|i| (i * 7 % 251) as u8).collect();
        let s = seal_file("foto.jpg", "image/jpeg", &bytes, Some(b"thumb")).unwrap();
        assert_eq!(s.doc.bytes, bytes.len() as u64);
        let mut joined = Vec::new();
        for c in &s.doc.chunks {
            let up = s.uploads.iter().find(|u| u.blob == c.blob).unwrap();
            joined.extend(open_chunk(c, &up.sealed).unwrap());
        }
        assert_eq!(joined, bytes);
        let t = s.doc.thumb.as_ref().unwrap();
        let mut bad = s
            .uploads
            .iter()
            .find(|u| u.blob == t.blob)
            .unwrap()
            .sealed
            .clone();
        assert_eq!(open_chunk(t, &bad).unwrap(), b"thumb");
        *bad.last_mut().unwrap() ^= 1;
        assert!(open_chunk(t, &bad).is_err());
        assert!(seal_file("a/b", "image/png", b"x", None).is_err());
    }
}
