//! Disposable selection over the same signed membership, trust and App head.
use super::*;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub(crate) struct AppEditTargetInput {
    pub expected_identity_id: String,
    pub expected_device_id: String,
    pub space_id: String,
    pub item_id: String,
    pub expected_version: u32,
    pub expected_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "client-types", ts(optional))]
    pub expected_resource_uri: Option<String>,
}
#[derive(serde::Serialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub(crate) struct AppEditTarget {
    pub agent: crate::Persona,
    pub delivery_ready: bool,
    pub permission_required: bool,
    pub eligible: bool,
}
#[derive(serde::Serialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub(crate) struct AppEditTargets {
    pub agents: Vec<AppEditTarget>,
    pub revision: String,
}
impl From<&AppEditRequestInput> for AppEditTargetInput {
    fn from(i: &AppEditRequestInput) -> Self {
        Self {
            expected_identity_id: i.expected_identity_id.clone(),
            expected_device_id: i.expected_device_id.clone(),
            space_id: i.space_id.clone(),
            item_id: i.item_id.clone(),
            expected_version: i.expected_version,
            expected_sha256: i.expected_sha256.clone(),
            expected_resource_uri: i.expected_resource_uri.clone(),
        }
    }
}
impl Engine {
    pub(crate) fn app_edit_targets(&self, i: &AppEditTargetInput) -> R<AppEditTargets> {
        self.native_surface_owner(&i.expected_identity_id, &i.expected_device_id)?;
        if !self.state.writer(&i.space_id, &i.expected_identity_id)
            || self.checked_integrity_error(&i.space_id)?.is_some()
        {
            return Err(stale());
        }
        let item = self.item_state(&i.item_id)?;
        let current = item.current();
        let ItemContent::App(app) = &current.content else {
            return Err(invalid());
        };
        let owned = app.view.is_some() && crate::app_resources::valid(app);
        if item.space != i.space_id
            || current.number != i.expected_version
            || app
                .document
                .as_ref()
                .is_none_or(|f| f.sha256 != i.expected_sha256)
            || (!owned
                && !miniapps::binary::Format::for_id(&app.app)
                    .is_some_and(|f| app.resource_uri == f.resource()))
            || i.expected_resource_uri
                .as_ref()
                .is_some_and(|uri| uri != &app.resource_uri)
        {
            return Err(stale());
        }
        let agents = self.owned_app_agents(&i.space_id, &i.expected_identity_id);
        let choices: Vec<_> = agents
            .iter()
            .map(|id| {
                let delivery_ready = self.member_delivery_ready(&i.space_id, id);
                let permission_required =
                    !self.explicit_app_agent_trust(&i.space_id, &i.expected_identity_id, id);
                AppEditTarget {
                    agent: self.persona(id),
                    delivery_ready,
                    permission_required,
                    eligible: delivery_ready && !permission_required,
                }
            })
            .collect();
        // No wall clock is hashed: unchanged reads are stable, while crossing an
        // expiry changes the eligible set. Replacing a grant changes its identity.
        let grants: Vec<_> = agents
            .iter()
            .map(|a| {
                self.state
                    .latest_trust(&i.space_id, &i.expected_identity_id, a)
                    .map(|g| (&g.grant, g.at_ms))
            })
            .collect();
        let members = self.space_state(&i.space_id)?.members.as_slice();
        let profiles: Vec<_> = agents.iter().map(|a| self.identities.get(a)).collect();
        // Pending role ABA is visible even before its relay-confirmed cut.
        let pending = self.store.outbox()?;
        let authority_cut: Vec<_> = pending
            .iter()
            .filter(|row| row.event.space == i.space_id && authority_event(&row.event.body))
            .map(|row| (&row.event.client_id, row.failed))
            .collect();
        let revision = event_log::content_hash(&(
            "zoen.app-edit-targets.v1",
            i,
            &app.resource_uri,
            &choices,
            profiles,
            grants,
            members,
            authority_cut,
            self.logs.get(&i.space_id).and_then(|log| {
                log.events()
                    .iter()
                    .rev()
                    .find(|e| authority_event(&e.body))
                    .map(|e| (&e.client_id, &e.hash))
            }),
        ))?;
        Ok(AppEditTargets {
            agents: choices,
            revision,
        })
    }
    fn explicit_app_agent_trust(&self, space: &str, owner: &str, id: &str) -> bool {
        let at = crate::engine::now_ms();
        self.state.latest_trust(space, owner, id).is_some_and(|g| {
            g.grant.expires_at_ms.is_none_or(|expiry| expiry > at)
                && matches!(g.grant.capability, types::Capability::Trust(level)
                if level >= types::TrustLevel::Suggest)
        })
    }
    fn owned_app_agents(&self, space: &str, owner: &str) -> Vec<String> {
        let mut agents: Vec<_> = self
            .state
            .spaces
            .get(space)
            .into_iter()
            .flat_map(|s| s.members.iter())
            .filter(|(id, _)| self.state.writer(space, id) && self.verified_owned_agent(id, owner))
            .map(|(id, _)| id.clone())
            .collect();
        agents.sort();
        agents
    }
    pub(super) fn eligible_app_agents(&self, space: &str, owner: &str) -> Vec<String> {
        self.owned_app_agents(space, owner)
            .into_iter()
            .filter(|id| {
                (!self.is_e2e(space) || self.member_delivery_ready(space, id))
                    && self.trust_available(id, space)
                    && self.trust(id, space) >= types::TrustLevel::Suggest
            })
            .collect()
    }
    pub(super) fn select_app_agent(&self, i: &AppEditRequestInput) -> R<String> {
        let context = self.app_edit_targets(&i.into())?;
        match (&i.target_agent_id, &i.expected_target_revision) {
            (Some(target), Some(revision)) => {
                let item = self.item_state(&i.item_id)?;
                if revision != &context.revision
                    || !context
                        .agents
                        .iter()
                        .any(|a| &a.agent.id == target && a.eligible)
                    || matches!(&item.current().content, ItemContent::App(app)
                        if app.view.is_some() && i.expected_resource_uri.is_none())
                {
                    return Err(stale());
                }
                Ok(target.clone())
            }
            (None, None) => match self
                .eligible_app_agents(&i.space_id, &i.expected_identity_id)
                .as_slice()
            {
                [agent] => Ok(agent.clone()),
                [] => Err(CoreError::Stale {
                    reason: self
                        .ready_owned_app_agent(&i.space_id, &i.expected_identity_id)
                        .err()
                        .unwrap_or(AppEditConflictKind::Changed)
                        .message(),
                }),
                _ => Err(CoreError::Stale {
                    reason: AppEditConflictKind::ChooseAgent.message(),
                }),
            },
            _ => Err(invalid()),
        }
    }
}

fn authority_event(body: &EventBody) -> bool {
    matches!(
        body,
        EventBody::SpaceCreated { .. }
            | EventBody::MemberAdded { .. }
            | EventBody::MemberRemoved { .. }
            | EventBody::GrantIssued { .. }
            | EventBody::GrantRevoked { .. }
    )
}
