//! Bounded physical chunks behind the existing evidence/transfer references.
//! A root is published only after all immutable chunks are known committed.
use super::*;
const PREFIX: &[u8] = b"ZOEN-EVIDENCE-CHUNKS/1\0";
const CHUNK: usize = 64 * 1024;
const MAXIMUM: usize = 32 * 1024 * 1024;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    bytes: usize,
    sha256: String,
    chunks: Vec<String>,
}
impl Execution {
    pub(super) async fn evidence_payload(
        &self,
        attempt: &str,
        digest: &str,
        sealed: &[u8],
    ) -> Result<Vec<u8>, RuntimeError> {
        if sealed.len() > MAXIMUM {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        if sealed.len() <= crate::custody::MAX_SEALED {
            return Ok(sealed.to_vec());
        }
        self.referenced_evidence_payload(attempt, digest, sealed)
            .await
    }

    /// Always retain physical chunks, including small canonical blobs. The
    /// returned manifest contains references only and grants no activation.
    pub(crate) async fn referenced_evidence_payload(
        &self,
        attempt: &str,
        digest: &str,
        sealed: &[u8],
    ) -> Result<Vec<u8>, RuntimeError> {
        if sealed.is_empty() || sealed.len() > MAXIMUM {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        let sha256 = crate::hash(sealed);
        let mut chunks = Vec::with_capacity(sealed.len().div_ceil(CHUNK));
        for (index, chunk) in sealed.chunks(CHUNK).enumerate() {
            let hash = crate::hash(chunk);
            let key = self.root.pack(&(
                "evidence-chunk/1",
                attempt,
                digest,
                sha256.as_str(),
                index as i64,
            ));
            let trx = self.transaction().await?;
            if let Some(old) = trx.get(&key, false).await? {
                if old.as_ref() != chunk {
                    return Err(RuntimeError::InvalidBinding);
                }
            } else {
                trx.set(&key, chunk);
                trx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
            }
            chunks.push(hash);
        }
        let mut root = PREFIX.to_vec();
        root.extend(
            serde_json::to_vec(&Manifest {
                bytes: sealed.len(),
                sha256,
                chunks,
            })
            .map_err(|_| RuntimeError::InvalidBinding)?,
        );
        if root.len() > crate::custody::MAX_SEALED {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        Ok(root)
    }
    pub(super) fn evidence_payload_size(root: &[u8]) -> Result<usize, RuntimeError> {
        if root.len() > crate::custody::MAX_SEALED {
            return Err(RuntimeError::InvalidBinding);
        }
        if !root.starts_with(PREFIX) {
            return Ok(root.len());
        }
        let manifest: Manifest = serde_json::from_slice(&root[PREFIX.len()..])
            .map_err(|_| RuntimeError::InvalidBinding)?;
        if !(1..=MAXIMUM).contains(&manifest.bytes)
            || manifest.chunks.len() != manifest.bytes.div_ceil(CHUNK)
            || std::iter::once(&manifest.sha256)
                .chain(manifest.chunks.iter())
                .any(|v| {
                    v.len() != 64
                        || !v
                            .bytes()
                            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
                })
        {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok(manifest.bytes)
    }
    pub(crate) async fn read_evidence_payload(
        &self,
        attempt: &str,
        digest: &str,
        root: &[u8],
    ) -> Result<Vec<u8>, RuntimeError> {
        Self::evidence_payload_size(root)?;
        if !root.starts_with(PREFIX) {
            if root.len() > crate::custody::MAX_SEALED {
                return Err(RuntimeError::InvalidBinding);
            }
            return Ok(root.to_vec());
        }
        let manifest: Manifest = serde_json::from_slice(&root[PREFIX.len()..])
            .map_err(|_| RuntimeError::InvalidBinding)?;
        if !(1..=MAXIMUM).contains(&manifest.bytes)
            || manifest.chunks.len() != manifest.bytes.div_ceil(CHUNK)
            || manifest.chunks.len() > MAXIMUM / CHUNK
        {
            return Err(RuntimeError::InvalidBinding);
        }
        let mut sealed = Vec::with_capacity(manifest.bytes);
        for (index, hash) in manifest.chunks.iter().enumerate() {
            let key = self.root.pack(&(
                "evidence-chunk/1",
                attempt,
                digest,
                manifest.sha256.as_str(),
                index as i64,
            ));
            let trx = self.transaction().await?;
            let chunk = trx
                .get(&key, false)
                .await?
                .ok_or(RuntimeError::Unavailable)?;
            let expected = CHUNK.min(manifest.bytes - sealed.len());
            if chunk.len() != expected || crate::hash(&chunk) != *hash {
                return Err(RuntimeError::InvalidBinding);
            }
            sealed.extend_from_slice(&chunk);
        }
        if sealed.len() != manifest.bytes || crate::hash(&sealed) != manifest.sha256 {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok(sealed)
    }
}
