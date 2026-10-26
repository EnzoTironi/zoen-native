//! Closed owned-Excalidraw commands on the existing signed review authority.
use crate::{
    agent_binary_documents::tool_approvals as shared,
    engine::{Engine, State, R},
    AppDocumentDto, Client, CoreError,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use types::{ActionClass, AgentRequest, ItemContent, ToolProposal};

mod effects;
mod recovery;
mod review;
#[cfg(test)]
mod tests;
pub mod tools;
pub(crate) use effects::approved_effect;
pub use review::ApprovedSceneTurn;
pub(crate) use review::{access, matches, parsed};
pub const SCHEMA: u8 = 6;

fn invalid() -> CoreError {
    CoreError::Invalid {
        reason: crate::i18n::t("Alteração de desenho inválida.", "Invalid drawing change."),
    }
}
fn stale() -> CoreError {
    CoreError::Stale {
        reason: crate::i18n::t(
            "O desenho ou o acesso mudou. Leia novamente.",
            "The drawing or access changed. Read it again.",
        ),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SceneSnapshot {
    pub artifact_id: String,
    pub space_id: String,
    pub actor_id: String,
    pub version: u32,
    pub sha256: String,
    pub resource_uri: String,
    pub title: String,
    pub scene_json: String,
}
pub struct AgentSceneTurn {
    snapshot: SceneSnapshot,
    device: String,
    owner: String,
    source: String,
    serving: String,
    source_sha256: String,
    approval: Option<(String, String)>,
}
impl AgentSceneTurn {
    #[must_use]
    pub fn snapshot(&self) -> &SceneSnapshot {
        &self.snapshot
    }
}

/// Provenance augments an existing document receipt; it is not another ledger.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Source {
    pub(crate) source: String,
    pub(crate) source_sha256: String,
    pub(crate) request_id: String,
    pub(crate) base_version: u32,
    pub(crate) base_sha256: String,
    pub(crate) resource_uri: String,
}
impl Source {
    pub(crate) fn valid(&self) -> bool {
        tools::token(&self.source)
            && tools::token(&self.request_id)
            && tools::digest_valid(&self.source_sha256)
            && tools::digest_valid(&self.base_sha256)
            && self.base_version > 0
            && self.resource_uri.starts_with("ui://zoen/package/")
            && self.resource_uri.len() <= 256
    }
}
impl Engine {
    fn scene_snapshot_at(&self, item: &str, version: u32) -> R<SceneSnapshot> {
        let state = self.item_state(item)?;
        let (actor, _) = self.app_document_identity(&state.space)?;
        let selected = state
            .versions
            .iter()
            .find(|v| v.number == version)
            .ok_or_else(stale)?;
        let ItemContent::App(app) = &selected.content else {
            return Err(invalid());
        };
        if state.kind != types::ItemKind::App
            || app.view.is_none()
            || !crate::app_resources::valid(app)
        {
            return Err(invalid());
        }
        let document = app.document.as_ref().ok_or_else(invalid)?;
        if document.mime != miniapps::excalidraw::MIME
            || document.voice.is_some()
            || document.bytes > tools::SCENE_LIMIT as u64
        {
            return Err(invalid());
        }
        let bytes = self
            .owned_file_bytes(document, tools::SCENE_LIMIT as u64)?
            .ok_or_else(stale)?;
        let scene_json = String::from_utf8(bytes).map_err(|_| invalid())?;
        tools::scene_valid(&scene_json)?;
        Ok(SceneSnapshot {
            artifact_id: item.into(),
            space_id: state.space.clone(),
            actor_id: actor,
            version,
            sha256: document.sha256.clone(),
            resource_uri: app.resource_uri.clone(),
            title: app.title.clone(),
            scene_json,
        })
    }
    fn recheck_scene_turn(&self, turn: &AgentSceneTurn, write: bool) -> R<()> {
        let (device, owner, space, source) = self.agent_binary_authority(
            &turn.snapshot.artifact_id,
            &turn.source,
            &turn.serving,
            write,
        )?;
        if self.me_id()? != turn.snapshot.actor_id
            || device != turn.device
            || owner != turn.owner
            || space != turn.snapshot.space_id
            || source != turn.source_sha256
            || !self.state.writer(&space, &owner)
            || !self.state.writer(&space, &turn.snapshot.actor_id)
            || matches!(
                self.decide(
                    &turn.snapshot.actor_id,
                    &space,
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
        self.check_app_resource_pin(&turn.snapshot.artifact_id, &turn.snapshot.resource_uri)
    }
}
impl Client {
    pub fn agent_scene_turn(&self, item: &str, source: &str, serving: &str) -> R<AgentSceneTurn> {
        let e = self.lock();
        let (device, owner, _, source_sha256) =
            e.agent_binary_authority(item, source, serving, false)?;
        let version = e.item_state(item)?.current().number;
        let turn = AgentSceneTurn {
            snapshot: e.scene_snapshot_at(item, version)?,
            device,
            owner,
            source: source.into(),
            serving: serving.into(),
            source_sha256,
            approval: None,
        };
        e.recheck_scene_turn(&turn, false)?;
        Ok(turn)
    }
    pub fn agent_scene_finish_read(&self, turn: &AgentSceneTurn) -> R<SceneSnapshot> {
        let e = self.lock();
        e.recheck_scene_turn(turn, false)?;
        let now = e.scene_snapshot_at(
            &turn.snapshot.artifact_id,
            e.item_state(&turn.snapshot.artifact_id)?.current().number,
        )?;
        if now != turn.snapshot {
            return Err(stale());
        }
        if let Some((id, hash)) = &turn.approval {
            let (r, p) = e.tool_resolution(id, hash)?;
            if p.schema != SCHEMA
                || p.canonical_tool != "read_scene"
                || p.expires_at_ms <= crate::engine::now_ms()
                || !e.state.tool_review_current(r)
                || !matches(&e.state, &r.req, &r.space)
            {
                return Err(stale());
            }
        }
        Ok(now)
    }
}
