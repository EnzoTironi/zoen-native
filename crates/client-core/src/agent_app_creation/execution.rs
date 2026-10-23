//! The native create function owns document encryption, grant and durable receipt.
use super::*;
use base64::{engine::general_purpose::STANDARD, Engine as _};

/// Native-only, nonserializable authority. No UI/model payload can mint this.
pub struct ApprovedAppCreation {
    request: String,
    observed: String,
    actor: String,
    proposal: ToolProposal,
}
impl ApprovedAppCreation {
    pub fn proposal(&self) -> &ToolProposal {
        &self.proposal
    }
}
pub(super) fn receipt(document: &AppBinaryDocumentDto, replayed: bool) -> String {
    json!({"itemId":document.item_id,"version":document.version,"currentVersion":document.current_version,"sha256":document.sha256,"authorId":document.author_id,"engineRevision":document.engine_revision,"replayed":replayed}).to_string()
}
impl Engine {
    fn check_creation(&self, request: &str, observed: &str, actor: &str) -> R<ToolProposal> {
        let (r, p) = self.tool_resolution(request, observed)?;
        if r.req.agent != actor || self.me_id()? != actor {
            return Err(stale());
        }
        parsed(p)?;
        let (ask, _) = parsed(p)?;
        let (device, owner, _, source) =
            self.creation_authority(ask_space(&ask)?, &p.source, &p.serving)?;
        if device != p.device || owner != p.owner || source != p.source_sha256 {
            return Err(stale());
        }
        Ok(p.clone())
    }
}
impl Client {
    pub fn approved_app_creation(&self, request: &str) -> R<ApprovedAppCreation> {
        let e = self.lock();
        let r = e.state.requests.get(request).ok_or_else(stale)?;
        let actor = e.me_id()?;
        let proposal = e.check_creation(request, &r.req.content_hash, &actor)?;
        Ok(ApprovedAppCreation {
            request: request.into(),
            observed: r.req.content_hash.clone(),
            actor,
            proposal,
        })
    }
    pub fn execute_app_creation(&self, permit: &ApprovedAppCreation) -> R<String> {
        let mut e = self.lock();
        let p = e.check_creation(&permit.request, &permit.observed, &permit.actor)?;
        if p != permit.proposal {
            return Err(stale());
        }
        // A completed receipt is recovered before checking freshness/currentness.
        // It cannot grant another effect, even after expiry or a later edit.
        if let Some(document) = e.creation_result(&p)? {
            return Ok(receipt(&document, true));
        }
        let (r, _) = e.tool_resolution(&permit.request, &permit.observed)?;
        if p.expires_at_ms <= crate::engine::now_ms()
            || e.approval_replay_needed
            || !e.state.tool_review_current(r)
            || !matches(&e.state, &r.req, &r.space)
        {
            return Err(stale());
        }
        let (ask, args) = parsed(&p)?;
        let space = ask_space(&ask)?;
        let document = e.app_binary_create(
            space,
            &title(&args.ok_or_else(invalid)?)?,
            &STANDARD.encode(registration::EMPTY),
            &mutation(&p, space, &permit.actor)?,
        )?;
        if document.item_id != p.artifact || document.version != 1 || document.sha256 != p.sha256 {
            return Err(stale());
        }
        Ok(receipt(&document, false))
    }
}
