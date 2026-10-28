//! Typed block construction and semantic/source identity.
use super::{canonical_spans, err, markdown, Block, DocError, Kind, Run, Span};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Serialize, Deserialize)]
struct Fp {
    k: Kind,
    h: String,
}

impl Block {
    /// # Errors
    /// Refuses unavailable entropy when assigning the block identity.
    pub fn new(kind: Kind, runs: Vec<Run>) -> Result<Block, DocError> {
        Self::new_with(kind, runs, getrandom::getrandom)
    }

    fn new_with(
        kind: Kind,
        runs: Vec<Run>,
        fill: impl FnOnce(&mut [u8]) -> Result<(), getrandom::Error>,
    ) -> Result<Block, DocError> {
        Ok(Block {
            id: new_id_with(fill)?,
            kind,
            runs,
            src: String::new(),
            tail: String::new(),
            lead: String::new(),
            fp: String::new(),
        })
    }

    /// The kind plus a hash of the canonical inline Markdown: equal fingerprints mean the
    /// block reads the same.
    /// # Errors
    /// Returns an error if the source fingerprint cannot be serialized.
    pub fn fingerprint(&self) -> Result<String, DocError> {
        let md = markdown::inline_md(&self.runs);
        serde_json::to_string(&Fp {
            k: self.kind.clone(),
            h: hex::encode(&Sha256::digest(md.as_bytes())[..12]),
        })
        .map_err(err)
    }

    pub(crate) fn kind_from_fp(fp: &str) -> Option<Kind> {
        serde_json::from_str::<Fp>(fp).ok().map(|f| f.k)
    }

    #[must_use]
    pub fn plain_text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }

    /// SHA-256 of the visible kind, exact text and normalized marks. Source spelling,
    /// run segmentation and the block's identity are not content.
    /// # Errors
    /// Returns an error if canonical block content cannot be serialized.
    pub fn content_hash(&self) -> Result<String, DocError> {
        #[derive(Serialize)]
        struct Content {
            kind: Kind,
            text: String,
            spans: Vec<Span>,
        }
        let kind = match &self.kind {
            Kind::Heading { level, .. } => Kind::Heading {
                level: *level,
                setext: false,
            },
            kind => kind.clone(),
        };
        let bytes = serde_json::to_vec(&Content {
            kind,
            text: self.plain_text(),
            spans: canonical_spans(&self.runs),
        })
        .map_err(err)?;
        Ok(hex::encode(
            Sha256::new()
                .chain_update(b"zoen-page-block-content-v1\0")
                .chain_update(bytes)
                .finalize(),
        ))
    }
}

fn new_id_with(
    fill: impl FnOnce(&mut [u8]) -> Result<(), getrandom::Error>,
) -> Result<String, DocError> {
    let mut b = [0u8; 8];
    fill(&mut b).map_err(err)?;
    Ok(hex::encode(b))
}

#[cfg(test)]
mod tests {
    use super::{Block, Kind};

    #[test]
    fn failed_entropy_never_returns_a_partial_block() {
        let result = Block::new_with(Kind::Paragraph, Vec::new(), |bytes| {
            bytes[0] = 7;
            Err(getrandom::Error::UNSUPPORTED)
        });
        assert!(result.is_err());
    }
}
