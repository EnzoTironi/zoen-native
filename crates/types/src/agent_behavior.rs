//! Canonical Agent behavior shared by hosts and native execution.
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
    /// Serialize the existing versioned citizen definition.
    ///
    /// # Errors
    /// Returns a JSON serialization error if the definition cannot be encoded.
    pub fn encode(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }

    /// Read the host-compatible definition without changing its defaults.
    ///
    /// # Errors
    /// Returns a JSON error for malformed bytes or incompatible required fields.
    pub fn decode(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}

impl Definition {
    #[must_use]
    pub fn system_prompt(&self) -> String {
        let mut s = self.persona.clone();
        if !self.rules.is_empty() {
            s.push_str("\n\nRules:\n");
            for r in &self.rules {
                s.push_str("- ");
                s.push_str(r);
                s.push('\n');
            }
        }
        s.push_str("\nYour state (JSON, you own it):\n");
        s.push_str(&self.state.to_string());
        s
    }
}

impl AgentKind {
    /// The relay's spelling (`agent_citizens.kind`).
    #[must_use]
    pub fn relay_kind(self) -> &'static str {
        match self {
            AgentKind::Creator => "Creator",
            AgentKind::User => "User",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn existing_definition_bytes_and_prompt_stay_compatible() -> Result<(), serde_json::Error> {
        let wire = br#"{"version":1,"kind":"user","persona":"Careful helper","rules":["Ask first","Keep receipts"],"state":{"count":2},"model_tier":"cheap"}"#;
        let definition = Definition::decode(wire)?;
        assert_eq!(definition.encode()?, wire);
        assert_eq!(definition.system_prompt(), "Careful helper\n\nRules:\n- Ask first\n- Keep receipts\n\nYour state (JSON, you own it):\n{\"count\":2}");
        assert_eq!(definition.kind.relay_kind(), "User");
        let legacy = Definition::decode(
            br#"{"version":1,"kind":"creator","persona":"Ol\u00e1","rules":[]}"#,
        )?;
        assert_eq!(legacy.model_tier, ModelTier::Cheap);
        assert_eq!(legacy.kind.relay_kind(), "Creator");
        assert_eq!(
            legacy.system_prompt(),
            "Olá\nYour state (JSON, you own it):\nnull"
        );
        Ok(())
    }
}
