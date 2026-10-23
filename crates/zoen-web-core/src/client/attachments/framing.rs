//! Bounded single-file and album framing; no publication or retained state.
use super::*;
use zeroize::Zeroizing;

const MAX_HEADER_BYTES: usize = 256 << 10;
const MAX_FRAMED_BYTES: usize = MAX_FILE_BYTES + (2 << 20) + MAX_HEADER_BYTES;
const MAX_BATCH_FRAMED_BYTES: usize = MAX_FILE_BYTES + 10 * ((2 << 20) + MAX_HEADER_BYTES + 4);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    doc: types::FileDoc,
    uploads: Vec<Upload>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Upload {
    blob: String,
    len: usize,
}

/// The outer framing has exactly one bounded sealed frame for every ordered item.
pub(super) fn unpack_batch(framed: &[u8], count: usize) -> Result<Vec<UnpackedAttachment>, String> {
    if framed.len() > MAX_BATCH_FRAMED_BYTES {
        return Err("Attachment batch too large.".into());
    }
    let mut remaining = framed;
    let mut total = 0u64;
    let mut files = Vec::with_capacity(count);
    for _ in 0..count {
        let (header, tail) = remaining
            .split_first_chunk::<4>()
            .ok_or("Invalid attachment batch framing.")?;
        let length = usize::try_from(u32::from_le_bytes(*header))
            .map_err(|_| "Invalid attachment batch length.")?;
        let (frame, next) = tail
            .split_at_checked(length)
            .ok_or("Invalid attachment batch framing.")?;
        let file = unpack(frame)?;
        total = total
            .checked_add(file.0.bytes)
            .ok_or("Attachment batch too large.")?;
        if total > MAX_FILE_BYTES as u64 {
            return Err("Attachment batch too large.".into());
        }
        files.push(file);
        remaining = next;
    }
    if !remaining.is_empty() {
        return Err("Invalid attachment batch trailing bytes.".into());
    }
    Ok(files)
}

/// Validate the actual sealed frame, not caller-declared size/hash metadata alone.
pub(super) fn unpack(framed: &[u8]) -> Result<UnpackedAttachment, String> {
    if framed.len() < 4 || framed.len() > MAX_FRAMED_BYTES {
        return Err("Invalid attachment payload.".into());
    }
    let (header, tail) = framed
        .split_first_chunk::<4>()
        .ok_or("Invalid attachment payload.")?;
    let length = usize::try_from(u32::from_le_bytes(*header))
        .map_err(|_| "Invalid attachment header length.")?;
    if length > MAX_HEADER_BYTES || length > framed.len() - 4 {
        return Err("Invalid attachment header.".into());
    }
    let (metadata, _) = tail
        .split_at_checked(length)
        .ok_or("Invalid attachment header.")?;
    let Header { doc, uploads } =
        serde_json::from_slice(metadata).map_err(|_| "Invalid attachment header.")?;
    crate::media::validate_owned_file(&doc, MAX_FILE_BYTES)?;
    if !doc.path.is_empty()
        || doc.name.is_empty()
        || doc.name.len() > 255
        || doc.name.contains(['/', '\\', '\0'])
        || !doc.mime.contains('/')
        || doc.mime.len() > 128
        || doc.mime.chars().any(char::is_control)
        || doc.chunks.len() > 65
        || uploads.len() > 66
    {
        return Err("Invalid attachment metadata.".into());
    }
    let wanted = doc
        .chunks
        .iter()
        .chain(doc.thumb.iter())
        .map(|chunk| chunk.blob.as_str())
        .collect::<BTreeSet<_>>();
    if uploads.len() != wanted.len() {
        return Err("Invalid attachment chunks.".into());
    }
    let mut chunks = BTreeMap::new();
    let mut offset = 4 + length;
    for upload in uploads {
        if !wanted.contains(upload.blob.as_str())
            || chunks.contains_key(&upload.blob)
            || upload.len > framed.len() - offset
        {
            return Err("Invalid attachment chunks.".into());
        }
        chunks.insert(upload.blob, framed[offset..offset + upload.len].to_vec());
        offset += upload.len;
    }
    if offset != framed.len() {
        return Err("Invalid attachment trailing bytes.".into());
    }
    let mut digest = Sha256::new();
    let mut actual_bytes = 0usize;
    for chunk in &doc.chunks {
        let plain = Zeroizing::new(crate::media::open_chunk(chunk, &chunks[&chunk.blob])?);
        if plain.len() as u64 != chunk.bytes || plain.len() > MAX_FILE_BYTES - actual_bytes {
            return Err("Attachment chunk length does not match its manifest.".into());
        }
        actual_bytes += plain.len();
        digest.update(plain.as_slice());
    }
    if actual_bytes as u64 != doc.bytes || hex::encode(digest.finalize()) != doc.sha256 {
        return Err("Attachment content does not match its manifest.".into());
    }
    if let Some(thumb) = &doc.thumb {
        let plain = Zeroizing::new(crate::media::open_chunk(thumb, &chunks[&thumb.blob])?);
        if plain.len() as u64 != thumb.bytes {
            return Err("Attachment thumbnail length does not match its manifest.".into());
        }
    }
    Ok((doc, chunks))
}
