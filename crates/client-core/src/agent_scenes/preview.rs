//! Read-only review of exact signed scene bytes. Rendering never grants an effect.
use super::{access, invalid, matches, parsed, stale, tools, Deserialize, Engine, Serialize, R};

#[derive(Clone, Debug, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub(crate) struct SceneToolReviewInput {
    pub expected_identity_id: String,
    pub expected_device_id: String,
    pub space_id: String,
    pub item_id: String,
    pub expected_resource_uri: String,
    pub request_id: String,
    pub expected_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub(crate) struct SceneToolReview {
    pub request_id: String,
    pub content_hash: String,
    pub artifact_id: String,
    pub space_id: String,
    pub resource_uri: String,
    pub title: String,
    pub version: u32,
    pub before_sha256: String,
    pub before_scene_json: String,
    pub after_sha256: String,
    pub after_scene_json: String,
}

impl Engine {
    pub(crate) fn scene_tool_review(&self, input: &SceneToolReviewInput) -> R<SceneToolReview> {
        self.native_surface_owner(&input.expected_identity_id, &input.expected_device_id)?;
        self.check_app_resource_pin(&input.item_id, &input.expected_resource_uri)?;
        let r = self
            .state
            .requests
            .get(&input.request_id)
            .ok_or_else(stale)?;
        let p = r.req.tool_proposal.as_deref().ok_or_else(invalid)?;
        let (_, action) = parsed(p)?;
        if p.owner != input.expected_identity_id
            || r.space != input.space_id
            || p.artifact != input.item_id
            || r.req.content_hash != input.expected_hash
            || r.status != crate::engine::ReqStatus::Pending
            || p.expires_at_ms <= crate::engine::now_ms()
            || !self.state.tool_review_current(r)
            || !self.tool_non_blocked(&r.req)
            || !access(&self.state, p, crate::engine::now_ms())
            || !matches(&self.state, &r.req, &r.space)
        {
            return Err(stale());
        }
        let args = action.ok_or_else(invalid)?;
        let tools::Command::Replace(after) = tools::parse(&p.canonical_tool, &args)? else {
            return Err(invalid());
        };
        let before = self.scene_snapshot_at(&input.item_id, p.version)?;
        if before.resource_uri != input.expected_resource_uri
            || after.resource_uri != before.resource_uri
            || after.sha256 != before.sha256
            || before.space_id != input.space_id
        {
            return Err(stale());
        }
        Ok(SceneToolReview {
            request_id: r.req.id.clone(),
            content_hash: r.req.content_hash.clone(),
            artifact_id: before.artifact_id,
            space_id: before.space_id,
            resource_uri: before.resource_uri,
            title: before.title,
            version: before.version,
            before_sha256: before.sha256,
            before_scene_json: before.scene_json,
            after_sha256: tools::digest(after.scene_json.as_bytes()),
            after_scene_json: after.scene_json,
        })
    }
}
