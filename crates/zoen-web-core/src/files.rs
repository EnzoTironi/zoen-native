//! Shared file ciphertext bridge. HTTP belongs to the host: persist an encrypted upload
//! plan, PUT its blobs, then commit its signed Item event. Never publish before upload.
use crate::{Client, Update};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    XChaCha20Poly1305, XNonce,
};
use openmls_traits::signatures::Signer;
use roda_types::{ChunkRef, EventBody, FileDoc, ItemContent, ItemKind, Role};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
const MAX_FILE: usize = 64 << 20;
const CHUNK: usize = 4 << 20;
const MAX_CHUNK: usize = 16 << 20;

fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}
fn validate(doc: &FileDoc) -> Result<(), String> {
    if doc.bytes > MAX_FILE as u64
        || doc.chunks.len() > 65
        || doc.name.is_empty()
        || doc.name.len() > 255
        || doc.name.contains(['/', '\\', '\0'])
        || doc.mime.len() > 128
        || !doc.mime.contains('/')
        || !valid_hash(&doc.sha256)
    {
        return Err("Invalid or oversized file metadata (browser maximum 64 MiB)".into());
    }
    let mut size = 0u64;
    for c in &doc.chunks {
        if c.bytes == 0
            || c.bytes > MAX_CHUNK as u64
            || !valid_hash(&c.sha256)
            || !valid_hash(&c.blob)
            || !valid_hash(&c.key)
        {
            return Err("Invalid chunk metadata".into());
        }
        size = size.checked_add(c.bytes).ok_or("Invalid file size")?;
    }
    if size != doc.bytes {
        return Err("File size does not match chunks".into());
    }
    Ok(())
}
fn open(doc: &FileDoc, packed: &[u8]) -> Result<Vec<u8>, String> {
    validate(doc)?;
    let expected = usize::try_from(doc.bytes)
        .map_err(|_| "Invalid file size")?
        .checked_add(doc.chunks.len() * 40)
        .ok_or("Invalid ciphertext size")?;
    if packed.len() != expected {
        return Err("Ciphertext size does not match file".into());
    }
    let mut plain = Vec::with_capacity(doc.bytes as usize);
    let mut offset = 0;
    for c in &doc.chunks {
        let size = c.bytes as usize + 40;
        let sealed = &packed[offset..offset + size];
        offset += size;
        if hash(sealed) != c.blob {
            return Err("Encrypted blob hash mismatch".into());
        }
        let key = hex::decode(&c.key).map_err(|_| "Invalid chunk key")?;
        let cipher = XChaCha20Poly1305::new_from_slice(&key).map_err(|_| "Invalid chunk key")?;
        let nonce: [u8; 24] = sealed[..24].try_into().map_err(|_| "Invalid chunk nonce")?;
        let bytes = cipher
            .decrypt(&XNonce::from(nonce), &sealed[24..])
            .map_err(|_| "Encrypted chunk authentication failed")?;
        if bytes.len() != c.bytes as usize || hash(&bytes) != c.sha256 {
            return Err("Plaintext chunk hash mismatch".into());
        }
        plain.extend_from_slice(&bytes);
    }
    if hash(&plain) != doc.sha256 {
        return Err("File hash mismatch".into());
    }
    Ok(plain)
}
pub(crate) fn open_chunk(reference: &ChunkRef, sealed: &[u8]) -> Result<Vec<u8>, String> {
    let doc = FileDoc {
        name: "Page blob".into(),
        path: String::new(),
        mime: "application/octet-stream".into(),
        bytes: reference.bytes,
        sha256: reference.sha256.clone(),
        chunks: vec![reference.clone()],
        thumb: None,
        voice: None,
    };
    open(&doc, sealed)
}

#[derive(Serialize, Deserialize)]
struct Upload {
    blob: String,
    body_base64: String,
    headers: Value,
}
#[derive(Serialize, Deserialize)]
struct Plan {
    schema: u32,
    space: String,
    item: String,
    doc: FileDoc,
    uploads: Vec<Upload>,
}

