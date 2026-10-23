//! Identity, privacy and Space membership contracts.

use crate::IdentityId;
use serde::{Deserialize, Serialize};

// ───────────────────────────── Identidade ─────────────────────────────

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IdentityKind {
    Person,
    Agent,
}

/// An owner's certified device authorizes one agent identity. The proof grants no
/// Space membership, spending budget or tool capability by itself.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AgentOwnerProof {
    pub device: String,
    pub cert: String,
    pub signature: String,
}

/// Uma Identidade: um par de chaves com nome e rosto.
/// Pessoa e agente são o mesmo objeto; o agente tem, a mais, um dono.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    pub id: IdentityId,
    pub kind: IdentityKind,
    pub name: String,
    pub handle: String,
    /// Cor da Identidade (agentes: gradiente do squircle; pessoas: fundo do avatar).
    pub tint_hex: String,
    /// Glifo monocromático (nome de SF Symbol no cliente Apple). Só agentes.
    pub glyph: Option<String>,
    /// Dono do agente. "Quem é dono do agente paga o agente."
    pub owner: Option<IdentityId>,
    /// Uma linha sobre o agente/pessoa.
    pub bio: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_proof: Option<Box<AgentOwnerProof>>,
}

// ───────────────────────────── Espaço & Membro ─────────────────────────────

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Privacy {
    /// 🔒 Ponta a ponta (MLS). Agentes são leitores declarados.
    EndToEnd,
    /// 🛡️ Fechada: legível pelo servidor para moderar, buscar e rodar agentes.
    Closed,
    /// 🌐 Pública.
    Public,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SpaceKind {
    /// Conversa a dois (pessoa ↔ pessoa ou pessoa ↔ agente).
    Direct,
    Group,
    Community,
    /// Seu contexto pessoal: só você. Guarda orçamentos dos seus agentes e Memória.
    /// Não aparece em Conversas.
    Personal,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Role {
    Owner,
    Admin,
    Member,
    /// Lê, mas não escreve (ex.: agente em modo Ouvir numa comunidade).
    Reader,
}
