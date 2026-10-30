//! A selected deletion uses the existing signed tombstones as its terminal state.
//! There is no second history, per-attempt ledger, or best-effort partial deletion.
use crate::{engine::EntryBody, i18n::t, Client, CoreError};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct MessageDeletionTarget {
    pub message_id: String,
    pub expected_revision: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct MessageDeletion {
    pub expected_identity_id: String,
    pub expected_device_id: String,
    pub space_id: String,
    pub targets: Vec<MessageDeletionTarget>,
}

fn valid_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}
fn changed() -> CoreError {
    CoreError::Stale {
        reason: t(
            "a seleção mudou; revise novamente",
            "the selection changed; review it again",
        ),
    }
}

impl Client {
    /// Delete exactly this reviewed set, or commit nothing. Already deleted entries
    /// are terminal: an unknown ACK or cold retry never appends another tombstone.
    pub fn delete_messages(&self, input: MessageDeletion) -> Result<Vec<String>, CoreError> {
        let mut engine = self.lock();
        engine.native_surface_owner(&input.expected_identity_id, &input.expected_device_id)?;
        let mut ids = HashSet::new();
        if !valid_id(&input.space_id)
            || input.targets.is_empty()
            || input.targets.len() > 100
            || input.targets.iter().any(|target| {
                !valid_id(&target.message_id)
                    || !valid_id(&target.expected_revision)
                    || !ids.insert(&target.message_id)
            })
        {
            return Err(CoreError::Invalid {
                reason: t("seleção de mensagens inválida", "invalid message selection"),
            });
        }
        if !engine
            .state
            .writer(&input.space_id, &input.expected_identity_id)
        {
            return Err(CoreError::Forbidden {
                reason: t("o acesso à conversa mudou", "conversation access changed"),
            });
        }
        if let Some(reason) = engine.checked_integrity_error(&input.space_id)? {
            return Err(CoreError::Invalid { reason });
        }
        let state = engine.space_state(&input.space_id)?;
        let mut pending = Vec::new();
        for target in &input.targets {
            let entry = state
                .entries
                .iter()
                .find(|entry| entry.id() == target.message_id)
                .ok_or_else(changed)?;
            let EntryBody::Message { marks, .. } = &entry.body else {
                return Err(changed());
            };
            if entry.author != input.expected_identity_id {
                return Err(CoreError::Forbidden {
                    reason: t("só quem escreveu apaga", "only the author can delete"),
                });
            }
            // A terminal deletion needs no further write or historical text access.
            // Every still-live entry must match the version actually confirmed.
            if !marks.deleted {
                if marks.text_revision.as_deref().unwrap_or(entry.id()) != target.expected_revision
                {
                    return Err(changed());
                }
                pending.push(target.message_id.clone());
            }
        }
        engine.atomic_agent_action(|engine| {
            for id in pending {
                engine.annotate(&input.space_id, &id, types::Annotation::Delete)?;
            }
            Ok(input
                .targets
                .into_iter()
                .map(|target| target.message_id)
                .collect())
        })
    }
}

#[cfg(test)]
mod tests;
