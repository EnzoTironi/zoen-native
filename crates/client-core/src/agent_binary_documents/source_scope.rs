//! The optional signed reply narrows an existing source; it never grants access.
use super::*;

/// Pure signed-log shape used by projection and remote owner review too. Local
/// receipt ancestry is additionally checked by the executing native authority.
pub(crate) fn request_valid(
    state: &crate::engine::State,
    space: &str,
    text: &str,
    reference: &types::ReplyRef,
) -> bool {
    let Some(request) = &reference.artifact else {
        return false;
    };
    if !request.is_well_formed()
        || !reference.thread
        || text.trim().is_empty()
        || hex::encode(Sha256::digest(text.as_bytes())) != request.text_sha256
    {
        return false;
    }
    let Some(item) = state
        .items
        .get(&request.item_id)
        .filter(|i| i.space == space)
    else {
        return false;
    };
    if state.app_discussion_anchor(space, &request.item_id) != Some(reference.to.as_str()) {
        return false;
    }
    item.versions
        .iter()
        .find(|v| v.number == request.version)
        .is_some_and(|v| {
            matches!(&v.content, ItemContent::App(app)
            if miniapps::binary::Format::for_id(&app.app).is_some()
                && app.document.as_ref().is_some_and(|f| f.sha256 == request.sha256))
        })
}

pub(crate) fn proposal_valid(
    state: &crate::engine::State,
    space: &str,
    proposal: &types::ToolProposal,
) -> bool {
    let Some(entry) = state
        .spaces
        .get(space)
        .and_then(|s| s.entries.iter().find(|e| e.id() == proposal.source))
    else {
        return false;
    };
    let EntryBody::Message {
        text,
        reply: Some(reference),
        ..
    } = &entry.body
    else {
        return true;
    };
    let Some(request) = &reference.artifact else {
        return true;
    };
    request_valid(state, space, text, reference)
        && proposal.schema == 1
        && proposal.artifact == request.item_id
        && proposal.version >= request.version
        && (proposal.version != request.version || proposal.sha256 == request.sha256)
}

impl Engine {
    pub(crate) fn artifact_request(
        &self,
        space: &str,
        source: &str,
    ) -> R<Option<types::ArtifactRequest>> {
        let entries = &self.space_state(space)?.entries;
        let entry = entries
            .iter()
            .find(|e| e.id() == source)
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
        let Some(reference) = reply else {
            return Ok(None);
        };
        let Some(request) = &reference.artifact else {
            return Ok(None);
        };
        if !request.is_well_formed()
            || !reference.thread
            || attaches.is_some()
            || marks.deleted
            || marks.forwarded
            || hex::encode(Sha256::digest(text.as_bytes())) != request.text_sha256
        {
            return Err(denied());
        }
        let discussion = self.item_discussion(space, &request.item_id)?;
        if discussion
            .root
            .as_ref()
            .is_none_or(|root| root.id != reference.to)
            || self.state.app_discussion_anchor(space, &request.item_id)
                != Some(reference.to.as_str())
        {
            return Err(denied());
        }
        let item = self.item_state(&request.item_id)?;
        let base = item
            .versions
            .iter()
            .find(|v| v.number == request.version)
            .ok_or_else(denied)?;
        let ItemContent::App(app) = &base.content else {
            return Err(denied());
        };
        if miniapps::binary::Format::for_id(&app.app).is_none()
            || app
                .document
                .as_ref()
                .is_none_or(|f| f.sha256 != request.sha256)
        {
            return Err(denied());
        }
        Ok(Some(request.clone()))
    }
    pub(super) fn check_artifact_head(
        &self,
        item: &str,
        source: &str,
        source_sha: &str,
        space: &str,
    ) -> R<()> {
        if let Some(request) = self.artifact_request(space, source)? {
            if request.item_id != item {
                return Err(denied());
            }
            self.app_binary_scoped_head(&request, source, source_sha)?;
        }
        Ok(())
    }
    pub(super) fn check_artifact_version(&self, turn: &AgentBinaryTurn) -> R<()> {
        if let Some(request) = self.artifact_request(&turn.space, &turn.source)? {
            if turn.document.version < request.version {
                return Err(denied());
            }
        }
        Ok(())
    }
    pub(super) fn artifact_receipt(
        &self,
        turn: &AgentBinaryTurn,
    ) -> R<Option<crate::app_binary_documents::SourceBinding>> {
        Ok(self
            .artifact_request(&turn.space, &turn.source)?
            .map(|request| crate::app_binary_documents::SourceBinding {
                source: turn.source.clone(),
                source_sha256: turn.source_sha256.clone(),
                request_id: request.request_id,
                base_version: turn.document.version,
                base_sha256: turn.document.sha256.clone(),
            }))
    }
}
impl Client {
    /// Publish the scoped answer under the same lock as its source/head checks.
    /// Keep the ordinary inline source link used by host recovery and the sheet.
    pub fn agent_artifact_reply(
        &self,
        space: &str,
        source: &str,
        serving: &str,
        text: &str,
    ) -> R<crate::TimelineEntry> {
        let mut e = self.lock();
        let (_, _, _, source_sha) = e.agent_source_binding(space, source, serving, false)?;
        let request = e.artifact_request(space, source)?.ok_or_else(denied)?;
        e.check_artifact_head(&request.item_id, source, &source_sha, space)?;
        let me = e.me_id()?;
        if matches!(
            e.decide(&me, space, &types::ActionClass::Reply, 0),
            grants::Decision::Block(_)
        ) {
            return Err(denied());
        }
        if let Some(answer) = e.space_state(space)?.entries.iter().find(|entry| {
            entry.author == me
                && matches!(&entry.body, EntryBody::Message {reply: Some(reference), ..}
                if !reference.thread && reference.to == source)
        }) {
            return Ok(e.entry_dto(answer));
        }
        e.send_reply(space, text, source, false)
    }

    /// Rust-only discovery hint. Errors must never fall back to unscoped tools;
    /// execution independently rechecks this signed source and the receipt chain.
    pub fn agent_artifact_request(
        &self,
        space: &str,
        source: &str,
        serving: &str,
    ) -> R<Option<types::ArtifactRequest>> {
        let e = self.lock();
        let (_, _, _, source_sha) = e.agent_source_binding(space, source, serving, false)?;
        let request = e.artifact_request(space, source)?;
        if let Some(request) = &request {
            e.check_artifact_head(&request.item_id, source, &source_sha, space)?;
        }
        Ok(request)
    }
}
