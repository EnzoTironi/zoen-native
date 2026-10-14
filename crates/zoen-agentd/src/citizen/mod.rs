//! First-class agent citizens (M3 / barber v1 base).
//!
//! An agent is the same kind of Zoen identity as a person: MLS root, certified device,
//! profile, avatar, Space membership and invites. The runtime holds the device while the
//! owner is offline. Exact code covers crypto, sync and permissions; persona and behavior
//! live in prompts and free JSON state.

mod creator;
mod host;
mod models;
mod profile;

pub use creator::{creator_definition, CREATOR_HANDLE, CREATOR_NAME};
pub use host::{parse_reply, Brain, BrainOut, Host, HostError, ScriptedBrain, Turn, TurnOutcome};
pub use models::{system_prompt, CheapTierAuthority, ModelBrain, CHEAP_CHAT_PROFILE, CHEAP_MODEL};
pub use profile::{
    claim_citizen, AgentKind, Avatar, Citizen, CitizenRecord, Definition, ModelTier, AVATAR_STYLE,
};
