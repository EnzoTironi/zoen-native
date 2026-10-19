//! Identity, device, definition and avatar for one agent citizen.

use event_log::{agent_owner, Author, Signer};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use types::{Identity, IdentityKind};
use zeroize::Zeroizing;

/// Default avatar style until content-addressed hand-drawn art lands.
pub const AVATAR_STYLE: &str = "hand-drawn-animated";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    /// Prompt-first agent that creates other user agents from conversation.
    Creator,
    /// An ordinary user agent (booking bot, shop assistant, …).
    User,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelTier {
    /// Cheap default (GLM-5.3-Flash class). Always the starting tier.
    #[default]
    Cheap,
    Standard,
    Premium,
}

/// Hand-drawn animated face. Glyph + tint stand in until `asset` is filled.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Avatar {
    pub style: String,
    pub tint_hex: String,
    pub glyph: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
}

impl Avatar {
    pub fn hand_drawn(tint_hex: impl Into<String>, glyph: impl Into<String>) -> Self {
        Self {
            style: AVATAR_STYLE.into(),
            tint_hex: tint_hex.into(),
            glyph: glyph.into(),
            asset: None,
        }
    }
}

/// Versioned behavior. Persona and rules are prompts; `state` is free JSON.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Definition {
    pub version: u32,
    pub kind: AgentKind,
    pub persona: String,
    pub rules: Vec<String>,
    #[serde(default)]
    pub state: Value,
    #[serde(default)]
    pub model_tier: ModelTier,
}

impl Definition {
    pub fn encode(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}

/// Live citizen: profile + certified device + definition + avatar.
pub struct Citizen {
    pub profile: Identity,
    pub author: Author,
    pub definition: Definition,
    pub avatar: Avatar,
    root_secret: Zeroizing<[u8; 32]>,
    device_secret: Zeroizing<[u8; 32]>,
}

impl std::fmt::Debug for Citizen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Citizen")
            .field("id", &self.profile.id)
            .field("kind", &self.definition.kind)
            .field("handle", &self.profile.handle)
            .finish_non_exhaustive()
    }
}

impl Citizen {
    pub fn id(&self) -> &str {
        &self.profile.id
    }

    pub fn device_id(&self) -> &str {
        self.author.device.as_deref().unwrap_or("")
    }

    pub fn root_secret(&self) -> [u8; 32] {
        *self.root_secret
    }

    pub fn device_secret(&self) -> [u8; 32] {
        *self.device_secret
    }

    /// The public record a host keeps; no secrets.
    pub fn record(&self) -> CitizenRecord {
        CitizenRecord {
            profile: self.profile.clone(),
            device: self.device_id().to_string(),
            cert: self.author.cert.clone().unwrap_or_default(),
            definition: self.definition.clone(),
            avatar: self.avatar.clone(),
            memberships: Vec::new(),
        }
    }

    /// Sealed definition bytes for the relay's opaque `agent_citizens.definition` column.
    pub fn definition_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        self.definition.encode()
    }
}

/// Durable record the host keeps (and the relay stores opaquely).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CitizenRecord {
    pub profile: Identity,
    pub device: String,
    pub cert: String,
    pub definition: Definition,
    pub avatar: Avatar,
    pub memberships: Vec<String>,
}

/// The owner's device approves a Creator's proposal: the only way a proposed agent exists.
pub fn approve_agent(
    owner: &Author,
    owner_person: &Identity,
    spec: &super::AgentSpec,
) -> Result<Citizen, ClaimError> {
    claim_citizen(
        owner,
        owner_person,
        spec.name.clone(),
        spec.handle.clone(),
        spec.avatar(),
        spec.definition.clone(),
    )
}

/// Owner's certified device authorizes a new agent citizen with its own MLS root and device.
pub fn claim_citizen(
    owner: &Author,
    owner_person: &Identity,
    name: impl Into<String>,
    handle: impl Into<String>,
    avatar: Avatar,
    definition: Definition,
) -> Result<Citizen, ClaimError> {
    if owner_person.kind != IdentityKind::Person
        || owner.identity != owner_person.id
        || owner.device.is_none()
    {
        return Err(ClaimError::Owner);
    }
    let root = Signer::generate();
    let device = Signer::generate();
    let author = Author::device(&root, device);
    let proof = agent_owner::authorize(owner, &root.id()).ok_or(ClaimError::Owner)?;
    let name = name.into();
    let handle = handle.into();
    if name.trim().is_empty() || handle.trim().is_empty() {
        return Err(ClaimError::Profile);
    }
    let profile = Identity {
        id: root.id(),
        kind: IdentityKind::Agent,
        name,
        handle,
        tint_hex: avatar.tint_hex.clone(),
        glyph: Some(avatar.glyph.clone()),
        owner: Some(owner_person.id.clone()),
        bio: match definition.kind {
            AgentKind::Creator => "Creates agents from conversation.".into(),
            AgentKind::User => definition
                .persona
                .lines()
                .next()
                .unwrap_or("A Zoen agent.")
                .chars()
                .take(140)
                .collect(),
        },
        owner_proof: Some(Box::new(proof)),
    };
    if !agent_owner::profile_authorized(&profile) {
        return Err(ClaimError::Owner);
    }
    Ok(Citizen {
        root_secret: Zeroizing::new(root.secret()),
        device_secret: Zeroizing::new(author.key.secret()),
        profile,
        author,
        definition,
        avatar,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClaimError {
    #[error("owner device cannot authorize an agent")]
    Owner,
    #[error("agent profile needs a name and handle")]
    Profile,
}

impl AgentKind {
    /// The relay's spelling (`agent_citizens.kind`).
    pub fn relay_kind(self) -> &'static str {
        match self {
            AgentKind::Creator => "Creator",
            AgentKind::User => "User",
        }
    }
}
