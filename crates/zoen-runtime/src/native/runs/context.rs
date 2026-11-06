//! Only native message provenance and the canonical SQL citizen definition form input.
use super::*;
use sqlx::{Postgres, Transaction};
use types::agent_behavior::{AgentKind, Definition, ModelTier};
use zoen_models::OutputBlock;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AgentContext {
    kind: String,
    version: u32,
    definition: String,
}
impl AgentContext {
    pub(super) fn behavior(&self) -> Result<Definition, RuntimeError> {
        if self.definition.is_empty() || self.definition.len() > 65_536 || self.version == 0 {
            return Err(RuntimeError::InvalidBinding);
        }
        let value: Definition =
            serde_json::from_str(&self.definition).map_err(|_| RuntimeError::InvalidBinding)?;
        let expected = match value.kind {
            AgentKind::User => "User",
            AgentKind::Creator => "Creator",
        };
        if value.version != self.version
            || expected != self.kind
            || value.model_tier != ModelTier::Cheap
        {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok(value)
    }
    pub(super) fn version(&self) -> Result<String, RuntimeError> {
        self.behavior()?;
        Ok(format!(
            "native-agent-context/2/{}",
            hash(&serde_json::to_vec(self).map_err(|_| RuntimeError::InvalidBinding)?)
        ))
    }
    pub(super) fn messages(
        &self,
        facts: &ReplyFacts,
        principal: &Principal,
    ) -> Result<Vec<InputMessage>, RuntimeError> {
        let entries = facts.messages();
        if entries.is_empty()
            || entries.len() > 17
            || entries.last().is_none_or(|entry| {
                entry.source() != facts.intent().trigger() || entry.author() != principal.owner
            })
        {
            return Err(RuntimeError::InvalidBinding);
        }
        let mut messages = vec![InputMessage::System {
            text: self.behavior()?.system_prompt(),
        }];
        for entry in entries {
            if entry.author() == principal.owner {
                messages.push(InputMessage::User {
                    text: entry.text().into(),
                });
            } else if entry.author() == principal.agent {
                messages.push(InputMessage::Assistant {
                    blocks: vec![OutputBlock::Text {
                        text: entry.text().into(),
                    }],
                });
            } else {
                return Err(RuntimeError::InvalidBinding);
            }
        }
        Ok(messages)
    }
    pub(super) async fn read_in(
        tx: &mut Transaction<'_, Postgres>,
        principal: &Principal,
    ) -> Result<Option<Self>, RuntimeError> {
        // Claiming a citizen locks this identity FOR SHARE. Lock it before
        // reading optional metadata so absence cannot race a new claim.
        let identity: Option<String> =
            sqlx::query_scalar("SELECT id FROM identities WHERE id=$1 FOR UPDATE")
                .bind(&principal.agent)
                .fetch_optional(&mut **tx)
                .await?;
        if identity.as_deref() != Some(principal.agent.as_str()) {
            return Err(RuntimeError::Denied);
        }
        type Row = (
            String,
            String,
            Option<String>,
            i32,
            Option<Vec<u8>>,
            bool,
            bool,
        );
        let row:Option<Row>=sqlx::query_as("SELECT owner,device,kind,definition_version,CASE WHEN octet_length(definition)<=65536 THEN definition ELSE NULL END,paused_at IS NOT NULL,revoked_at IS NOT NULL FROM agent_citizens WHERE agent=$1 FOR SHARE")
            .bind(&principal.agent).fetch_optional(&mut **tx).await?;
        let Some((owner, device, kind, version, definition, paused, revoked)) = row else {
            return Ok(None);
        };
        if owner != principal.owner || paused || revoked {
            return Err(RuntimeError::Denied);
        }
        // Registration has always claimed the runtime device before its first
        // definition. Preserve that exact legacy text-only state; a configured
        // or malformed definition must never be treated as missing persona.
        if kind.is_none() && version == 0 && definition.as_deref() == Some(b"".as_slice()) {
            return Ok(None);
        }
        if device != principal.device {
            return Err(RuntimeError::Denied);
        }
        let value = Self {
            kind: kind.ok_or(RuntimeError::Denied)?,
            version: version
                .try_into()
                .map_err(|_| RuntimeError::InvalidBinding)?,
            definition: String::from_utf8(definition.ok_or(RuntimeError::EvidenceTooLarge)?)
                .map_err(|_| RuntimeError::InvalidBinding)?,
        };
        value.behavior()?;
        Ok(Some(value))
    }
}

impl RunRecord {
    pub(super) fn validate_context(&self) -> Result<String, RuntimeError> {
        match (self.format, &self.agent_context) {
            (1, None)
                if matches!(
                    self.request.messages.as_slice(),
                    [InputMessage::User { .. }]
                ) =>
            {
                Ok(PROFILE.into())
            }
            (2, Some(context)) => {
                let [InputMessage::System { text }, rest @ ..] = self.request.messages.as_slice()
                else {
                    return Err(RuntimeError::InvalidBinding);
                };
                if text!=&context.behavior()?.system_prompt() || rest.is_empty() || rest.len()>17
                    || !matches!(rest.last(),Some(InputMessage::User {..}))
                    || rest.iter().any(|entry| !matches!(entry,InputMessage::User {..})
                        && !matches!(entry,InputMessage::Assistant {blocks} if matches!(blocks.as_slice(),[OutputBlock::Text {..}]))) {
                    return Err(RuntimeError::InvalidBinding);
                }
                context.version()
            }
            _ => Err(RuntimeError::InvalidBinding),
        }
    }
    pub(super) fn verify_input(&self, facts: &ReplyFacts) -> Result<(), RuntimeError> {
        let expected = match &self.agent_context {
            Some(context) => context.messages(facts, &self.original.context.principal)?,
            None => vec![InputMessage::User {
                text: facts.text().into(),
            }],
        };
        if self.request.messages != expected {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok(())
    }
    pub(super) async fn current_definition_in(
        &self,
        tx: &mut Transaction<'_, Postgres>,
    ) -> Result<(), RuntimeError> {
        let current = AgentContext::read_in(tx, &self.original.context.principal).await?;
        if current.as_ref().map(AgentContext::version).transpose()?
            != self
                .agent_context
                .as_ref()
                .map(AgentContext::version)
                .transpose()?
        {
            return Err(RuntimeError::Denied);
        }
        Ok(())
    }
}
impl ReplyStep {
    pub(crate) async fn current_definition_in(
        &self,
        tx: &mut Transaction<'_, Postgres>,
    ) -> Result<(), RuntimeError> {
        self.loaded.record.current_definition_in(tx).await
    }
}
