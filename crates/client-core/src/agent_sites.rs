//! Private Sites use `FileDoc` versions and the existing signed tool approvals.
//! This family has no hosting destination, network or publication operation.
use crate::{
    agent_binary_documents::tool_approvals as shared,
    engine::{Engine, State, R},
    Client, CoreError,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use types::{ActionClass, AgentRequest, ItemContent, ItemKind, Privacy, ToolProposal};

mod contract;
mod execution;
mod preview;
#[cfg(feature = "client-types")]
pub(crate) use preview::SiteToolReview;
mod review;
pub(crate) use review::{access, matches, parsed};
mod recovery;
pub use contract::{
    definition, definitions, executor_binding, is_model_name, model_name, parse,
    selected_definition, Command,
};
pub use execution::ApprovedSiteTool;
#[cfg(test)]
mod tests;

pub const REVISION: &str = "zoen.native-site.file.v1";
pub const SCHEMA: u8 = 5;
const HTML_LIMIT: u32 = 16 * 1024;
const ARGUMENT_LIMIT: usize = 32 * 1024;

fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn invalid() -> CoreError {
    CoreError::Invalid {
        reason: "Invalid private Site command.".into(),
    }
}
fn stale() -> CoreError {
    CoreError::Stale {
        reason: "The reviewed Site, source or access changed.".into(),
    }
}
fn token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}
fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn html_valid(value: &str) -> bool {
    !value.is_empty()
        && u32::try_from(value.len()).is_ok_and(|length| length <= HTML_LIMIT)
        && !value.contains('\0')
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SiteSnapshot {
    pub artifact_id: String,
    pub space_id: String,
    pub version: u32,
    pub sha256: String,
    pub name: String,
    pub html: String,
    pub actor_id: String,
}

impl Engine {
    fn agent_site_authority(
        &self,
        space: &str,
        source: &str,
        serving: &str,
        write: bool,
    ) -> R<(String, String, String, String)> {
        let binding = self.agent_source_authority(space, source, serving, write)?;
        let state = self.space_state(space)?;
        let actor = self.me_id()?;
        if state.privacy != Privacy::EndToEnd
            || state.integrity_error.is_some()
            || self.broken_spaces.contains_key(space)
            || !self.is_member(space, &binding.1)
            || (write
                && (!self.state.writer(space, &actor) || !self.state.writer(space, &binding.1)))
            || matches!(
                self.decide(
                    &actor,
                    space,
                    &if write {
                        ActionClass::Reversible
                    } else {
                        ActionClass::Reply
                    },
                    0
                ),
                grants::Decision::Block(_)
            )
        {
            return Err(stale());
        }
        Ok(binding)
    }
    fn agent_site_snapshot(&self, item: &str, version: u32, expected_sha: &str) -> R<SiteSnapshot> {
        let state = self.item_state(item)?;
        if state.kind != ItemKind::File || self.state.is_recovery_item(item) {
            return Err(stale());
        }
        let info = self.file_info_at(item, Some(version))?;
        if version == 0
            || !digest(expected_sha)
            || info.sha256 != expected_sha
            || info.mime != "text/html"
            || info.bytes > u64::from(HTML_LIMIT)
        {
            return Err(stale());
        }
        let bytes = self
            .file_bytes_bounded(item, Some(version), u64::from(HTML_LIMIT))?
            .ok_or_else(stale)?;
        let html = String::from_utf8(bytes).map_err(|_| invalid())?;
        if !html_valid(&html) {
            return Err(invalid());
        }
        Ok(SiteSnapshot {
            artifact_id: item.into(),
            space_id: state.space.clone(),
            version,
            sha256: info.sha256,
            name: info.name,
            html,
            actor_id: self.me_id()?,
        })
    }
}

impl Client {
    /// Discovery rechecks the certified agent and triggering message. It returns
    /// metadata only; reading HTML goes through the explicit selected-source port.
    pub fn agent_site_sources(
        &self,
        space: &str,
        source: &str,
        serving: &str,
    ) -> Vec<(String, u32, String)> {
        let e = self.lock();
        if e.agent_site_authority(space, source, serving, false)
            .is_err()
        {
            return vec![];
        }
        let mut out: Vec<_> = e
            .state
            .items
            .values()
            .filter(|it| {
                it.space == space && it.kind == ItemKind::File && !e.state.is_recovery_item(&it.id)
            })
            .filter_map(|it| {
                let ItemContent::File(file) = &it.current().content else {
                    return None;
                };
                (file.mime == "text/html" && file.bytes > 0 && file.bytes <= u64::from(HTML_LIMIT))
                    .then(|| (it.id.clone(), it.current().number, file.sha256.clone()))
            })
            .collect();
        out.sort();
        out
    }
    pub fn can_create_agent_site(&self, space: &str, source: &str, serving: &str) -> bool {
        self.lock()
            .agent_site_authority(space, source, serving, true)
            .is_ok()
    }
    pub fn agent_site_read(
        &self,
        item: &str,
        version: u32,
        expected_sha: &str,
        source: &str,
        serving: &str,
    ) -> R<SiteSnapshot> {
        let e = self.lock();
        let space = e.item_state(item)?.space.clone();
        e.agent_site_authority(&space, source, serving, false)?;
        e.agent_site_snapshot(item, version, expected_sha)
    }
}
