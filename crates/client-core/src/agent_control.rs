//! Durable lifecycle intent. Relay citizen revision is the only lifecycle authority.
//! Recovery observes; it never retries a request whose delivery is unknown.
use crate::{
    agent_definition::{DefinitionTarget, Scope},
    api::invalid,
    AgentControl, AgentStatus, Client, CoreError, Engine,
};
use protocol::{agent_control::Record, AgentAction, AgentRequest, Op, Reply};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
type R<T> = Result<T, CoreError>;
fn changed() -> CoreError {
    invalid("The account or reviewed agent lifecycle changed. Refresh it again.")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub enum AgentControlOutcome {
    Observed,
    Unknown,
    Confirmed,
    Superseded,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct AgentControlDto {
    pub status: AgentStatus,
    pub outcome: AgentControlOutcome,
    pub revision: String,
    pub operation_id: Option<String>,
    /// A durable unresolved intent; observing its old base never clears it.
    pub pending: Option<AgentControl>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Intent {
    scope: Scope,
    operation: String,
    revision: u64,
    action: AgentControl,
}
fn action(value: AgentControl) -> AgentAction {
    match value {
        AgentControl::Pause => AgentAction::Pause,
        AgentControl::Resume => AgentAction::Resume,
        AgentControl::Revoke => AgentAction::Revoke,
    }
}
fn target_status(value: AgentControl) -> &'static str {
    match value {
        AgentControl::Pause => "paused",
        AgentControl::Resume => "active",
        AgentControl::Revoke => "revoked",
    }
}
fn key(scope: &Scope) -> R<String> {
    let binding = (
        &scope.owner,
        &scope.device,
        &scope.certificate_sha256,
        &scope.relay,
        &scope.agent,
    );
    Ok(format!(
        "agent.control.intent.v1:{}",
        hex::encode(Sha256::digest(
            serde_json::to_vec(&binding).map_err(|_| changed())?
        ))
    ))
}
impl Intent {
    fn load(engine: &Engine, scope: &Scope) -> R<Option<Self>> {
        scope.check(engine)?;
        let Some(value) = engine.store.meta(&key(scope)?)? else {
            return Ok(None);
        };
        if value.len() > 4096 {
            return Err(changed());
        }
        let held: Self = serde_json::from_str(&value).map_err(|_| changed())?;
        if held.scope != *scope
            || held.operation.len() != 64
            || !held.operation.bytes().all(|b| b.is_ascii_hexdigit())
            || held.revision == 0
            || held.revision >= i64::MAX as u64
        {
            return Err(changed());
        }
        Ok(Some(held))
    }
    fn outcome(&self, record: &Record) -> R<AgentControlOutcome> {
        if record.revision < self.revision {
            return Err(changed());
        }
        if record.revision == self.revision {
            return Ok(AgentControlOutcome::Unknown);
        }
        // Every later revision fences this request. An exact receipt confirms it;
        // a different winner supersedes it. Neither permits this operation to run again.
        if record.operation == self.operation
            && (record.revision != self.revision + 1
                || record.action != action(self.action).id()
                || record.device != self.scope.device
                || record.status != target_status(self.action))
        {
            return Err(changed());
        }
        Ok(if record.operation == self.operation {
            AgentControlOutcome::Confirmed
        } else {
            AgentControlOutcome::Superseded
        })
    }
}
fn status(record: &Record) -> R<AgentStatus> {
    let hex_id = |value: &str| value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit());
    let receipt_valid = if record.operation.is_empty() {
        record.action.is_empty() && record.device.is_empty()
    } else {
        hex_id(&record.operation)
            && hex_id(&record.device)
            && matches!(
                (record.action.as_str(), record.status.as_str()),
                ("pause", "paused") | ("resume", "active") | ("revoke", "revoked")
            )
    };
    if record.revision == 0 || record.revision > i64::MAX as u64 || !receipt_valid {
        return Err(changed());
    }
    match record.status.as_str() {
        "active" => Ok(AgentStatus::Active),
        "paused" => Ok(AgentStatus::Paused),
        "revoked" => Ok(AgentStatus::Revoked),
        _ => Err(changed()),
    }
}
impl Client {
    async fn control_record(&self, scope: &Scope) -> R<Record> {
        let response = self
            .owner_agent_request(
                scope,
                Op::Agent(AgentRequest::ControlStatus {
                    agent: scope.agent.clone(),
                }),
            )
            .await;
        scope.check(&self.lock())?;
        let Reply::AgentControl(record) = response? else {
            return Err(changed());
        };
        status(&record)?;
        Ok(record)
    }
    fn control_observation(&self, scope: &Scope, record: Record) -> R<AgentControlDto> {
        let status = status(&record)?;
        let engine = self.lock();
        let held = Intent::load(&engine, scope)?;
        let operation_id = held.as_ref().map(|intent| intent.operation.clone());
        let (pending, outcome) = if let Some(intent) = held {
            let outcome = intent.outcome(&record)?;
            if outcome != AgentControlOutcome::Unknown {
                engine.store.meta_delete(&key(scope)?)?;
                (None, outcome)
            } else {
                (Some(intent.action), outcome)
            }
        } else {
            (None, AgentControlOutcome::Observed)
        };
        Ok(AgentControlDto {
            status,
            outcome,
            revision: record.revision.to_string(),
            operation_id,
            pending,
        })
    }
    pub(crate) async fn agent_control_read(&self, target: DefinitionTarget) -> R<AgentControlDto> {
        let scope = Scope::capture(&self.lock(), &target)?;
        self.control_observation(&scope, self.control_record(&scope).await?)
    }
    pub(crate) async fn agent_control_change(
        &self,
        target: DefinitionTarget,
        observed: &str,
        requested: AgentControl,
    ) -> R<AgentControlDto> {
        let scope = Scope::capture(&self.lock(), &target)?;
        // A duplicate consumer request only reconciles, including after process restart.
        let held = Intent::load(&self.lock(), &scope)?;
        if let Some(held) = held {
            if held.action != requested || held.revision.to_string() != observed {
                return Err(changed());
            }
            return self.control_observation(&scope, self.control_record(&scope).await?);
        }
        let current = self.control_record(&scope).await?;
        if observed != current.revision.to_string() || current.status == "revoked" {
            return Err(changed());
        }
        let mut nonce = [0_u8; 32];
        getrandom::getrandom(&mut nonce).map_err(|_| changed())?;
        let intent = Intent {
            scope: scope.clone(),
            operation: hex::encode(nonce),
            revision: current.revision,
            action: requested,
        };
        {
            let engine = self.lock();
            if Intent::load(&engine, &scope)?.is_some() {
                return Err(changed());
            }
            let value = serde_json::to_string(&intent).map_err(|_| changed())?;
            if value.len() > 4096 {
                return Err(changed());
            }
            engine.store.set_meta(&key(&scope)?, &value)?;
            if engine.store.meta(&key(&scope)?)?.as_deref() != Some(value.as_str()) {
                return Err(changed());
            }
        }
        self.dispatch_control_intent(intent).await
    }
    pub(crate) async fn agent_control_retry(
        &self,
        target: DefinitionTarget,
        observed: &str,
        operation: &str,
        requested: AgentControl,
    ) -> R<AgentControlDto> {
        let scope = Scope::capture(&self.lock(), &target)?;
        let held = Intent::load(&self.lock(), &scope)?.ok_or_else(changed)?;
        if held.action != requested
            || held.operation != operation
            || held.revision.to_string() != observed
        {
            return Err(changed());
        }
        let record = self.control_record(&scope).await?;
        if held.outcome(&record)? != AgentControlOutcome::Unknown {
            return self.control_observation(&scope, record);
        }
        {
            let engine = self.lock();
            if Intent::load(&engine, &scope)?
                .is_none_or(|current| current.operation != held.operation)
            {
                return Err(changed());
            }
        }
        // Only an explicit user retry reaches this path. Exact durable bytes;
        // current revision preflight plus server CAS prevent a second effect.
        self.dispatch_control_intent(held).await
    }
    async fn dispatch_control_intent(&self, intent: Intent) -> R<AgentControlDto> {
        let scope = &intent.scope;
        let response = self
            .owner_agent_request(
                scope,
                Op::Agent(AgentRequest::ControlReviewed {
                    agent: scope.agent.clone(),
                    action: action(intent.action),
                    operation: intent.operation.clone(),
                    revision: intent.revision,
                }),
            )
            .await;
        scope.check(&self.lock())?;
        match response {
            Ok(Reply::AgentControl(record)) => self.control_observation(scope, record),
            // A lost ACK never turns into permission to send the opposite control.
            _ => self.control_observation(scope, self.control_record(scope).await?),
        }
    }
}
