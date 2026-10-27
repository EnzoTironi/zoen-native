//! Rust-only turn authority for a bounded isolated artifact worker. No actor,
//! device, grant or source-message authority comes from a model/browser payload.
use crate::{
    engine::{Engine, EntryBody, R},
    AppBinaryDocumentDto, Client, CoreError,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use sha2::{Digest, Sha256};
use types::{IdentityKind, ItemContent};

pub(crate) mod source_scope;
pub(crate) mod tool_approvals;
pub use tool_approvals::{AgentToolApprovalInput, ApprovedToolTurn};

pub const AGENT_BINARY_DOCUMENT_LIMIT: usize = 512 * 1024;
fn denied() -> CoreError {
    CoreError::Forbidden {
        reason: "The agent's artifact or source-message authority changed.".into(),
    }
}
fn invalid() -> CoreError {
    CoreError::Invalid {
        reason: "This isolated presentation executor supports documents up to 512 KiB.".into(),
    }
}

/// Minted by the signed native core; not serializable, FFI-exported or writable by
/// a browser. An asynchronous worker must return through the rechecking methods.
pub struct AgentBinaryTurn {
    document: AppBinaryDocumentDto,
    device: String,
    owner: String,
    space: String,
    source: String,
    serving: String,
    source_sha256: String,
    approval: Option<tool_approvals::ApprovalBinding>,
}
impl AgentBinaryTurn {
    pub fn document(&self) -> &AppBinaryDocumentDto {
        &self.document
    }
    pub fn source_sha256(&self) -> &str {
        &self.source_sha256
    }
}
impl Engine {
    pub(crate) fn agent_binary_authority(
        &self,
        item: &str,
        source: &str,
        serving: &str,
        write: bool,
    ) -> R<(String, String, String, String)> {
        let state = self.item_state(item)?;
        let binding = self.agent_source_binding(&state.space, source, serving, write)?;
        self.check_artifact_head(item, source, &binding.3, &state.space)?;
        Ok(binding)
    }

    /// Same signed source authority before an artifact exists. Creation cannot
    /// borrow permissions from a different item or a model-supplied actor.
    pub(crate) fn agent_source_authority(
        &self,
        space: &str,
        source: &str,
        serving: &str,
        write: bool,
    ) -> R<(String, String, String, String)> {
        let binding = self.agent_source_binding(space, source, serving, write)?;
        if self.artifact_request(space, source)?.is_some() {
            return Err(denied());
        }
        Ok(binding)
    }
    fn agent_source_binding(
        &self,
        space: &str,
        source: &str,
        serving: &str,
        write: bool,
    ) -> R<(String, String, String, String)> {
        let (me, device) = self.app_document_identity(space)?;
        // Local demo personas and an unsigned owner string cannot authorize this port.
        let account = self.account().ok_or_else(denied)?;
        if account.identity != me || account.device != device {
            return Err(denied());
        }
        let identity = self.identities.get(&me).ok_or_else(denied)?;
        let owner = identity.owner.clone().ok_or_else(denied)?;
        if identity.kind != IdentityKind::Agent
            || !event_log::agent_owner::profile_authorized(identity)
        {
            return Err(denied());
        }
        if !self.is_member(space, serving) || (write && !self.state.writer(space, serving)) {
            return Err(denied());
        }
        let entry = self
            .space_state(space)?
            .entries
            .iter()
            .find(|entry| entry.id() == source)
            .ok_or_else(denied)?;
        let EntryBody::Message {
            text,
            attaches,
            reply,
            marks,
        } = &entry.body
        else {
            return Err(denied());
        };
        if entry.author != serving || entry.author == me || marks.deleted || text.trim().is_empty()
        {
            return Err(denied());
        }
        if reply
            .as_ref()
            .and_then(|r| r.artifact.as_ref())
            .and_then(|r| r.agent_id.as_deref())
            .is_some_and(|id| id != me || entry.author != owner)
        {
            return Err(denied());
        }
        let source_bytes = serde_json::to_vec(&(entry.id(), &entry.author, text, attaches, reply))
            .map_err(|_| denied())?;
        Ok((
            device,
            owner,
            space.into(),
            hex::encode(Sha256::digest(source_bytes)),
        ))
    }
    fn recheck_agent_binary_turn(&self, turn: &AgentBinaryTurn, write: bool) -> R<()> {
        let (device, owner, space, source_sha) = self.agent_binary_authority(
            &turn.document.item_id,
            &turn.source,
            &turn.serving,
            write,
        )?;
        if self.me_id()? != turn.document.actor_id
            || device != turn.device
            || owner != turn.owner
            || space != turn.space
            || source_sha != turn.source_sha256
        {
            return Err(denied());
        }
        if write
            && matches!(
                self.decide(
                    &turn.document.actor_id,
                    &turn.space,
                    &types::ActionClass::Reversible,
                    0
                ),
                grants::Decision::Block(_)
            )
        {
            return Err(denied());
        }
        self.check_artifact_version(turn)?;
        self.recheck_tool_turn(turn, None)?;
        Ok(())
    }
}
impl Client {
    /// Select the exact observed source, including an old base needed to reconcile
    /// a lost result. Authorization and the executor's smaller capacity precede IO.
    pub fn agent_binary_turn(
        &self,
        item: &str,
        source: &str,
        serving: &str,
        version: Option<u32>,
        sha256: Option<&str>,
    ) -> R<AgentBinaryTurn> {
        if version.is_some() != sha256.is_some() {
            return Err(invalid());
        }
        let e = self.lock();
        let (device, owner, space, source_sha256) =
            e.agent_binary_authority(item, source, serving, false)?;
        let state = e.item_state(item)?;
        let selected = match version {
            Some(v) => state
                .versions
                .iter()
                .find(|vrs| vrs.number == v)
                .ok_or_else(invalid)?,
            None => state.current(),
        };
        if e.artifact_request(&space, source)?
            .is_some_and(|r| selected.number < r.version)
        {
            return Err(denied());
        }
        let ItemContent::App(app) = &selected.content else {
            return Err(invalid());
        };
        if app
            .document
            .as_ref()
            .is_none_or(|file| file.bytes > AGENT_BINARY_DOCUMENT_LIMIT as u64)
        {
            return Err(invalid());
        }
        let document = e.app_binary_read(item, version, sha256)?;
        Ok(AgentBinaryTurn {
            document,
            device,
            owner,
            space,
            source: source.into(),
            serving: serving.into(),
            source_sha256,
            approval: None,
        })
    }
    /// A worker result must not reveal the inspected source after access or its
    /// triggering message changes while the isolated computation is in flight.
    pub fn agent_binary_finish_read(&self, turn: &AgentBinaryTurn) -> R<()> {
        self.lock().recheck_agent_binary_turn(turn, false)
    }
    /// Must run before isolated compute. Some is a durable accepted result; None
    /// means this exact base and present grants still permit the first execution.
    pub fn agent_binary_result(
        &self,
        turn: &AgentBinaryTurn,
        mutation: &str,
        intent_sha256: &str,
    ) -> R<Option<AppBinaryDocumentDto>> {
        let e = self.lock();
        e.recheck_agent_binary_turn(turn, true)?;
        e.recheck_tool_turn(turn, Some((mutation, intent_sha256)))?;
        e.app_binary_result(
            &turn.document.item_id,
            turn.document.version,
            &turn.document.sha256,
            mutation,
            intent_sha256,
        )
    }
    /// Recheck source, actual certified actor/device, served writer, App Act and
    /// canonical CAS after compute; reuse the existing atomic receipt/file store.
    pub fn agent_binary_save(
        &self,
        turn: &AgentBinaryTurn,
        mutation: &str,
        intent_sha256: &str,
        data_base64: &str,
    ) -> R<AppBinaryDocumentDto> {
        if data_base64.is_empty() || data_base64.len() > AGENT_BINARY_DOCUMENT_LIMIT.div_ceil(3) * 4
        {
            return Err(invalid());
        }
        let bytes = STANDARD.decode(data_base64).map_err(|_| invalid())?;
        if bytes.len() > AGENT_BINARY_DOCUMENT_LIMIT {
            return Err(invalid());
        }
        let mut e = self.lock();
        e.recheck_agent_binary_turn(turn, true)?;
        e.recheck_tool_turn(turn, Some((mutation, intent_sha256)))?;
        let receipt = e.artifact_receipt(turn)?;
        e.app_binary_save_with_source(
            &turn.document.item_id,
            turn.document.version,
            &turn.document.sha256,
            data_base64,
            (mutation, intent_sha256, receipt),
        )
    }
}

#[cfg(test)]
pub(crate) mod tests;

#[cfg(test)]
mod source_cohort_tests;
#[cfg(test)]
mod source_scope_tests;
