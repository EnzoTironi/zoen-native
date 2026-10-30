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
mod targets;
pub(crate) use admission::AppEditConflictKind;
pub(crate) use targets::AppEditTargetInput;
#[cfg(feature = "client-types")]
pub(crate) use targets::AppEditTargets;

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

#[derive(Clone, serde::Serialize)]
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
    pub target_agent_id: Option<String>,
    pub expected_target_revision: Option<String>,
    pub expected_resource_uri: Option<String>,
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
        if input.target_agent_id.is_some() != input.expected_target_revision.is_some() {
            return Err(invalid());
        }
        let mut binding = ArtifactRequest {
            item_id: input.item_id.clone(),
            version: input.expected_version,
            sha256: input.expected_sha256.clone(),
            request_id: input.mutation_id.clone(),
            text_sha256: hex::encode(Sha256::digest(text.as_bytes())),
            agent_id: input.target_agent_id.clone(),
            intent_sha256: Some(event_log::content_hash(&(
                "zoen.app-edit-intent.v1",
                &input,
            ))?),
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
            let mut expected = binding.clone();
            if reference
                .artifact
                .as_ref()
                .is_some_and(|a| a.agent_id.is_none() && a.intent_sha256.is_none())
                && input.target_agent_id.is_none()
            {
                expected.intent_sha256 = None;
            }
            if expected.agent_id.is_none() {
                expected.agent_id = reference.artifact.as_ref().and_then(|a| a.agent_id.clone());
            }
            if entry.author != input.expected_identity_id
                || marks.deleted
                || marks.forwarded
                || prior != text
                || !reference.thread
                || reference.to != root.id
                || reference.artifact.as_ref() != Some(&expected)
                || self.net.failed.contains_key(&entry.client_id)
            {
                return Err(stale());
            }
            if input.expected_resource_uri.as_ref().is_some_and(|uri| {
                self.item_state(&binding.item_id).ok().and_then(|i| i.versions.iter()
                    .find(|v| v.number == binding.version)).is_none_or(|v|
                        !matches!(&v.content, ItemContent::App(app) if &app.resource_uri == uri))
            }) {
                return Err(stale());
            }
            return Ok(self.entry_dto(entry));
        }
        let item = self.item_state(&input.item_id)?;
        let current = item.current();
        let ItemContent::App(app) = &current.content else {
            return Err(invalid());
        };
        let resource_valid = miniapps::binary::Format::for_id(&app.app)
            .is_some_and(|format| app.resource_uri == format.resource())
            || (app.view.is_some() && crate::app_resources::valid(app));
        if item.space != input.space_id
            || !resource_valid
            || current.number != binding.version
            || app
                .document
                .as_ref()
                .is_none_or(|file| file.sha256 != binding.sha256)
        {
            return Err(stale());
        }
        let target = self.select_app_agent(&input)?;
        binding.agent_id = Some(target.clone());
        self.atomic_agent_action(|engine| {
            if input.share_current {
                engine.share_app_for_request(&input, &binding, &target)?;
            }
            let message = types::new_id("msg")?;
            let appended = engine.append(
                &input.space_id,
                &input.expected_identity_id,
                EventBody::AppEditRequested {
                    message,
                    text: text.into(),
                    reply: ReplyRef {
                        shared: None,
                        quote: None,
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
