//! First-class agent citizens (M3 / barber v1 base).
//!
//! An agent is the same kind of Zoen identity as a person: MLS root, certified device,
//! profile, avatar, Space membership and invites. The runtime holds the device while the
//! owner is offline. Exact code covers crypto, sync and permissions; persona and behavior
//! live in prompts and free JSON state.

mod creator;
mod models;
mod host;
mod profile;

pub use creator::{creator_definition, CREATOR_HANDLE, CREATOR_NAME};
pub use host::{Brain, Host, HostError, ScriptedBrain, Turn, TurnOutcome};
pub use models::CHEAP_CHAT_PROFILE;
pub use profile::{
    claim_citizen, AgentKind, Avatar, Citizen, CitizenRecord, Definition, ModelTier,
    AVATAR_STYLE,
};
