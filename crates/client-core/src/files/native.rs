//! The same `FileDoc` and canonical encryption with externally retained bytes.
use super::{
    invalid_file, seal_chunk, ChunkRef, Digest, Engine, FileDoc, Sha256, AVG_CHUNK, MAX_CHUNK,
    MAX_FILE_BYTES, MIN_CHUNK, R,
};

/// Prepared canonical relay ciphertext, never an upload acknowledgement.
pub struct NativeFileBlob {
    reference: ChunkRef,
    bytes: Vec<u8>,
}
impl std::fmt::Debug for NativeFileBlob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NativeFileBlob { redacted }")
    }
}
impl NativeFileBlob {
    #[must_use]
    pub fn reference(&self) -> &ChunkRef {
        &self.reference
    }
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}
impl Engine {
    pub(crate) fn native_file_doc(
        &mut self,
        space: &str,
        file_key: &[u8; 32],
        name: &str,
        bytes: &[u8],
    ) -> R<(FileDoc, Vec<NativeFileBlob>)> {
        if bytes.is_empty() || bytes.len() > MAX_FILE_BYTES || !self.net.synced.contains(space) {
            return Err(invalid_file());
        }
        let mut blobs = Vec::new();
        let mut chunks = Vec::new();
        for chunk in fastcdc::v2020::FastCDC::new(bytes, MIN_CHUNK, AVG_CHUNK, MAX_CHUNK) {
            let (plain, key, sealed) =
                seal_chunk(file_key, &bytes[chunk.offset..chunk.offset + chunk.length])?;
            let blob = hex::encode(Sha256::digest(&sealed));
            let key = hex::encode(key);
            self.store
                .queue_external_media(&plain, &blob, &key, sealed.len() as u64)?;
            let reference = ChunkRef {
                sha256: plain,
                bytes: chunk.length as u64,
                blob,
                key,
            };
            chunks.push(reference.clone());
            blobs.push(NativeFileBlob {
                reference,
                bytes: sealed,
            });
        }
        Ok((
            FileDoc {
                name: name.into(),
                path: name.into(),
                mime: "image/png".into(),
                bytes: bytes.len() as u64,
                sha256: hex::encode(Sha256::digest(bytes)),
                chunks,
                thumb: None,
                voice: None,
            },
            blobs,
        ))
    }
}
