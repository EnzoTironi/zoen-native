//! One owned-reader admission for execution and the existing bridge result.
use super::*;
#[derive(Clone, Copy, serde::Serialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub(crate) enum AppEditConflictKind {
    MissingOwnedAgent,
    WaitingForMls,
    Changed,
    ChooseAgent,
    AgentTrustRequired,
}
impl AppEditConflictKind {
    pub(super) fn message(self) -> String {
        match self {
            Self::MissingOwnedAgent => t(
                "Adicione seu agente à conversa antes de pedir uma alteração.",
                "Add your agent to this conversation before requesting an edit.",
            ),
            Self::WaitingForMls => t(
                "Aguarde a entrada do seu agente na conversa.",
                "Wait for your agent to join this conversation.",
            ),
            Self::ChooseAgent => t(
                "Escolha o agente para este pedido.",
                "Choose the agent for this request.",
            ),
            Self::AgentTrustRequired => t(
                "Permita que seu agente sugira alterações nesta conversa.",
                "Allow your agent to suggest edits in this conversation.",
            ),
            Self::Changed => t(
                "O App ou o pedido mudou; confira a versão atual.",
                "The App or request changed; review the current version.",
            ),
        }
    }
}
impl Engine {
    pub(super) fn ready_owned_app_agent(
        &self,
        space: &str,
        owner: &str,
    ) -> Result<String, AppEditConflictKind> {
        let room = self
            .state
            .spaces
            .get(space)
            .ok_or(AppEditConflictKind::Changed)?;
        let agents: Vec<_> = room
            .members
            .iter()
            .filter_map(|(id, _)| {
                (self.state.writer(space, id) && self.verified_owned_agent(id, owner)).then_some(id)
            })
            .collect();
        if agents.is_empty() {
            return Err(AppEditConflictKind::MissingOwnedAgent);
        }
        let eligible = self.eligible_app_agents(space, owner);
        match eligible.as_slice() {
            [agent] => Ok(agent.clone()),
            [] if agents
                .iter()
                .any(|id| self.member_delivery_ready(space, id)) =>
            {
                Err(AppEditConflictKind::AgentTrustRequired)
            }
            [] => Err(AppEditConflictKind::WaitingForMls),
            _ => Err(AppEditConflictKind::ChooseAgent),
        }
    }
    /// Classification is consumed only for an actual definitive Stale result.
    /// It grants nothing and never substitutes for the operation's checks.
    pub(crate) fn app_edit_conflict_kind(
        &self,
        input: &AppEditRequestInput,
    ) -> AppEditConflictKind {
        let current = self.item_state(&input.item_id).ok().is_some_and(|item| {
            item.space == input.space_id
                && item.current().number == input.expected_version
                && matches!(&item.current().content, ItemContent::App(app)
                if app.document.as_ref().is_some_and(|file| file.sha256 == input.expected_sha256))
        });
        if !current {
            return AppEditConflictKind::Changed;
        }
        self.ready_owned_app_agent(&input.space_id, &input.expected_identity_id)
            .err()
            .unwrap_or(AppEditConflictKind::Changed)
    }
}
