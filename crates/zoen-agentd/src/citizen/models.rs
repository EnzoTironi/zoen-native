//! Cheap-tier chat goes through the Zoen-owned model gateway (`zoen-models`).
//! Journeys use [`super::ScriptedBrain`]; production plugs a gateway-backed brain
//! that admits each attempt before send.

/// Wire profile for the default cheap chat path (OpenAI-compatible via Rig).
pub const CHEAP_CHAT_PROFILE: &str = zoen_models::CHAT_PROFILE;
