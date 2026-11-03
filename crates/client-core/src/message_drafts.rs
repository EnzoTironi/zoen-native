//! Private, device-local normal composer drafts. Never a replicated event or UI preference.
use crate::{
    engine::{Engine, EntryBody, R},
    i18n::t,
    replies::external::SharedReplyDraft,
    CoreError,
};
use hkdf::Hkdf;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

mod acceptance;
mod contract;
pub(crate) use contract::{DraftContent, DraftScope};
pub use contract::{MessageDraftDto, MessageDraftSendResult};
mod editing;
pub use editing::MessageDraftEdit;

pub const MESSAGE_DRAFT_REVISION_ZERO: &str = "0";
const DOMAIN: &[u8] = b"zoen.message.draft.v1";
pub(crate) const PREFIX: &str = "message.draft.";
const MAX_SCOPES: usize = 4096;
const MAX_RETAINED_BYTES: usize = 64 << 20;
const MAX_PLAIN_BYTES: usize = crate::client_api::MAX_REQUEST_BYTES + 16 * 1024;
const MAX_CIPHER_BYTES: usize = (MAX_PLAIN_BYTES + 40).div_ceil(3) * 4 + 3;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    input_revision: String,
    message_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    text_revision: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    draft: MessageDraftDto,
    last_accepted: Option<Receipt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_accepted_edit: Option<Receipt>,
}
impl Default for Record {
    fn default() -> Self {
        Self {
            draft: MessageDraftDto {
                reply_intent: None,
                revision: MESSAGE_DRAFT_REVISION_ZERO.into(),
                text: String::new(),
                edit: None,
                reply_to: None,
                reply_quote: None,
                shared_reply: None,
            },
            last_accepted: None,
            last_accepted_edit: None,
        }
    }
}
struct StorageScope {
    id: String,
    aad: Vec<u8>,
    key: Zeroizing<[u8; 32]>,
}
fn storage_error() -> CoreError {
    CoreError::Storage {
        message: t(
            "não foi possível recuperar o rascunho da conversa",
            "could not recover the conversation draft",
        ),
    }
}
fn invalid() -> CoreError {
    CoreError::Invalid {
        reason: t(
            "rascunho de conversa inválido ou grande demais",
            "invalid or oversized conversation draft",
        ),
    }
}
fn stale() -> CoreError {
    CoreError::Stale {
        reason: t(
            "o rascunho mudou; tente novamente com a versão atual",
            "the draft changed; retry from its current revision",
        ),
    }
}
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 256 && !id.chars().any(char::is_control)
}
fn valid_revision(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

impl Receipt {
    fn valid(&self) -> bool {
        valid_revision(&self.input_revision)
            && self.input_revision != MESSAGE_DRAFT_REVISION_ZERO
            && valid_id(&self.message_id)
            && self.text_revision.as_deref().is_none_or(valid_id)
    }
}

impl Engine {
    pub(crate) fn validate_private_draft_scope(&self, scope: &DraftScope) -> R<()> {
        let account = self.net.account.as_ref().ok_or_else(invalid)?;
        if !valid_id(&scope.expected_identity_id)
            || !valid_id(&scope.expected_device_id)
            || !valid_id(&scope.space_id)
            || scope
                .thread_root_id
                .as_deref()
                .is_some_and(|id| !valid_id(id))
            || account.identity != scope.expected_identity_id
            || account.device != scope.expected_device_id
            || self.me.as_deref() != Some(account.identity.as_str())
            || self.provider_setup_pending()
            || !self.is_member(&scope.space_id, &account.identity)
        {
            return Err(CoreError::Forbidden {
                reason: t(
                    "a conta ou o acesso à conversa mudou",
                    "the account or conversation access changed",
                ),
            });
        }
        let author = self.author_for(&account.identity)?;
        if author.device.as_deref() != Some(account.device.as_str())
            || author.key.id() != account.device
        {
            return Err(invalid());
        }
        if let Some(root) = &scope.thread_root_id {
            if self.reply_ref(&scope.space_id, root, true)?.to != *root {
                return Err(invalid());
            }
        }
        Ok(())
    }
    fn draft_storage_scope(&self, scope: &DraftScope) -> R<StorageScope> {
        self.validate_private_draft_scope(scope)?;
        let account = self.net.account.as_ref().ok_or_else(invalid)?;
        let author = self.author_for(&account.identity)?;
        let aad = serde_json::to_vec(&(
            "zoen.message.draft.v1",
            &account.identity,
            &account.device,
            &scope.space_id,
            &scope.thread_root_id,
        ))
        .map_err(|_| storage_error())?;
        let id = format!("{PREFIX}{}", hex::encode(Sha256::digest(&aad)));
        let mut key = Zeroizing::new([0u8; 32]);
        Hkdf::<Sha256>::new(Some(DOMAIN), &author.key.secret())
            .expand(b"local draft encryption", &mut *key)
            .map_err(|_| storage_error())?;
        Ok(StorageScope { id, aad, key })
    }
    pub(crate) fn draft_target(&self, scope: &DraftScope, reply: Option<&str>) -> R<()> {
        let target = reply.or(scope.thread_root_id.as_deref());
        if let Some(target) = target {
            if !valid_id(target) {
                return Err(invalid());
            }
            let entry = self
                .space_state(&scope.space_id)?
                .entries
                .iter()
                .find(|entry| entry.id() == target)
                .ok_or_else(stale)?;
            if !matches!(&entry.body, EntryBody::Message { marks, .. } if !marks.deleted) {
                return Err(stale());
            }
            if !self.visible_message(entry) && scope.thread_root_id.is_none() {
                return Err(stale());
            }
            if let Some(root) = &scope.thread_root_id {
                if self.reply_ref(&scope.space_id, target, true)?.to != *root {
                    return Err(stale());
                }
            }
        }
        Ok(())
    }
    fn draft_text_budget(
        scope: &DraftScope,
        text: &str,
        reply: Option<&str>,
        quote: Option<&crate::ReplySelection>,
        shared: Option<&SharedReplyDraft>,
    ) -> R<()> {
        if text.len() > crate::client_api::MAX_REQUEST_BYTES {
            return Err(invalid());
        }
        let mut message =
            serde_json::json!({"op":"send_message", "space_id":scope.space_id, "text":text});
        if let Some(to) = reply.or(scope.thread_root_id.as_deref()) {
            message["reply_to"] = to.into();
        }
        if let Some(quote) = quote {
            message["reply_quote"] = serde_json::to_value(quote).map_err(|_| invalid())?;
        }
        if let Some(shared) = shared {
            message["shared_reply"] = serde_json::to_value(shared).map_err(|_| invalid())?;
        }
        if scope.thread_root_id.is_some() {
            message["thread"] = true.into();
        }
        if serde_json::to_vec(&message).map_err(|_| invalid())?.len()
            > crate::client_api::MAX_REQUEST_BYTES
        {
            return Err(invalid());
        }
        Ok(())
    }
    fn load_message_draft(&self, scope: &DraftScope, storage: &StorageScope) -> R<Record> {
        // Bound the value in SQLite before materializing attacker-controlled ciphertext.
        let row: Option<Option<String>> = self.store.conn().query_row(
            "SELECT CASE WHEN length(CAST(value AS BLOB)) <= ?2 THEN value ELSE NULL END FROM meta WHERE key=?1",
            rusqlite::params![storage.id, MAX_CIPHER_BYTES], |row| row.get(0)).optional().map_err(|_| storage_error())?;
        let Some(encoded) = row else {
            return Ok(Record::default());
        };
        let encoded = encoded.ok_or_else(storage_error)?;
        let plain = Zeroizing::new(
            crate::local_sealed::open(&storage.key, &storage.aad, &encoded, MAX_PLAIN_BYTES)
                .map_err(|()| storage_error())?,
        );
        let record: Record = serde_json::from_slice(&plain).map_err(|_| storage_error())?;
        if !valid_revision(&record.draft.revision)
            || record.draft.revision == MESSAGE_DRAFT_REVISION_ZERO
            || record
                .draft
                .reply_to
                .as_deref()
                .is_some_and(|id| !valid_id(id))
            || record.last_accepted.as_ref().is_some_and(|r| !r.valid())
            || record
                .draft
                .reply_intent
                .as_deref()
                .is_some_and(|id| !valid_revision(id) || id == "0")
            || (record.draft.reply_intent.is_some()
                && record.draft.reply_to.is_none()
                && record.draft.shared_reply.is_none())
        {
            return Err(storage_error());
        }
        Self::draft_text_budget(
            scope,
            &record.draft.text,
            record.draft.reply_to.as_deref(),
            record.draft.reply_quote.as_ref(),
            record.draft.shared_reply.as_ref(),
        )
        .map_err(|_| storage_error())?;
        Self::validate_edit_shape(record.draft.edit.as_ref()).map_err(|_| storage_error())?;
        if record
            .last_accepted_edit
            .as_ref()
            .is_some_and(|r| !r.valid())
        {
            return Err(storage_error());
        }
        Ok(record)
    }
    fn persist_message_draft(&self, storage: &StorageScope, record: &Record) -> R<()> {
        let plain = Zeroizing::new(serde_json::to_vec(record).map_err(|_| storage_error())?);
        if plain.len() > MAX_PLAIN_BYTES {
            return Err(invalid());
        }
        let encoded = crate::local_sealed::seal(&storage.key, &storage.aad, &plain)
            .map_err(|()| storage_error())?;
        let (count, bytes, old): (usize, usize, usize) = self.store.conn().query_row(
            "SELECT COUNT(*), COALESCE(SUM(length(CAST(value AS BLOB))),0), COALESCE(MAX(CASE WHEN key=?1 THEN length(CAST(value AS BLOB)) END),0) FROM meta WHERE key LIKE 'message.draft.%'",
            [&storage.id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(|_| storage_error())?;
        if (old == 0 && count >= MAX_SCOPES)
            || bytes
                .checked_sub(old)
                .and_then(|n| n.checked_add(encoded.len()))
                .is_none_or(|n| n > MAX_RETAINED_BYTES)
        {
            return Err(invalid());
        }
        self.store.set_meta(&storage.id, &encoded)?;
        Ok(())
    }
    pub(crate) fn message_draft(&self, scope: &DraftScope) -> R<MessageDraftDto> {
        let storage = self.draft_storage_scope(scope)?;
        Ok(self.load_message_draft(scope, &storage)?.draft)
    }
    #[cfg(test)]
    pub(crate) fn save_message_draft(
        &self,
        scope: &DraftScope,
        expected: &str,
        text: String,
        reply_to: Option<String>,
    ) -> R<MessageDraftDto> {
        self.save_message_draft_selected(scope, expected, text, reply_to, None)
    }
    #[cfg(test)]
    pub(crate) fn save_message_draft_selected(
        &self,
        scope: &DraftScope,
        expected: &str,
        text: String,
        reply_to: Option<String>,
        reply_quote: Option<crate::ReplySelection>,
    ) -> R<MessageDraftDto> {
        self.save_message_draft_content(
            scope,
            expected,
            DraftContent {
                reply_intent: None,
                text,
                edit: None,
                reply_to,
                reply_quote,
                shared_reply: None,
            },
        )
    }
    pub(crate) fn validate_draft_reply(
        &self,
        scope: &DraftScope,
        to: Option<&str>,
        quote: Option<&crate::ReplySelection>,
        shared: Option<&SharedReplyDraft>,
    ) -> R<()> {
        if let Some(shared) = shared {
            if to.is_some() || quote.is_some() {
                return Err(invalid());
            }
            self.validate_shared_reply(scope, shared)
        } else {
            self.draft_target(scope, to)?;
            self.validate_reply_selection(&scope.space_id, to, quote)
        }
    }
    pub(crate) fn save_message_draft_content(
        &self,
        scope: &DraftScope,
        expected: &str,
        content: DraftContent,
    ) -> R<MessageDraftDto> {
        let DraftContent {
            text,
            edit,
            reply_to,
            reply_quote,
            shared_reply,
            reply_intent,
        } = content;
        let storage = self.draft_storage_scope(scope)?;
        let mut record = self.load_message_draft(scope, &storage)?;
        if !valid_revision(expected) || record.draft.revision != expected {
            return Err(stale());
        }
        Self::draft_text_budget(
            scope,
            &text,
            reply_to.as_deref(),
            reply_quote.as_ref(),
            shared_reply.as_ref(),
        )?;
        // Private user work survives stale, deleted or unloaded sources. Scope,
        // encryption, CAS and quotas protect storage; publication reauthorizes.
        Self::validate_edit_shape(edit.as_ref())?;
        // The encrypted record already owns this exact reviewed context. New
        // private typing must survive source loss; changing the context and
        // publishing still require the live source independently.
        let same_reply = record.draft.reply_to == reply_to
            && record.draft.reply_quote == reply_quote
            && record.draft.shared_reply == shared_reply;
        // Older clients omit the token on text-only edits; preserve the known
        // intent. A new explicit selection, even of the same text, renews it.
        let reply_intent = reply_intent.or_else(|| {
            same_reply
                .then(|| record.draft.reply_intent.clone())
                .flatten()
        });
        if reply_intent
            .as_deref()
            .is_some_and(|id| !valid_revision(id) || id == "0")
            || (reply_intent.is_some() && reply_to.is_none() && shared_reply.is_none())
        {
            return Err(invalid());
        }
        let retained_reply = same_reply && record.draft.reply_intent == reply_intent;
        if !retained_reply {
            self.validate_draft_reply(
                scope,
                reply_to.as_deref(),
                reply_quote.as_ref(),
                shared_reply.as_ref(),
            )?;
        }
        if (!text.is_empty() || reply_to.is_some() || edit.is_some())
            && record.draft.edit == edit
            && record.draft.text == text
            && record.draft.reply_to == reply_to
            && record.draft.reply_quote == reply_quote
            && record.draft.shared_reply == shared_reply
            && record.draft.reply_intent == reply_intent
        {
            return Ok(record.draft);
        }
        record.draft = MessageDraftDto {
            reply_intent,
            revision: types::new_id("draft")?,
            text,
            edit,
            reply_to,
            reply_quote,
            shared_reply,
        };
        self.persist_message_draft(&storage, &record)?;
        Ok(record.draft)
    }
    pub(crate) fn send_message_draft(
        &mut self,
        scope: &DraftScope,
        expected: &str,
    ) -> R<MessageDraftSendResult> {
        let storage = self.draft_storage_scope(scope)?;
        let record = self.load_message_draft(scope, &storage)?;
        if !valid_revision(expected) {
            return Err(stale());
        }
        if let Some(receipt) = record
            .last_accepted
            .as_ref()
            .filter(|r| r.input_revision == expected)
        {
            return Ok(self.message_draft_acceptance(scope, receipt, record.draft));
        }
        if record.draft.revision != expected || expected == MESSAGE_DRAFT_REVISION_ZERO {
            return Err(stale());
        }
        // This exact revision is current and has no accepted receipt. Only
        // these memory-only checks can prove definitive refusal. Owner, storage,
        // CAS and response-encoding errors must never acquire this meaning.
        if self
            .validate_draft_reply(
                scope,
                record.draft.reply_to.as_deref(),
                record.draft.reply_quote.as_ref(),
                record.draft.shared_reply.as_ref(),
            )
            .is_err()
        {
            return Err(CoreError::DraftRejected {
                reason: t(
                    "a mensagem de resposta não está mais disponível",
                    "the reply target is no longer available",
                ),
            });
        }
        if !self
            .state
            .writer(&scope.space_id, &scope.expected_identity_id)
        {
            return Err(CoreError::DraftRejected {
                reason: t(
                    "só autores enviam mensagens",
                    "only writers can send messages",
                ),
            });
        }
        if record.draft.text.trim().is_empty() {
            return Err(CoreError::DraftRejected {
                reason: t("mensagem vazia", "empty message"),
            });
        }
        self.atomic_agent_action(|engine| {
            let entry = if let Some(shared) = &record.draft.shared_reply {
                engine.send_shared_reply(scope, &record.draft.text, shared)?
            } else {
                match record
                    .draft
                    .reply_to
                    .as_deref()
                    .or(scope.thread_root_id.as_deref())
                {
                    Some(to) => engine.reply_selected(
                        &scope.space_id,
                        &scope.expected_identity_id,
                        &record.draft.text,
                        to,
                        scope.thread_root_id.is_some(),
                        record.draft.reply_quote.as_ref(),
                    )?,
                    None => engine.send_message(&scope.space_id, &record.draft.text)?,
                }
            };
            let cleared = Record {
                draft: MessageDraftDto {
                    reply_intent: None,
                    revision: types::new_id("draft")?,
                    text: String::new(),
                    edit: record.draft.edit.clone(),
                    reply_to: None,
                    reply_quote: None,
                    shared_reply: None,
                },
                last_accepted_edit: record.last_accepted_edit.clone(),
                last_accepted: Some(Receipt {
                    input_revision: expected.into(),
                    message_id: entry.id.clone(),
                    text_revision: entry.text_revision.clone(),
                }),
            };
            engine.persist_message_draft(&storage, &cleared)?;
            Ok(MessageDraftSendResult {
                message_id: entry.id.clone(),
                text_revision: entry.text_revision.clone(),
                entry: Some(entry),
                draft: cleared.draft,
            })
        })
    }
}

#[cfg(test)]
mod tests;