impl<S: Signer> Client<S> {
    fn browser_file(&self, item: &str) -> Result<FileDoc, String> {
        let mut space = None;
        let mut versions = vec![];
        for e in self.page_events() {
            match &e.body {
                EventBody::ItemCreated {
                    item: id,
                    content: ItemContent::File(doc),
                    ..
                } if id == item => {
                    if space.is_some() {
                        return Err("Ambiguous file ID".into());
                    }
                    space = Some(e.space.clone());
                    versions.push(doc.clone());
                }
                EventBody::ItemVersioned {
                    item: id,
                    content: ItemContent::File(doc),
                    ..
                } if id == item && space.as_deref() == Some(&e.space) => versions.push(doc.clone()),
                EventBody::ItemReverted {
                    item: id,
                    to_version,
                    ..
                } if id == item && space.as_deref() == Some(&e.space) => {
                    let prior = versions
                        .get(to_version.saturating_sub(1) as usize)
                        .ok_or("File version not found")?
                        .clone();
                    versions.push(prior);
                }
                _ => {}
            }
        }
        let space = space.ok_or("File not found")?;
        if self.page_role(&space).is_none() || !self.living_tier_allows(&space) {
            return Err("File membership and entitlement required".into());
        }
        let doc = versions.pop().ok_or("File not found")?;
        validate(&doc)?;
        Ok(doc)
    }
    pub fn file_manifest(&self, item: &str) -> Result<Value, String> {
        let doc = self.browser_file(item)?;
        Ok(
            json!({"item":item,"relay":self.file_relay(),"name":doc.name,"mime":doc.mime,"bytes":doc.bytes,"sha256":doc.sha256,"chunks":doc.chunks.iter().map(|c|json!({"blob":c.blob,"bytes":c.bytes,"sealed_bytes":c.bytes+40})).collect::<Vec<_>>()}),
        )
    }
    pub fn file_open(&self, item: &str, packed: &[u8]) -> Result<Vec<u8>, String> {
        open(&self.browser_file(item)?, packed)
    }
    pub fn file_upload_headers(&self, blob: &str, now_ms: i64) -> Result<Value, String> {
        if !valid_hash(blob) {
            return Err("Invalid blob hash".into());
        }
        self.file_blob_headers(blob, now_ms)
    }
    pub fn file_prepare(
        &self,
        space: &str,
        name: &str,
        mime: &str,
        bytes: &[u8],
        now_ms: i64,
    ) -> Result<Value, String> {
        if bytes.len() > MAX_FILE
            || !matches!(
                self.page_role(space),
                Some(Role::Owner | Role::Admin | Role::Member)
            )
            || !self.living_tier_allows(space)
        {
            return Err("File write permission required or file exceeds 64 MiB".into());
        }
        let mut doc = FileDoc {
            name: name.into(),
            path: String::new(),
            mime: mime.into(),
            bytes: bytes.len() as u64,
            sha256: hash(bytes),
            chunks: vec![],
            thumb: None,
            voice: None,
        };
        let mut uploads = vec![];
        for plain in bytes.chunks(CHUNK) {
            let mut key = [0u8; 32];
            let mut nonce = [0u8; 24];
            getrandom::getrandom(&mut key).map_err(|_| "No secure entropy")?;
            getrandom::getrandom(&mut nonce).map_err(|_| "No secure entropy")?;
            let cipher =
                XChaCha20Poly1305::new_from_slice(&key).map_err(|_| "Invalid chunk key")?;
            let mut sealed = nonce.to_vec();
            sealed.extend(
                cipher
                    .encrypt(&XNonce::from(nonce), plain)
                    .map_err(|_| "Chunk encryption failed")?,
            );
            let blob = hash(&sealed);
            doc.chunks.push(ChunkRef {
                sha256: hash(plain),
                bytes: plain.len() as u64,
                blob: blob.clone(),
                key: hex::encode(key),
            });
            uploads.push(Upload {
                headers: self.file_blob_headers(&blob, now_ms)?,
                blob,
                body_base64: STANDARD.encode(sealed),
            });
        }
        validate(&doc)?;
        serde_json::to_value(Plan {
            schema: 1,
            space: space.into(),
            item: roda_types::new_secret_id("it"),
            doc,
            uploads,
        })
        .map_err(|e| e.to_string())
    }
    pub fn file_commit(
        &mut self,
        space: &str,
        plan_json: &str,
        now_ms: i64,
        edit_id: &str,
    ) -> Result<Update, String> {
        if plan_json.len() > 95_000_000 || edit_id.trim().is_empty() || edit_id.len() > 128 {
            return Err("Invalid upload plan or edit ID".into());
        }
        let plan: Plan = serde_json::from_str(plan_json).map_err(|e| e.to_string())?;
        if plan.schema != 1
            || plan.space != space
            || plan.item.trim().is_empty()
            || plan.item.len() > 128
            || plan.uploads.len() != plan.doc.chunks.len()
        {
            return Err("Upload plan does not match this Space".into());
        }
        validate(&plan.doc)?;
        let mut packed = vec![];
        for (upload, chunk) in plan.uploads.iter().zip(&plan.doc.chunks) {
            if upload.blob != chunk.blob
                || upload.body_base64.len() > ((MAX_CHUNK + 40) * 4 / 3 + 8)
            {
                return Err("Invalid upload chunk".into());
            }
            packed.extend(
                STANDARD
                    .decode(&upload.body_base64)
                    .map_err(|_| "Invalid ciphertext encoding")?,
            );
        }
        open(&plan.doc, &packed)?; // Verify the exact payload before publishing a reference.
        let digest = hash(plan_json.as_bytes());
        self.queue_page_body(
            space,
            EventBody::ItemCreated {
                item: plan.item,
                kind: ItemKind::File,
                content: ItemContent::File(plan.doc),
                origin: "Uploaded in browser".into(),
            },
            now_ms,
            edit_id,
            &digest,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn browser_file_ciphertext_roundtrip_rejects_tampering_and_wrong_key() {
        let plain = b"A genuine shared file";
        let key = [3u8; 32];
        let nonce = [4u8; 24];
        let cipher = XChaCha20Poly1305::new_from_slice(&key).unwrap();
        let mut sealed = nonce.to_vec();
        sealed.extend(
            cipher
                .encrypt(&XNonce::from(nonce), plain.as_slice())
                .unwrap(),
        );
        let mut doc = FileDoc {
            name: "test.txt".into(),
            path: String::new(),
            mime: "text/plain".into(),
            bytes: plain.len() as u64,
            sha256: hash(plain),
            chunks: vec![ChunkRef {
                sha256: hash(plain),
                bytes: plain.len() as u64,
                blob: hash(&sealed),
                key: hex::encode(key),
            }],
            thumb: None,
            voice: None,
        };
        assert_eq!(open(&doc, &sealed).unwrap(), plain);
        let mut altered = sealed.clone();
        altered[25] ^= 1;
        assert!(open(&doc, &altered).is_err());
        doc.chunks[0].key = hex::encode([5u8; 32]);
        assert!(open(&doc, &sealed).is_err());
    }
    #[test]
    fn browser_file_metadata_bound_precedes_allocation() {
        let doc = FileDoc {
            name: "test.txt".into(),
            path: String::new(),
            mime: "text/plain".into(),
            bytes: u64::MAX,
            sha256: "00".repeat(32),
            chunks: vec![],
            thumb: None,
            voice: None,
        };
        assert!(open(&doc, &[]).is_err());
    }
}
