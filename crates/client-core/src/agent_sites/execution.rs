//! A small effect receipt commits with the FileDoc change. It stores no HTML.
use super::*;

pub struct ApprovedSiteTool {
    request: String,
    observed: String,
    actor: String,
    proposal: ToolProposal,
}
impl ApprovedSiteTool {
    pub fn proposal(&self) -> &ToolProposal {
        &self.proposal
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(super) struct Effect {
    item_id: String,
    version: u32,
    sha256: String,
    author_id: String,
    intent: String,
}

impl Engine {
    pub(crate) fn site_proposal_authority(
        &self,
        p: &ToolProposal,
    ) -> R<(String, String, String, String)> {
        let (ask, action) = review::parsed(p)?;
        let space = ask["envelope"]["space"].as_str().ok_or_else(invalid)?;
        self.agent_site_authority(space, &p.source, &p.serving, action.is_some())
    }
    pub(super) fn checked_site_proposal(
        &self,
        request: &str,
        observed: &str,
        actor: &str,
    ) -> R<ToolProposal> {
        let (r, p) = self.tool_resolution(request, observed)?;
        if r.req.agent != actor || self.me_id()? != actor {
            return Err(stale());
        }
        let (device, owner, space, source) = self.site_proposal_authority(p)?;
        if device != p.device || owner != p.owner || space != r.space || source != p.source_sha256 {
            return Err(stale());
        }
        Ok(p.clone())
    }
    pub(super) fn site_effect(&self, p: &ToolProposal) -> R<Option<Effect>> {
        let intent = review::intent(p)?;
        let actor = self.me_id()?;
        let Some(raw) = self.store.meta(&format!("site_tool:{actor}:{intent}"))? else {
            return Ok(None);
        };
        if raw.len() > 2048 {
            return Err(invalid());
        }
        let effect: Effect = serde_json::from_str(&raw).map_err(|_| invalid())?;
        if effect.intent != intent || effect.author_id != actor {
            return Err(stale());
        }
        let source = self.agent_site_snapshot(&effect.item_id, effect.version, &effect.sha256)?;
        let it = self.item_state(&effect.item_id)?;
        let v = it
            .versions
            .iter()
            .find(|v| v.number == effect.version)
            .ok_or_else(stale)?;
        let (ask, _) = review::parsed(p)?;
        let args = serde_json::from_str(&p.arguments_json).map_err(|_| invalid())?;
        let (expected_version, expected_sha) = match parse(&p.canonical_tool, &args)? {
            Command::Create(c) if effect.item_id == format!("it_site_{intent}") => {
                (1, sha(c.html.as_bytes()))
            }
            Command::Replace(c) if effect.item_id == c.artifact_id => (
                c.version.checked_add(1).ok_or_else(invalid)?,
                sha(c.html.as_bytes()),
            ),
            _ => return Err(stale()),
        };
        if source.space_id != ask["envelope"]["space"].as_str().ok_or_else(invalid)?
            || v.author != actor
            || effect.version != expected_version
            || effect.sha256 != expected_sha
        {
            return Err(stale());
        }
        Ok(Some(effect))
    }
    pub(crate) fn site_tool_accepted(&self, p: &ToolProposal) -> R<bool> {
        self.site_effect(p).map(|v| v.is_some())
    }
    pub(super) fn site_tool_result(&self, effect: &Effect, replayed: bool) -> R<String> {
        Ok(json!({"itemId":effect.item_id,"version":effect.version,"sha256":effect.sha256,"authorId":effect.author_id,"engineRevision":REVISION,"replayed":replayed}).to_string())
    }
}

impl Client {
    pub fn approved_site_tool(&self, request: &str) -> R<ApprovedSiteTool> {
        let e = self.lock();
        let r = e.state.requests.get(request).ok_or_else(stale)?;
        let actor = e.me_id()?;
        let proposal = e.checked_site_proposal(request, &r.req.content_hash, &actor)?;
        Ok(ApprovedSiteTool {
            request: request.into(),
            observed: r.req.content_hash.clone(),
            actor,
            proposal,
        })
    }
    pub fn execute_site_tool(&self, permit: &ApprovedSiteTool) -> R<String> {
        let mut e = self.lock();
        let p = e.checked_site_proposal(&permit.request, &permit.observed, &permit.actor)?;
        if p != permit.proposal {
            return Err(stale());
        }
        let args = serde_json::from_str(&p.arguments_json).map_err(|_| invalid())?;
        let command = parse(&p.canonical_tool, &args)?;
        if command.is_write() {
            if let Some(effect) = e.site_effect(&p)? {
                return e.site_tool_result(&effect, true);
            }
        }
        let (r, _) = e.tool_resolution(&permit.request, &permit.observed)?;
        if p.expires_at_ms <= crate::engine::now_ms()
            || e.approval_replay_needed
            || !e.state.tool_review_current(r)
            || !review::matches(&e.state, &r.req, &r.space)
        {
            return Err(stale());
        }
        if let Command::Read(c) = command {
            return serde_json::to_string(&e.agent_site_snapshot(
                &c.artifact_id,
                c.version,
                &c.sha256,
            )?)
            .map_err(|_| invalid());
        }
        let intent = review::intent(&p)?;
        let (ask, _) = review::parsed(&p)?;
        let space = ask["envelope"]["space"]
            .as_str()
            .ok_or_else(invalid)?
            .to_owned();
        e.atomic_agent_action(|e| {
            let (item_id, version) = match command {
                Command::Create(c) => {
                    let name = format!("{}.html", c.title);
                    let item_id = format!("it_site_{intent}");
                    let mutation_id = format!("site_{intent}");
                    let entry = e.send_file_message_inner(crate::files::FileSendInput {
                        space: &space,
                        name: &name,
                        mime: "text/html",
                        bytes: c.html.as_bytes(),
                        thumbnail: None,
                        reply_to: Some(&p.source),
                        thread: false,
                        mutation_id: &mutation_id,
                        caption: "",
                        media: None,
                        fixed_item: Some(&item_id),
                    })?;
                    let crate::EntryKind::Message {
                        card: Some(card), ..
                    } = entry.kind
                    else {
                        return Err(invalid());
                    };
                    (card.item_id, card.version)
                }
                Command::Replace(c) => {
                    let before = e.agent_site_snapshot(&c.artifact_id, c.version, &c.sha256)?;
                    if before.html == c.html {
                        return Err(stale());
                    }
                    e.file_new_version_checked(
                        &c.artifact_id,
                        c.version,
                        &c.sha256,
                        c.html.as_bytes(),
                        None,
                        "",
                    )?;
                    let version = e.item_state(&c.artifact_id)?.current().number;
                    (c.artifact_id, version)
                }
                Command::Read(_) => return Err(invalid()),
            };
            let info = e.file_info_at(&item_id, Some(version))?;
            let effect = Effect {
                item_id,
                version,
                sha256: info.sha256,
                author_id: permit.actor.clone(),
                intent: intent.clone(),
            };
            let raw = serde_json::to_string(&effect).map_err(|_| invalid())?;
            if raw.len() > 2048 {
                return Err(invalid());
            }
            e.store
                .set_meta(&format!("site_tool:{}:{intent}", permit.actor), &raw)?;
            e.site_effect(&p)?.ok_or_else(stale)?;
            e.site_tool_result(&effect, false)
        })
    }
}
