//! The existing document receipt proves completion; the signed request proves consent.
use super::*;
use types::{Event, EventBody, ToolDecision};

fn replacement(p: &ToolProposal) -> R<tools::ReplaceScene> {
    let (_, action) = parsed(p)?;
    let args = action.ok_or_else(invalid)?;
    match tools::parse(&p.canonical_tool, &args)? {
        tools::Command::Replace(c) => Ok(*c),
        _ => Err(invalid()),
    }
}
fn approved(r: &crate::engine::RequestState) -> bool {
    r.status == crate::engine::ReqStatus::Approved
        && r.resolution.as_ref().and_then(|r| r.tool_decision) == Some(ToolDecision::ApproveOnce)
}
pub(super) fn live_approval(state: &State, r: &crate::engine::RequestState, at: i64) -> bool {
    approved(r)
        && r.req
            .tool_proposal
            .as_ref()
            .is_some_and(|p| p.schema == SCHEMA && p.expires_at_ms > at)
        && state.tool_review_active_leaf(r)
}
fn expected_state(scene: &str, title: &str) -> R<String> {
    let parsed = miniapps::excalidraw::Scene::parse(scene).map_err(|_| invalid())?;
    let mut value: Value = serde_json::from_str(&parsed.metadata()).map_err(|_| invalid())?;
    value["title"] = json!(title);
    Ok(value.to_string())
}
pub(crate) fn approved_effect(state: &State, event: &Event, owner: &str) -> bool {
    let EventBody::ItemVersioned {
        item,
        content: ItemContent::App(after),
        ..
    } = &event.body
    else {
        return false;
    };
    let Some(it) = state.items.get(item) else {
        return false;
    };
    let ItemContent::App(before) = &it.current().content else {
        return false;
    };
    state
        .requests
        .values()
        .filter(|r| {
            r.space == event.space
                && r.req.agent == event.author
                && live_approval(state, r, event.at_ms)
        })
        .any(|r| {
            let Some(p) = r.req.tool_proposal.as_deref() else {
                return false;
            };
            let Ok(c) = replacement(p) else {
                return false;
            };
            let Some(file) = &after.document else {
                return false;
            };
            p.owner == owner
                && p.device.as_str() == event.device.as_deref().unwrap_or("")
                && p.artifact == *item
                && p.expires_at_ms > event.at_ms
                && state.tool_review_current(r)
                && matches(state, &r.req, &r.space)
                && access(state, p, event.at_ms)
                && after.app == before.app
                && after.view == before.view
                && after.resource_uri == before.resource_uri
                && after.title == before.title
                && c.resource_uri == before.resource_uri
                && file.sha256 == tools::digest(c.scene_json.as_bytes())
                && file.bytes == c.scene_json.len() as u64
                && file.mime == miniapps::excalidraw::MIME
                && file.path.is_empty()
                && file.voice.is_none()
                && file.app_origin == before.document.as_ref().and_then(|d| d.app_origin.clone())
                && crate::files::validate_file(file, tools::SCENE_LIMIT as u64).is_ok()
                && expected_state(&c.scene_json, &before.title)
                    .is_ok_and(|state| state == after.state_json)
        })
}

