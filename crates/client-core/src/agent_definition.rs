//! Owner-reviewed behavior edits. Status/Define remain the relay authority; the
//! local receipt is protected recovery context, never a permission or cancellation.
use crate::{api::invalid, AgentStatus, Client, CoreError, Engine};
use protocol::{AgentRequest, AgentStateRecord, Op, Reply};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

type R<T> = Result<T, CoreError>;
const RECEIPT_LIMIT: usize = 8192;
const RELAY_DEFINITION_LIMIT: usize = 64 * 1024;

fn changed() -> CoreError {
    invalid("The account, device or reviewed Agent definition changed. Review it again.")
}
fn unsupported() -> CoreError {
    invalid("This editor supports an exact User definition with Cheap tier and empty state. Unsupported behavior is not substituted.")
}
fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Shared with enrollment: keep the same strict canonical schema and limits.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct UserDefinition {
    pub version: u32,
    pub kind: String,
    pub persona: String,
    pub rules: Vec<String>,
    pub state: serde_json::Value,
    pub model_tier: String,
}
impl UserDefinition {
    pub(crate) fn parse(value: &str, version: u32) -> R<Self> {
        if value.len() > 1024 || version == 0 || version > i32::MAX as u32 {
            return Err(unsupported());
        }
        let definition: Self = serde_json::from_str(value).map_err(|_| unsupported())?;
        if definition.version != version
            || definition.kind != "user"
            || definition.model_tier != "cheap"
            || !definition
                .state
                .as_object()
                .is_some_and(|state| state.is_empty())
            || definition.persona.is_empty()
            || definition.persona.len() > 768
            || definition
                .persona
                .chars()
                .any(|c| c.is_control() && c != '\n')
            || definition.rules.len() > 8
            || definition.rules.iter().any(|rule| {
                rule.is_empty() || rule.len() > 256 || rule.chars().any(char::is_control)
            })
            || serde_json::to_string(&definition).map_err(|_| unsupported())? != value
        {
            return Err(unsupported());
        }
        Ok(definition)
    }
    pub(crate) fn parse_bootstrap(value: &str) -> R<Self> {
        Self::parse(value, 1)
    }
    fn edited(&self, persona: String, rules: Vec<String>) -> R<String> {
        if self.persona == persona && self.rules == rules {
            return Err(invalid(
                "Change the persona or rules before reviewing this update.",
            ));
        }
        let mut next = self.clone();
        next.version = self.version.checked_add(1).ok_or_else(unsupported)?;
        next.persona = persona;
        next.rules = rules;
        let encoded = serde_json::to_string(&next).map_err(|_| unsupported())?;
        Self::parse(&encoded, next.version)?;
        Ok(encoded)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct AgentDefinitionDto {
    pub agent_id: String,
    pub owner_id: String,
    pub device_id: String,
    pub relay_url: String,
    pub status: AgentStatus,
    pub version: u32,
    pub definition_sha256: String,
    pub persona: String,
    pub rules: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct AgentDefinitionPreviewDto {
    pub current: AgentDefinitionDto,
    pub proposed: AgentDefinitionDto,
    pub preview_digest: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub enum AgentDefinitionUpdateOutcome {
    /// The current relay version and exact bytes equal the reviewed next version.
    Confirmed,
    Conflict,
    /// Includes observing the old base: an earlier dispatched request may arrive later.
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct AgentDefinitionUpdateDto {
    pub receipt_id: String,
    pub review: AgentDefinitionPreviewDto,
    pub outcome: AgentDefinitionUpdateOutcome,
    pub current: Option<AgentDefinitionDto>,
}

pub(crate) struct DefinitionTarget {
    pub agent: String,
    pub identity: String,
    pub device: String,
}
pub(crate) struct DefinitionEdit {
    pub version: u32,
    pub sha256: String,
    pub persona: String,
    pub rules: Vec<String>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Scope {
    owner: String,
    device: String,
    certificate_sha256: String,
    relay: String,
    agent: String,
    configured_relay: String,
}
impl Scope {
    fn capture(engine: &Engine, target: &DefinitionTarget) -> R<Self> {
        let owner =
            crate::agent_enrollment::owner_context(engine, &target.identity, &target.device)?;
        if target.agent == target.identity
            || event_log::agent_owner::enrollment_offer_message(&target.agent, &"0".repeat(64))
                .is_none()
        {
            return Err(changed());
        }
        Ok(Self {
            owner: target.identity.clone(),
            device: target.device.clone(),
            certificate_sha256: owner.certificate_sha256,
            relay: owner.relay,
            agent: target.agent.clone(),
            configured_relay: engine.account().ok_or_else(changed)?.relay_url.clone(),
        })
    }
    fn check(&self, engine: &Engine) -> R<()> {
        let current = Self::capture(
            engine,
            &DefinitionTarget {
                agent: self.agent.clone(),
                identity: self.owner.clone(),
                device: self.device.clone(),
            },
        )?;
        if current != *self {
            return Err(changed());
        }
        Ok(())
    }
    fn key(&self) -> R<String> {
        // Preserve v306's exact key bytes/order. Adding a configured spelling
        // must never hide a retained unknown attempt as an absent receipt.
        #[derive(Serialize)]
        struct AuthorityKey<'a> {
            owner: &'a str,
            device: &'a str,
            certificate_sha256: &'a str,
            relay: &'a str,
            agent: &'a str,
        }
        let authority = AuthorityKey {
            owner: &self.owner,
            device: &self.device,
            certificate_sha256: &self.certificate_sha256,
            relay: &self.relay,
            agent: &self.agent,
        };
        Ok(format!(
            "agent.definition.owner-update.v1:{}",
            sha(&serde_json::to_vec(&authority).map_err(|_| changed())?)
        ))
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    status: String,
    kind: String,
    version: u32,
    bytes: String,
}
impl Record {
    fn from_relay(record: AgentStateRecord) -> R<Self> {
        if record.definition.is_empty() || record.definition.len() > RELAY_DEFINITION_LIMIT {
            // Old relays intentionally omit bytes. Never interpret this as no definition.
            return Err(unsupported());
        }
        let result = Self {
            status: record.status,
            kind: record.kind,
            version: record.definition_version,
            bytes: String::from_utf8(record.definition).map_err(|_| unsupported())?,
        };
        result.status()?;
        if result.version == 0 || result.version > i32::MAX as u32 {
            return Err(unsupported());
        }
        Ok(result)
    }
    fn status(&self) -> R<AgentStatus> {
        match self.status.as_str() {
            "active" => Ok(AgentStatus::Active),
            "paused" => Ok(AgentStatus::Paused),
            "revoked" => Ok(AgentStatus::Revoked),
            _ => Err(unsupported()),
        }
    }
    fn dto(&self, scope: &Scope) -> R<AgentDefinitionDto> {
        let definition = UserDefinition::parse(&self.bytes, self.version)?;
        if self.kind != "User" {
            return Err(unsupported());
        }
        Ok(AgentDefinitionDto {
            agent_id: scope.agent.clone(),
            owner_id: scope.owner.clone(),
            device_id: scope.device.clone(),
            relay_url: scope.configured_relay.clone(),
            status: self.status()?,
            version: self.version,
            definition_sha256: sha(self.bytes.as_bytes()),
            persona: definition.persona,
            rules: definition.rules,
        })
    }
    fn exact(&self, other: &Self) -> bool {
        self.version == other.version && self.kind == other.kind && self.bytes == other.bytes
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema: u8,
    id: String,
    scope: Scope,
    before: Record,
    after: Record,
    preview_digest: String,
}
impl Receipt {
    fn review(&self) -> R<AgentDefinitionPreviewDto> {
        let current = self.before.dto(&self.scope)?;
        let proposed = self.after.dto(&self.scope)?;
        if self.schema != 1
            || self.id.is_empty()
            || self.id.len() > 128
            || current.status == AgentStatus::Revoked
            || current.status != proposed.status
            || current.version.checked_add(1) != Some(proposed.version)
            || (current.persona == proposed.persona && current.rules == proposed.rules)
        {
            return Err(changed());
        }
        let digest = sha(&serde_json::to_vec(&(
            "zoen.agent-definition.review.v1",
            &self.scope,
            &self.before,
            &self.after,
        ))
        .map_err(|_| changed())?);
        if digest != self.preview_digest {
            return Err(changed());
        }
        Ok(AgentDefinitionPreviewDto {
            current,
            proposed,
            preview_digest: digest,
        })
    }
    fn prepare(scope: Scope, before: Record, edit: DefinitionEdit) -> R<Self> {
        if before.version != edit.version
            || sha(before.bytes.as_bytes()) != edit.sha256
            || before.status()? == AgentStatus::Revoked
        {
            return Err(changed());
        }
        before.dto(&scope)?;
        let bytes = UserDefinition::parse(&before.bytes, before.version)?
            .edited(edit.persona, edit.rules)?;
        let after = Record {
            version: before.version + 1,
            bytes,
            ..before.clone()
        };
        let preview_digest =
            sha(
                &serde_json::to_vec(&("zoen.agent-definition.review.v1", &scope, &before, &after))
                    .map_err(|_| changed())?,
            );
        let receipt = Self {
            schema: 1,
            id: types::new_id("adu")?,
            scope,
            before,
            after,
            preview_digest,
        };
        receipt.review()?;
        Ok(receipt)
    }
    fn load(engine: &Engine, scope: &Scope) -> R<Option<Self>> {
        scope.check(engine)?;
        let Some(value) = engine.store.meta(&scope.key()?)? else {
            return Ok(None);
        };
        if value.len() > RECEIPT_LIMIT {
            return Err(changed());
        }
        let receipt: Self = serde_json::from_str(&value).map_err(|_| changed())?;
        if receipt.scope != *scope {
            return Err(changed());
        }
        receipt.review()?;
        Ok(Some(receipt))
    }
    fn outcome(&self, latest: Option<Record>) -> R<AgentDefinitionUpdateDto> {
        let outcome = match latest.as_ref() {
            Some(current) if current.status != "revoked" && current.exact(&self.after) => {
                AgentDefinitionUpdateOutcome::Confirmed
            }
            // This observation cannot cancel or prove non-arrival of an in-flight Define.
            Some(current) if current.status != "revoked" && current.exact(&self.before) => {
                AgentDefinitionUpdateOutcome::Unknown
            }
            Some(_) => AgentDefinitionUpdateOutcome::Conflict,
            None => AgentDefinitionUpdateOutcome::Unknown,
        };
        Ok(AgentDefinitionUpdateDto {
            receipt_id: self.id.clone(),
            review: self.review()?,
            outcome,
            current: latest.and_then(|current| current.dto(&self.scope).ok()),
        })
    }
    fn matches_edit(&self, edit: &DefinitionEdit, preview: &str) -> R<bool> {
        let review = self.review()?;
        Ok(preview == self.preview_digest
            && edit.version == self.before.version
            && edit.sha256 == review.current.definition_sha256
            && edit.persona == review.proposed.persona
            && edit.rules == review.proposed.rules)
    }
}

impl Client {
    async fn definition_request(&self, scope: &Scope, op: Op) -> R<Reply> {
        // Match start_sync's net→engine lock order and select the authenticated
        // connection while the reviewed owner/device/relay are still current.
        let request = {
            let net = self.net.lock().unwrap_or_else(|p| p.into_inner());
            scope.check(&self.lock())?;
            let net = net.as_ref().ok_or_else(changed)?;
            if net
                .authenticated_relay_name(&scope.configured_relay, &scope.owner, &scope.device)
                .is_none()
            {
                return Err(changed());
            }
            net.request_handle()
        };
        request.send(op).await.map_err(invalid)
    }
    async fn definition_record(&self, scope: &Scope) -> R<Record> {
        let response = self
            .definition_request(
                scope,
                Op::Agent(AgentRequest::Status {
                    agent: scope.agent.clone(),
                }),
            )
            .await;
        scope.check(&self.lock())?;
        match response? {
            Reply::AgentState(record) => Record::from_relay(record),
            _ => Err(unsupported()),
        }
    }
    pub(crate) async fn agent_definition_read(
        &self,
        target: DefinitionTarget,
    ) -> R<AgentDefinitionDto> {
        let scope = Scope::capture(&self.lock(), &target)?;
        self.definition_record(&scope).await?.dto(&scope)
    }
    pub(crate) async fn agent_definition_preview(
        &self,
        target: DefinitionTarget,
        edit: DefinitionEdit,
    ) -> R<AgentDefinitionPreviewDto> {
        let scope = Scope::capture(&self.lock(), &target)?;
        let before = self.definition_record(&scope).await?;
        Receipt::prepare(scope, before, edit)?.review()
    }
    async fn reconcile_definition_receipt(&self, receipt: Receipt) -> R<AgentDefinitionUpdateDto> {
        let latest = self.definition_record(&receipt.scope).await.ok();
        let engine = self.lock();
        if Receipt::load(&engine, &receipt.scope)?.is_none_or(|current| current.id != receipt.id) {
            return Err(changed());
        }
        receipt.outcome(latest)
    }
    pub(crate) async fn agent_definition_reconcile(
        &self,
        target: DefinitionTarget,
    ) -> R<Option<AgentDefinitionUpdateDto>> {
        let scope = Scope::capture(&self.lock(), &target)?;
        let receipt = Receipt::load(&self.lock(), &scope)?;
        match receipt {
            Some(receipt) => self.reconcile_definition_receipt(receipt).await.map(Some),
            None => Ok(None),
        }
    }
    pub(crate) async fn agent_definition_update(
        &self,
        target: DefinitionTarget,
        edit: DefinitionEdit,
        preview: &str,
    ) -> R<AgentDefinitionUpdateDto> {
        let scope = Scope::capture(&self.lock(), &target)?;
        let held = Receipt::load(&self.lock(), &scope)?;
        if let Some(receipt) = held {
            if !receipt.matches_edit(&edit, preview)? {
                return Err(changed());
            }
            // Duplicate dispatches reconcile only, even after observing the old base.
            return self.reconcile_definition_receipt(receipt).await;
        }
        let before = self.definition_record(&scope).await?;
        let receipt = Receipt::prepare(scope, before, edit)?;
        if receipt.preview_digest != preview {
            return Err(changed());
        }
        {
            let engine = self.lock();
            if Receipt::load(&engine, &receipt.scope)?.is_some() {
                return Err(changed());
            }
            let value = serde_json::to_string(&receipt).map_err(|_| changed())?;
            if value.len() > RECEIPT_LIMIT {
                return Err(changed());
            }
            // Persist protected immutable before/after/review before any possible dispatch.
            engine.store.set_meta(&receipt.scope.key()?, &value)?;
            if engine.store.meta(&receipt.scope.key()?)?.as_deref() != Some(value.as_str()) {
                return Err(changed());
            }
        }
        // The relay serializes monotonic writes. Always observed+1; never bump on
        // conflict. This is not an expected-hash server CAS. A lost reply may arrive
        // later, so even observing the exact old base remains Unknown.
        let _ack = self
            .definition_request(
                &receipt.scope,
                Op::Agent(AgentRequest::Define {
                    agent: receipt.scope.agent.clone(),
                    version: receipt.after.version,
                    kind: receipt.after.kind.clone(),
                    definition: receipt.after.bytes.as_bytes().to_vec(),
                }),
            )
            .await;
        self.reconcile_definition_receipt(receipt).await
    }
    pub(crate) fn agent_definition_dismiss(
        &self,
        target: DefinitionTarget,
        id: &str,
        preview: &str,
    ) -> R<bool> {
        let scope = Scope::capture(&self.lock(), &target)?;
        let engine = self.lock();
        let Some(receipt) = Receipt::load(&engine, &scope)? else {
            return Ok(false);
        };
        if receipt.id != id || receipt.preview_digest != preview {
            return Ok(false);
        }
        // Forgetting a protected receipt does not cancel an already dispatched Define.
        engine.store.meta_delete(&scope.key()?)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests;
