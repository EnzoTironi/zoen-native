//! Exact old JSON addresses remain valid. Large sources use a bounded binary
//! frame so ciphertext bytes never expand into a JSON integer array.
use super::*;
pub(in crate::continuity::capture) const SOURCE_MAXIMUM: usize = 64 * 1024 * 1024;
const MAGIC: &[u8] = b"ZOEN-CHAT-ORIGINAL/1\0";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::continuity::capture) struct Source {
    pub(in crate::continuity::capture) attempt: String,
    pub(in crate::continuity::capture) digest: String,
    pub(in crate::continuity::capture) sealed: Vec<u8>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    attempt: String,
    digest: String,
    bytes: u64,
}
impl Source {
    pub(in crate::continuity::capture) fn bytes(&self) -> Result<Vec<u8>, RuntimeError> {
        if self.sealed.len() <= crate::custody::MAX_SEALED {
            return serde_json::to_vec(self).map_err(|_| RuntimeError::InvalidBinding);
        }
        if self.sealed.len() > crate::capture_codec::MAXIMUM {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        let header = serde_json::to_vec(&Header {
            attempt: self.attempt.clone(),
            digest: self.digest.clone(),
            bytes: u64::try_from(self.sealed.len()).map_err(|_| RuntimeError::EvidenceTooLarge)?,
        })
        .map_err(|_| RuntimeError::InvalidBinding)?;
        if header.len() > 16_384 {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        let mut bytes = Vec::with_capacity(MAGIC.len() + 4 + header.len() + self.sealed.len());
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(
            &u32::try_from(header.len())
                .map_err(|_| RuntimeError::EvidenceTooLarge)?
                .to_be_bytes(),
        );
        bytes.extend_from_slice(&header);
        bytes.extend_from_slice(&self.sealed);
        Ok(bytes)
    }
    pub(in crate::continuity::capture) fn decode(bytes: &[u8]) -> Result<Self, RuntimeError> {
        if bytes.len() > SOURCE_MAXIMUM {
            return Err(RuntimeError::EvidenceTooLarge);
        }
        if !bytes.starts_with(MAGIC) {
            if bytes.len() > 512 * 1024 {
                return Err(RuntimeError::InvalidBinding);
            }
            return serde_json::from_slice(bytes).map_err(|_| RuntimeError::InvalidBinding);
        }
        let split = MAGIC.len() + 4;
        let length = usize::try_from(u32::from_be_bytes(
            bytes
                .get(MAGIC.len()..split)
                .ok_or(RuntimeError::InvalidBinding)?
                .try_into()
                .map_err(|_| RuntimeError::InvalidBinding)?,
        ))
        .map_err(|_| RuntimeError::InvalidBinding)?;
        if length > 16_384 {
            return Err(RuntimeError::InvalidBinding);
        }
        let end = split
            .checked_add(length)
            .ok_or(RuntimeError::InvalidBinding)?;
        let header: Header =
            serde_json::from_slice(bytes.get(split..end).ok_or(RuntimeError::InvalidBinding)?)
                .map_err(|_| RuntimeError::InvalidBinding)?;
        let sealed = bytes.get(end..).ok_or(RuntimeError::InvalidBinding)?;
        if sealed.len() > crate::capture_codec::MAXIMUM
            || u64::try_from(sealed.len()).map_err(|_| RuntimeError::InvalidBinding)?
                != header.bytes
        {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok(Self {
            attempt: header.attempt,
            digest: header.digest,
            sealed: sealed.to_vec(),
        })
    }
    pub(in crate::continuity::capture) fn read(
        managed: &ManagedRuntime,
        key: &str,
    ) -> Result<Self, RuntimeError> {
        Self::decode(
            &managed
                .anchor
                .reader()
                .retained_original_bounded(key, SOURCE_MAXIMUM)
                .map_err(error)?,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_sources_keep_exact_addresses_and_large_sources_are_binary() -> Result<(), RuntimeError> {
        let small = Source {
            attempt: "attempt".into(),
            digest: "a".repeat(64),
            sealed: vec![7; 100],
        };
        let legacy = serde_json::to_vec(&small).map_err(|_| RuntimeError::InvalidBinding)?;
        assert_eq!(small.bytes()?, legacy);
        assert_eq!(Source::decode(&legacy)?.sealed, small.sealed);
        let large = Source {
            attempt: "attempt".into(),
            digest: "b".repeat(64),
            sealed: vec![7; 1024 * 1024],
        };
        let binary = large.bytes()?;
        assert!(binary.starts_with(MAGIC));
        assert!(binary.len() < large.sealed.len() + 4096);
        assert_eq!(Source::decode(&binary)?.sealed, large.sealed);
        assert!(Source::decode(&binary[..binary.len() - 1]).is_err());
        let mut trailing = binary;
        trailing.push(0);
        assert!(Source::decode(&trailing).is_err());
        Ok(())
    }
}
