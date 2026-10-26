//! # zoen-agentd
//!
//! The agent runtime (ADR 0028 + M3 citizen base). An agent is a first-class Zoen citizen:
//! its own MLS identity and device, profile, avatar, Space membership and invites. The host
//! holds that device on the relay while the owner is offline. Tools still run in the cheapest
//! tier their signed manifest allows (WASM, microVM, browser) under Grants.
//!
//! Exact code covers crypto, sync and permissions. Persona, rules and free JSON state are
//! prompts — including the Creator agent that mints user agents from conversation.

pub mod browser;
pub mod budget;
pub mod citizen;
pub mod deckcraft;
pub mod manifest;
pub mod router;
pub mod sandbox;
pub mod session;
pub mod wasm;

pub use browser::{BrowserCall, BrowserTools};
pub use citizen::{
    approve_agent, claim_citizen, creator_definition, AgentKind, AgentSpec, ApprovedToolAdmission,
    Avatar, Brain, BrainError, Citizen, CitizenRecord, Definition, Host, HostError, ModelTier,
    PublicArtifactReferences, Runtime, ToolAsk, ToolAskEnvelope, ToolCall, ToolDef, ToolHost,
    ToolObservation, ToolProgress, ToolResult, ToolStep, Turn, TurnContext, TurnOutcome,
    AVATAR_STYLE, CHEAP_CHAT_PROFILE, CREATOR_HANDLE, CREATOR_NAME, MAX_TOOL_CONTINUATION_BYTES,
    MAX_TOOL_STEPS, NATIVE_REVIEW_REQUIRED,
};
pub use manifest::{EgressRule, Needs, SecretBinding, SignedManifest, ToolManifest};
pub use router::{Route, Router, Tier};
pub use sandbox::{ExecOutput, ExecRequest, Lease, SandboxError, SandboxProvider, SandboxSpec};
