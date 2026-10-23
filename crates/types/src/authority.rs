//! Grants and signed requests, proposals and resolutions.

use crate::{GrantId, IdentityId, ItemContent, ItemId, ItemKind, RequestId, Role, SpaceId};
use serde::{Deserialize, Serialize};

// ───────────────────────────── Concessão ─────────────────────────────

/// Nível de confiança de um agente num Espaço. Um controle só.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TrustLevel {
    /// Lê só quando chamado e responde com texto.
    Listen,
    /// Prepara rascunhos e propostas; nada sai sem você.
    Suggest,
    /// Faz o que é reversível dentro do Espaço e do orçamento, sempre com desfazer.
    Act,
    /// Também age fora (agenda, e-mail, pagamentos pequenos) dentro dos limites.
    Autonomous,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum GrantScope {
    Space(SpaceId),
    Item(ItemId),
    Everywhere,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum Capability {
    Trust(TrustLevel),
    /// Orçamento mensal de IA, em centavos de real.
    MonthlyBudget {
        cents: i64,
    },
    /// Convite/link de compartilhamento: quem tiver o link entra com este papel.
    Invite {
        role: Role,
    },
    /// A native device capability for a mini-app (`photos.pick`, `location`,
    /// `calendar.freebusy`, `net:tiles.openfreemap.org`…), with the purpose it showed you.
    /// "Allow once" is the same grant with a short expiry, so it's logged too.
    Device {
        capability: String,
        purpose: String,
    },
    /// How much Zoen asks before a mini-app or agent uses something (Ask / Auto / Trusted).
    /// Issued to a subject: `consent:global`, `space:<id>`, an agent id or `app:<item>`.
    /// The newest unrevoked one for a subject wins.
    ConsentMode {
        mode: ConsentMode,
    },
    /// A request the auto reviewer approved on its own (with the policy rule that let it).
    /// Short-lived like "Allow once"; revoking it is the toast's Undo.
    AutoApproved {
        capability: String,
        purpose: String,
        rule: String,
    },
    /// A standing decision on an agent's requests: "always approve" / "always deny" this
    /// kind of action (`grants::standing_key`) in the grant's scope. Issued by the
    /// agent's owner from the approvals stack; revoking it (GrantRevoked) makes the agent
    /// ask again. Red lines can't be always-approved (`standing_allow_permitted`).
    Standing {
        action: String,
        allow: bool,
    },
}

/// Consent mode. `Auto` is the default; `Trusted` is only for agents or mini-apps you mark.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConsentMode {
    /// Always show Zoen's sheet.
    Ask,
    /// The reviewer approves reads and reversible in-Space writes that match what you just did.
    Auto,
    /// Approves everything except the always-ask list.
    Trusted,
}

/// Uma Concessão: "quem pode fazer o quê em qual Espaço/Item, com quais limites".
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Grant {
    pub id: GrantId,
    pub grantor: IdentityId,
    /// `None` = portador (links de convite).
    pub grantee: Option<IdentityId>,
    pub scope: GrantScope,
    pub capability: Capability,
    pub expires_at_ms: Option<i64>,
}

/// Classe de uma ação proposta por um agente. É isso que o avaliador olha.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ActionClass {
    /// Responder com texto.
    Reply,
    /// Criar/editar/mover dentro do Espaço. Tudo é versionado → desfazível.
    Reversible,
    /// Mandar algo para fora (e-mail, WhatsApp, agenda).
    External,
    /// Apagar de verdade.
    Irreversible,
    /// Gastar dinheiro (não o orçamento de IA: dinheiro de verdade).
    Money { cents: i64 },
    /// Publicar para uma audiência nova/maior.
    PublicAudience,
    /// Ler ou usar dados de terceiros.
    ThirdPartyData,
}

/// Um pedido de agente (o "cartão de pedido"): o que muda, para quem e quanto custa.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AgentRequest {
    pub id: RequestId,
    pub agent: IdentityId,
    pub title: String,
    pub detail: String,
    pub audience: String,
    pub action: ActionClass,
    /// Hash do conteúdo sendo aprovado. Mudou o conteúdo, a aprovação não vale mais.
    pub content_hash: String,
    /// Opcional: a linha de plano que este pedido executa.
    pub item: Option<ItemId>,
    pub line: Option<String>,
    /// Exact reversible output, retained while the owner decides and across restarts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposal: Option<Box<ItemProposal>>,
    /// Frozen, versioned tool intent. Never authorizes generic tools or standing grants.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_proposal: Option<Box<ToolProposal>>,
}

/// Inert continuation bound to one existing signed request. Only native core can
/// turn an exact owner resolution into present execution authority.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ToolProposal {
    pub schema: u8,
    pub source: String,
    pub serving: IdentityId,
    pub source_sha256: String,
    pub device: String,
    pub owner: IdentityId,
    pub task_sha256: String,
    pub step: u32,
    pub artifact: ItemId,
    pub version: u32,
    pub sha256: String,
    pub canonical_tool: String,
    pub arguments_json: String,
    pub worker_binding: String,
    pub engine_revision: String,
    pub continuation_json: String,
    pub expires_at_ms: i64,
    pub gate_rule: String,
    /// An explicitly requested successor review, not new execution authority.
    /// Absent on legacy schema1/schema2 proposals, preserving their exact bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review: Option<ToolReviewLineage>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ToolReviewLineage {
    pub root: RequestId,
    pub predecessor: RequestId,
    pub intent: String,
    pub generation: u32,
}
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolDecision {
    ApproveOnce,
    Deny,
    Cancel,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ItemProposal {
    pub item: ItemId,
    pub kind: ItemKind,
    pub content: ItemContent,
    pub origin: String,
    pub message: String,
    /// Model usage incurred preparing this proposal, charged when it is opened.
    pub ai_cost_cents: i64,
}

impl AgentRequest {
    pub fn tool_proposal_hash(&self, space: &str) -> Option<String> {
        use sha2::{Digest, Sha256};
        let proposal = self.tool_proposal.as_ref()?;
        let bytes = serde_json::to_vec(&(
            "zoen.tool-proposal.v1",
            space,
            &self.id,
            &self.agent,
            &self.title,
            &self.detail,
            &self.audience,
            &self.action,
            proposal,
        ))
        .ok()?;
        Some(hex::encode(Sha256::digest(bytes)))
    }
    pub fn proposal_hash(&self, space: &str) -> Option<String> {
        use sha2::{Digest, Sha256};
        let proposal = self.proposal.as_ref()?;
        let bytes = serde_json::to_vec(&(
            "zoen.item-proposal.v1",
            space,
            &self.id,
            &self.agent,
            &self.title,
            &self.detail,
            &self.audience,
            &self.action,
            proposal,
        ))
        .expect("an item proposal serializes");
        Some(hex::encode(Sha256::digest(bytes)))
    }
}

/// The owner's signed receipt. Its output is derived from the original proposal;
/// resolution cannot substitute different content or charge model usage again.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RequestResolution {
    pub message: String,
    pub created_item: Option<ItemId>,
    pub ai_cost_cents: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub standing_grant: Option<Grant>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_decision: Option<ToolDecision>,
}
