use base64::Engine as _;
use chacha20poly1305::{
    aead::{Aead, Payload},
    KeyInit, XChaCha20Poly1305, XNonce,
};
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::engine::{Engine, ItemState, R};
use crate::CoreError;

use super::{PageBlockDto, PageSession};

const RETAINED_RECEIPTS: usize = 512;

#[derive(Clone, Default, Serialize, Deserialize)]
pub(super) struct Journal {
    pub generation: u64,
    pub minimum_generation: u64,
    pub snapshot: Option<String>,
    /// Keep the snapshot after saving: retained receipts can refer to local no-op history.
    pub has_draft: bool,
    pub commit: Option<String>,
    receipts: Vec<Receipt>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Receipt {
    pub fingerprint: String,
    mutation_id: String,
    context_hash: String,
    generation: u64,
    #[serde(default)]
    pub accepted_generation: u64,
    pub content_hash: String,
    pub frontiers: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
pub(super) struct EditContext {
    pub generation: u64,
    pub frontiers: Vec<u8>,
}

impl Journal {
    pub fn receipt(&self, mutation_id: &str) -> Option<&Receipt> {
        self.receipts.iter().find(|r| r.mutation_id == mutation_id)
    }

    pub fn continuation(&self, context_hash: &str) -> Option<&Receipt> {
        self.receipts
            .iter()
            .rev()
            .find(|r| r.context_hash == context_hash)
    }

    pub fn accept(
        &mut self,
        mutation_id: String,
        fingerprint: String,
        context_hash: String,
        content_hash: String,
        generation: u64,
        frontiers: Vec<u8>,
    ) -> R<()> {
        self.generation = self.generation.checked_add(1).ok_or_else(storage_error)?;
        self.receipts.push(Receipt {
            mutation_id,
            fingerprint,
            context_hash,
            generation,
            accepted_generation: self.generation,
            content_hash,
            frontiers,
        });
        if self.receipts.len() > RETAINED_RECEIPTS {
            let removed = self.receipts.remove(0);
            self.minimum_generation = self.minimum_generation.max(
                removed
                    .generation
                    .checked_add(1)
                    .ok_or_else(storage_error)?,
            );
        }
        Ok(())
    }

    pub fn invalidate_contexts(&mut self) -> R<()> {
        self.generation = self.generation.checked_add(1).ok_or_else(storage_error)?;
        self.minimum_generation = self.generation;
        Ok(())
    }
}

pub(super) fn request_hash(
    mutation_id: &str,
    context: &str,
    order: &[String],
    changed: &[PageBlockDto],
) -> R<String> {
    let bytes =
        serde_json::to_vec(&(mutation_id, context, order, changed)).map_err(|_| storage_error())?;
    Ok(hex::encode(
        Sha256::new()
            .chain_update(b"zoen.page.edit.request.v1\0")
            .chain_update(bytes)
            .finalize(),
    ))
}

pub(super) fn context_hash(context: &str) -> String {
    hex::encode(
        Sha256::new()
            .chain_update(b"zoen.page.editor.context.v1\0")
            .chain_update(context)
            .finalize(),
    )
}

fn storage_error() -> CoreError {
    CoreError::Storage {
        reason: crate::i18n::t(
            "não foi possível recuperar o rascunho da página",
            "could not recover the page draft",
        ),
    }
}

impl Engine {
    fn draft_scope(&self, item: &ItemState) -> R<(String, Vec<u8>, [u8; 32])> {
        let me = self.me_id()?;
        let author = self.author_for(&me)?;
        let aad = serde_json::to_vec(&("zoen.page.draft.v1", me, &item.space, &item.id))
            .map_err(|_| storage_error())?;
        let id = format!("page.draft.{}", hex::encode(Sha256::digest(&aad)));
        let mut key = [0u8; 32];
        Hkdf::<Sha256>::new(Some(b"zoen.page.draft.v1"), &author.key.secret())
            .expand(b"local draft encryption", &mut key)
            .map_err(|_| storage_error())?;
        Ok((id, aad, key))
    }

    pub(super) fn load_page_journal(&self, item: &ItemState) -> R<Journal> {
        if self.me.is_none() {
            return Ok(Journal::default());
        }
        // Looking up the journal before deriving the key keeps historical reads
        // available when this device no longer has an account signing key.
        let aad = serde_json::to_vec(&(
            "zoen.page.draft.v1",
            self.me.as_ref().expect("checked above"),
            &item.space,
            &item.id,
        ))
        .map_err(|_| storage_error())?;
        let id = format!("page.draft.{}", hex::encode(Sha256::digest(&aad)));
        let Some(encoded) = self.store.meta(&id)? else {
            return Ok(Journal::default());
        };
        let (_, aad, key) = self.draft_scope(item)?;
        let plain = open(&key, &aad, &encoded)?;
        serde_json::from_slice(&plain).map_err(|_| storage_error())
    }

    pub(super) fn save_page_journal(&self, item: &ItemState, journal: &Journal) -> R<()> {
        let (id, aad, key) = self.draft_scope(item)?;
        let plain = serde_json::to_vec(journal).map_err(|_| storage_error())?;
        let encoded = seal(&key, &aad, &plain)?;
        self.store.set_meta(&id, &encoded)?;
        Ok(())
    }

    pub(super) fn page_edit_context(&self, item: &ItemState, session: &PageSession) -> R<String> {
        self.page_context_at(
            item,
            session.journal.generation,
            session.live.frontiers().encode(),
        )
    }

    pub(super) fn page_context_at(
        &self,
        item: &ItemState,
        generation: u64,
        frontiers: Vec<u8>,
    ) -> R<String> {
        let (_, mut aad, key) = self.draft_scope(item)?;
        aad.extend(b"\0editor context");
        let context = EditContext {
            generation,
            frontiers,
        };
        seal(
            &key,
            &aad,
            &serde_json::to_vec(&context).map_err(|_| storage_error())?,
        )
    }

    pub(super) fn decode_page_edit_context(
        &self,
        item: &ItemState,
        context: &str,
    ) -> R<EditContext> {
        let (_, mut aad, key) = self.draft_scope(item)?;
        aad.extend(b"\0editor context");
        let invalid = || super::invalid("contexto de edição inválido", "invalid editing context");
        let plain = open(&key, &aad, context).map_err(|_| invalid())?;
        serde_json::from_slice(&plain).map_err(|_| invalid())
    }
}

fn seal(key: &[u8; 32], aad: &[u8], plain: &[u8]) -> R<String> {
    let mut nonce = [0u8; 24];
    getrandom::getrandom(&mut nonce).map_err(|_| storage_error())?;
    let sealed = XChaCha20Poly1305::new(key.into())
        .encrypt(&XNonce::from(nonce), Payload { msg: plain, aad })
        .map_err(|_| storage_error())?;
    let mut bytes = nonce.to_vec();
    bytes.extend(sealed);
    Ok(format!(
        "v1:{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

fn open(key: &[u8; 32], aad: &[u8], encoded: &str) -> R<Vec<u8>> {
    let sealed = base64::engine::general_purpose::STANDARD
        .decode(encoded.strip_prefix("v1:").ok_or_else(storage_error)?)
        .map_err(|_| storage_error())?;
    if sealed.len() < 40 {
        return Err(storage_error());
    }
    let nonce: [u8; 24] = sealed[..24].try_into().map_err(|_| storage_error())?;
    XChaCha20Poly1305::new(key.into())
        .decrypt(
            &XNonce::from(nonce),
            Payload {
                msg: &sealed[24..],
                aad,
            },
        )
        .map_err(|_| storage_error())
}
