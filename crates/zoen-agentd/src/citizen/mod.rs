//! First-class agent citizens (M3 / barber v1 base).
//!
//! An agent is the same kind of Zoen identity as a person: MLS root, certified device,
//! profile, avatar, Space membership and invites. The runtime holds the device while the
//! owner is offline. Exact code covers crypto, sync and permissions; persona and behavior
//! live in prompts and free JSON state.

mod codex;
mod creator;
mod host;
mod models;
mod profile;
pub mod testing;

pub use codex::CodexBrain;
pub use creator::{creator_definition, CREATOR_HANDLE, CREATOR_NAME};
pub use host::{
    parse_reply, AgentSpec, Brain, BrainOut, Host, HostError, PublicArtifactReferences, Runtime,
    ToolCall, ToolDef, ToolHost, ToolStep, Turn, TurnContext, TurnOutcome, CREATE_AGENT_CAPABILITY,
    MAX_CALLS_PER_ROUND, MAX_TOOL_ROUNDS,
};
pub use models::{system_prompt, CheapTierAuthority, ModelBrain, CHEAP_CHAT_PROFILE, CHEAP_MODEL};
pub use profile::{
    approve_agent, claim_citizen, AgentKind, Avatar, Citizen, CitizenRecord, ClaimError,
    Definition, ModelTier, AVATAR_STYLE,
};
