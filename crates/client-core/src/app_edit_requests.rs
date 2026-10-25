//! App edit requests are ordinary signed replies with an exact artifact reference.
//! No model, budget, grant or independent task database is created here.
use crate::{
    engine::{Engine, EntryBody, R},
    i18n::t,
    CoreError, TimelineEntry,
};
use sha2::{Digest, Sha256};
mod admission;
mod sharing;
pub(crate) use admission::AppEditConflictKind;

use types::{ArtifactRequest, EventBody, ItemContent, ReplyRef};

#[derive(serde::Serialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub(crate) struct AppEditRequestResult {
    pub entry: Option<TimelineEntry>,
    /// A definitive pre-publication conflict; the prompt remains editable.
    pub conflict: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "client-types", ts(optional))]
    pub conflict_kind: Option<AppEditConflictKind>,
}

#[derive(Clone)]
pub(crate) struct AppEditRequestInput {
    pub expected_identity_id: String,
    pub expected_device_id: String,
    pub space_id: String,
    pub item_id: String,
    pub expected_version: u32,
    pub expected_sha256: String,
    pub text: String,
    pub mutation_id: String,
    pub share_current: bool,
}
fn invalid() -> CoreError {
    CoreError::Invalid {
        reason: t(
            "Pedido de alteração do App inválido.",
            "Invalid App edit request.",
        ),
    }
}
fn stale() -> CoreError {
    CoreError::Stale {
        reason: t(
            "O App ou o pedido mudou; confira a versão atual.",
            "The App or request changed; review the current version.",
        ),
    }
}
impl Engine {
    pub(crate) fn request_app_edit(&mut self, input: AppEditRequestInput) -> R<TimelineEntry> {
        self.native_surface_owner(&input.expected_identity_id, &input.expected_device_id)?;
        if !self
            .state
            .writer(&input.space_id, &input.expected_identity_id)
        {
            return Err(CoreError::Forbidden {
                reason: t("O acesso à conversa mudou.", "Conversation access changed."),
            });
        }
        if let Some(reason) = self.checked_integrity_error(&input.space_id)? {
            return Err(CoreError::Invalid { reason });
        }
        let text = input.text.trim();
        if text.is_empty() || text.len() > 16 * 1024 || text.contains('\0') {
            return Err(invalid());
        }
        let binding = ArtifactRequest {
            item_id: input.item_id.clone(),
            version: input.expected_version,
            sha256: input.expected_sha256.clone(),
            request_id: input.mutation_id.clone(),
            text_sha256: hex::encode(Sha256::digest(text.as_bytes())),
        };
        if !binding.is_well_formed() {
            return Err(invalid());
        }
        let discussion = self.item_discussion(&input.space_id, &input.item_id)?;
        let root = discussion
            .root
            .filter(|root| !root.deleted)
            .ok_or_else(invalid)?;
        // The signed reply itself is the durable receipt. A retry cannot enqueue
        // another task, even after that task has already advanced the document.
        for entry in &self.space_state(&input.space_id)?.entries {
            let EntryBody::Message {
                text: prior,
                reply: Some(reference),
                marks,
                ..
            } = &entry.body
            else {
                continue;
            };
            if !reference
                .artifact
                .as_ref()
                .is_some_and(|a| a.request_id == binding.request_id)
            {
                continue;
            }
            if entry.author != input.expected_identity_id
                || marks.deleted
                || marks.forwarded
                || prior != text
                || !reference.thread
                || reference.to != root.id
                || reference.artifact.as_ref() != Some(&binding)
            {
                return Err(stale());
            }
            return Ok(self.entry_dto(entry));
        }
        let item = self.item_state(&input.item_id)?;
        let current = item.current();
        let ItemContent::App(app) = &current.content else {
            return Err(invalid());
        };
        let format = miniapps::binary::Format::for_id(&app.app).ok_or_else(invalid)?;
        if item.space != input.space_id
            || app.resource_uri != format.resource()
            || current.number != binding.version
            || app
                .document
                .as_ref()
                .is_none_or(|file| file.sha256 != binding.sha256)
        {
            return Err(stale());
        }
        let target = self
            .ready_owned_app_agent(&input.space_id, &input.expected_identity_id)
            .map_err(|kind| CoreError::Stale {
                reason: kind.message(),
            })?;
        self.atomic_agent_action(|engine| {
            if input.share_current {
                engine.share_app_for_request(&input, &binding, &target)?;
            }
            let message = types::new_id("msg");
            let appended = engine.append(
                &input.space_id,
                &input.expected_identity_id,
                EventBody::AppEditRequested {
                    message,
                    text: text.into(),
                    reply: ReplyRef {
                        to: root.id,
                        thread: true,
                        artifact: Some(binding),
                    },
                },
            )?;
            engine
                .space_state(&input.space_id)?
                .entries
                .iter()
                .find(|entry| {
                    matches!(&entry.body, EntryBody::Message { .. })
                        && entry.id() == appended.client_id
                })
                .map(|entry| engine.entry_dto(entry))
                .ok_or_else(invalid)
        })
    }
}
