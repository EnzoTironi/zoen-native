//! Canonical encrypted FileDoc chunks retained by the existing evidence store.
//! Their authenticated roots are committed in the output activation transaction.
use super::*;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use sha2::{Digest, Sha256};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BlobReference {
    chunk: types::ChunkRef,
    payload: String,
}
fn subject(run: &str) -> String {
    format!("native-image-blob/1/{run}")
}

pub(super) async fn retain(
    runtime: &RuntimeAuthority,
    run: &str,
    blobs: &[client_core::runtime_core::NativeFileBlob],
    file: &types::FileDoc,
) -> Result<Vec<BlobReference>, RuntimeError> {
    if blobs.is_empty() || blobs.len() != file.chunks.len() || blobs.len() > 13 {
        return Err(RuntimeError::InvalidBinding);
    }
    let mut references = Vec::new();
    for (blob, chunk) in blobs.iter().zip(&file.chunks) {
        if blob.reference() != chunk
            || blob.bytes().len() as u64
                != chunk
                    .bytes
                    .checked_add(40)
                    .ok_or(RuntimeError::InvalidBinding)?
            || hash(blob.bytes()) != chunk.blob
        {
            return Err(RuntimeError::InvalidBinding);
        }
        let payload = runtime
            .execution
            .referenced_evidence_payload(&subject(run), &chunk.blob, blob.bytes())
            .await?;
        references.push(BlobReference {
            chunk: chunk.clone(),
            payload: STANDARD.encode(payload),
        });
    }
    Ok(references)
}

/// No provider or financial operation occurs here. Every byte must match the
/// authenticated canonical ciphertext/plaintext references, in original order.
pub(super) async fn read(
    runtime: &RuntimeAuthority,
    run: &str,
    refs: &[BlobReference],
    sha256: &str,
    maximum: u64,
) -> Result<Vec<u8>, RuntimeError> {
    if refs.is_empty() || refs.len() > 13 || maximum > zoen_models::MAX_IMAGE_BYTES {
        return Err(RuntimeError::InvalidBinding);
    }
    let mut plain = Vec::new();
    for reference in refs {
        let chunk = &reference.chunk;
        let next = (plain.len() as u64)
            .checked_add(chunk.bytes)
            .ok_or(RuntimeError::InvalidBinding)?;
        if chunk.bytes == 0
            || next > maximum
            || ![&chunk.blob, &chunk.key, &chunk.sha256]
                .iter()
                .all(|v| digest(v))
            || reference.payload.len() > 32 * 1024
        {
            return Err(RuntimeError::InvalidBinding);
        }
        let payload = STANDARD
            .decode(&reference.payload)
            .map_err(|_| RuntimeError::InvalidBinding)?;
        if !payload.starts_with(b"ZOEN-EVIDENCE-CHUNKS/1\0") {
            return Err(RuntimeError::InvalidBinding);
        }
        let encrypted = runtime
            .execution
            .read_evidence_payload(&subject(run), &chunk.blob, &payload)
            .await?;
        if encrypted.len() as u64
            != chunk
                .bytes
                .checked_add(40)
                .ok_or(RuntimeError::InvalidBinding)?
            || hash(&encrypted) != chunk.blob
        {
            return Err(RuntimeError::InvalidBinding);
        }
        let key = hex::decode(&chunk.key).map_err(|_| RuntimeError::InvalidBinding)?;
        let decoded =
            event_log::media::open(&key, &encrypted).ok_or(RuntimeError::InvalidBinding)?;
        if decoded.len() as u64 != chunk.bytes || hash(&decoded) != chunk.sha256 {
            return Err(RuntimeError::InvalidBinding);
        }
        plain.extend_from_slice(&decoded);
    }
    if hex::encode(Sha256::digest(&plain)) != sha256 {
        return Err(RuntimeError::InvalidBinding);
    }
    Ok(plain)
}

#[cfg(test)]
pub(super) fn first_payload(refs: &[BlobReference]) -> (&str, Vec<u8>) {
    let first = refs.first().unwrap();
    (&first.chunk.blob, STANDARD.decode(&first.payload).unwrap())
}
#[cfg(test)]
pub(super) async fn assert_faults(
    runtime: &RuntimeAuthority,
    run: &str,
    refs: &[BlobReference],
    sha: &str,
) {
    assert!(read(
        runtime,
        "wrong-run",
        refs,
        sha,
        zoen_models::MAX_IMAGE_BYTES
    )
    .await
    .is_err());
    assert!(read(
        runtime,
        run,
        refs,
        &"a".repeat(64),
        zoen_models::MAX_IMAGE_BYTES
    )
    .await
    .is_err());
    assert!(read(runtime, run, refs, sha, 1).await.is_err());
    let bytes = serde_json::to_vec(refs).unwrap();
    for change in 0..4 {
        let mut changed: Vec<BlobReference> = serde_json::from_slice(&bytes).unwrap();
        match change {
            0 => changed[0].chunk.key = "0".repeat(64),
            1 => changed[0].chunk.sha256 = "0".repeat(64),
            2 => changed[0].chunk.bytes += 1,
            _ => changed[0].payload = STANDARD.encode(b"invalid chunk root"),
        }
        assert!(
            read(runtime, run, &changed, sha, zoen_models::MAX_IMAGE_BYTES)
                .await
                .is_err()
        );
    }
}
