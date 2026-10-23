//! Human review reads the exact signed proposal. It never grants or publishes.
use super::*;

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub(crate) struct SiteToolReview {
    pub request_id: String,
    pub content_hash: String,
    pub title: String,
    pub version: u32,
    pub before_html: Option<String>,
    pub after_html: String,
}
impl Engine {
    pub(crate) fn site_tool_review(
        &self,
        identity: &str,
        device: &str,
        request: &str,
        hash: &str,
    ) -> R<SiteToolReview> {
        self.native_surface_owner(identity, device)?;
        let r = self.state.requests.get(request).ok_or_else(stale)?;
        let p = r.req.tool_proposal.as_deref().ok_or_else(invalid)?;
        let (ask, _) = review::parsed(p)?;
        let space = ask["envelope"]["space"].as_str().ok_or_else(invalid)?;
        if p.owner != identity
            || r.req.content_hash != hash
            || self.me_id()? != identity
            || !self.state.writer(space, identity)
            || !self.tool_non_blocked(&r.req)
            || !self.state.tool_review_current(r)
            || !review::matches(&self.state, &r.req, space)
            || p.expires_at_ms <= crate::engine::now_ms()
        {
            return Err(stale());
        }
        let args = serde_json::from_str(&p.arguments_json).map_err(|_| invalid())?;
        let (title, before_html, after_html) = match parse(&p.canonical_tool, &args)? {
            Command::Create(c) => (c.title, None, c.html),
            Command::Read(c) => {
                let s = self.agent_site_snapshot(&c.artifact_id, c.version, &c.sha256)?;
                (s.name, None, s.html)
            }
            Command::Replace(c) => {
                let s = self.agent_site_snapshot(&c.artifact_id, c.version, &c.sha256)?;
                (s.name, Some(s.html), c.html)
            }
        };
        Ok(SiteToolReview {
            request_id: request.into(),
            content_hash: hash.into(),
            title,
            version: p.version,
            before_html,
            after_html,
        })
    }
}