impl Engine {
    pub(crate) fn scene_save_source(
        &self,
        item: &str,
        version: u32,
        expected: (&str, Option<&str>),
        scene: &str,
        mutation: &str,
    ) -> R<Option<Source>> {
        let actor = self.me_id()?;
        if !self
            .identities
            .get(&actor)
            .is_some_and(|id| id.kind == types::IdentityKind::Agent)
        {
            return Ok(None);
        }
        let now = crate::engine::now_ms();
        for r in self
            .state
            .requests
            .values()
            .filter(|r| r.req.agent == actor && live_approval(&self.state, r, now))
        {
            let Some(p) = r
                .req
                .tool_proposal
                .as_deref()
                .filter(|p| p.schema == SCHEMA && p.artifact == item)
            else {
                continue;
            };
            if !review::mutation(p).is_ok_and(|id| id == mutation) {
                continue;
            }
            self.tool_resolution(&r.req.id, &r.req.content_hash)?;
            let c = replacement(p)?;
            let (device, owner, space, source) =
                self.agent_binary_authority(item, &p.source, &p.serving, true)?;
            if c.version != version
                || c.sha256 != expected.0
                || Some(c.resource_uri.as_str()) != expected.1
                || c.scene_json != scene
                || device != p.device
                || owner != p.owner
                || space != r.space
                || source != p.source_sha256
                || p.expires_at_ms <= crate::engine::now_ms()
                || !self.state.tool_review_current(r)
                || !matches(&self.state, &r.req, &r.space)
                || !access(&self.state, p, crate::engine::now_ms())
            {
                return Err(stale());
            }
            let scoped = self.artifact_request(&space, &p.source)?;
            return Ok(Some(Source {
                source: p.source.clone(),
                source_sha256: p.source_sha256.clone(),
                request_id: scoped.map_or_else(|| p.source.clone(), |s| s.request_id),
                base_version: version,
                base_sha256: c.sha256,
                resource_uri: c.resource_uri,
            }));
        }
        Ok(None)
    }
    pub(crate) fn scene_result(&self, p: &ToolProposal) -> R<Option<AppDocumentDto>> {
        let c = replacement(p)?;
        let actor = self.me_id()?;
        let (_, receipts) = self.document_receipts(&actor, &p.device, &p.artifact)?;
        let mutation = review::mutation(p)?;
        let Some(receipt) = receipts.iter().find(|r| r.id == mutation) else {
            return Ok(None);
        };
        let sha = tools::digest(c.scene_json.as_bytes());
        let first = tools::digest(
            &serde_json::to_vec(&(&actor, &p.device, &p.artifact, c.version, &c.sha256, &sha))
                .map_err(|_| invalid())?,
        );
        let fingerprint =
            tools::digest(&serde_json::to_vec(&(first, &c.resource_uri)).map_err(|_| invalid())?);
        let state = self.item_state(&p.artifact)?;
        let version = state
            .versions
            .iter()
            .find(|v| v.number == receipt.version)
            .ok_or_else(stale)?;
        if receipt.fingerprint != fingerprint
            || receipt.sha256 != sha
            || receipt.version != c.version.checked_add(1).ok_or_else(invalid)?
            || version.author != actor
            || !matches!(&version.content, ItemContent::App(app) if app.resource_uri == c.resource_uri
                && app.document.as_ref().is_some_and(|d| d.sha256 == sha
                    && d.bytes == c.scene_json.len() as u64
                    && d.mime == miniapps::excalidraw::MIME && d.voice.is_none()
                    && crate::files::validate_file(d, tools::SCENE_LIMIT as u64).is_ok()))
        {
            return Err(stale());
        }
        // The frozen signed proposal already contains these exact result bytes.
        // Historical cache eviction cannot turn a verified receipt into a retry.
        Ok(Some(AppDocumentDto {
            item_id: p.artifact.clone(),
            version: receipt.version,
            current_version: state.current().number,
            sha256: sha,
            scene_json: c.scene_json,
        }))
    }
    pub(crate) fn scene_tool_accepted(&self, p: &ToolProposal) -> R<bool> {
        let (_, action) = parsed(p)?;
        if action.is_none() {
            return Ok(false);
        }
        self.scene_result(p).map(|result| result.is_some())
    }
    pub(crate) fn scene_scoped_head(
        &self,
        request: &types::ArtifactRequest,
        source: &str,
        source_sha: &str,
        space: &str,
    ) -> R<()> {
        let state = self.item_state(&request.item_id)?;
        if state.space != space {
            return Err(stale());
        }
        let base = state
            .versions
            .iter()
            .find(|v| v.number == request.version)
            .ok_or_else(stale)?;
        let ItemContent::App(app) = &base.content else {
            return Err(stale());
        };
        let pin = &app.resource_uri;
        if app.view.is_none()
            || !crate::app_resources::valid(app)
            || app
                .document
                .as_ref()
                .is_none_or(|d| d.sha256 != request.sha256)
        {
            return Err(stale());
        }
        let actor = self.me_id()?;
        let (_, device) = self.app_document_identity(space)?;
        let (_, receipts) = self.document_receipts(&actor, &device, &request.item_id)?;
        let mut cursor = (request.version, request.sha256.clone());
        for next in state.versions.iter().filter(|v| v.number > request.version) {
            let ItemContent::App(after) = &next.content else {
                return Err(stale());
            };
            let sha = &after.document.as_ref().ok_or_else(stale)?.sha256;
            if after.resource_uri != *pin || after.view != app.view || next.author != actor {
                return Err(stale());
            }
            let matched = receipts.iter().any(|receipt| {
                receipt.version == next.number
                    && receipt.sha256 == *sha
                    && receipt.source.as_ref().is_some_and(|s| {
                        s.source == source
                            && s.source_sha256 == source_sha
                            && s.request_id == request.request_id
                            && s.base_version == cursor.0
                            && s.base_sha256 == cursor.1
                            && s.resource_uri == *pin
                    })
                    && self.state.requests.values().any(|r| {
                        let Some(p) = r.req.tool_proposal.as_deref() else {
                            return false;
                        };
                        approved(r)
                            && r.space == space
                            && r.req.agent == actor
                            && p.schema == SCHEMA
                            && p.artifact == request.item_id
                            && p.source == source
                            && p.source_sha256 == source_sha
                            && p.version == cursor.0
                            && p.sha256 == cursor.1
                            && review::mutation(p).is_ok_and(|id| id == receipt.id)
                            && self.scene_result(p).is_ok_and(|result| {
                                result.is_some_and(|result| {
                                    result.version == next.number && result.sha256 == *sha
                                })
                            })
                    })
            });
            if !matched || next.number != cursor.0.checked_add(1).ok_or_else(invalid)? {
                return Err(stale());
            }
            cursor = (next.number, sha.clone());
        }
        Ok(())
    }
}
