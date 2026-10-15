//! Structured notes an agent sends its owner (`agent_proposal`, `ask_owner`). They carry
//! power (approve an agent, release a gated draft), so the apps render them as cards only
//! when the author is the owner's own agent; from anyone else they stay plain text.

use roda_types::{Identity, IdentityKind};

use crate::RodaEngine;

/// The note keys the apps render as cards.
const KEYS: [&str; 2] = ["agent_proposal", "ask_owner"];

/// The note's JSON when `author` is an Agent owned by `me` and `text` is one fenced note.
pub(crate) fn owner_agent_note(me: &str, author: Option<&Identity>, text: &str) -> Option<String> {
    let a = author?;
    if a.kind != IdentityKind::Agent || a.owner.as_deref() != Some(me) || a.id == me {
        return None;
    }
    let body = text.trim().strip_prefix("```json")?.strip_suffix("```")?;
    let v: serde_json::Value = serde_json::from_str(body.trim()).ok()?;
    let obj = v.as_object()?;
    (obj.len() == 1 && KEYS.iter().any(|k| obj.contains_key(*k))).then(|| v.to_string())
}

#[uniffi::export]
impl RodaEngine {
    /// The card JSON for a message from `author_id`, only when that author is your own
    /// agent and the message is an `agent_proposal` or `ask_owner` note. Apps render the
    /// card from this and nothing else; `None` means show the text as an ordinary message.
    pub fn owner_agent_note(&self, author_id: String, text: String) -> Option<String> {
        let e = self.lock();
        let me = e.me_id().ok()?;
        let ids = e.store.identities().ok()?;
        let author = ids.iter().map(|(i, _)| i).find(|i| i.id == author_id);
        owner_agent_note(&me, author, &text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(id: &str, kind: IdentityKind, owner: Option<&str>) -> Identity {
        Identity {
            owner_proof: None,
            id: id.into(),
            kind,
            name: id.into(),
            handle: id.into(),
            tint_hex: "#19B5A5".into(),
            glyph: None,
            owner: owner.map(Into::into),
            bio: String::new(),
        }
    }

    const NOTE: &str = "```json\n{\"agent_proposal\":{\"source\":\"m1\",\"spec\":{}}}\n```";

    #[test]
    fn only_the_owners_own_agent_gets_a_card() {
        let mine = id("bot", IdentityKind::Agent, Some("bruno"));
        assert!(owner_agent_note("bruno", Some(&mine), NOTE).is_some());
        let ask = "```json\n{\"ask_owner\":{\"draft\":\"x\"}}\n```";
        assert!(owner_agent_note("bruno", Some(&mine), ask).is_some());

        // Someone else's agent, a person, an unknown author: plain text.
        let theirs = id("bot2", IdentityKind::Agent, Some("carla"));
        assert!(owner_agent_note("bruno", Some(&theirs), NOTE).is_none());
        let carla = id("carla", IdentityKind::Person, None);
        assert!(owner_agent_note("bruno", Some(&carla), NOTE).is_none());
        assert!(owner_agent_note("bruno", None, NOTE).is_none());

        // Other shapes from our own agent stay text too.
        assert!(owner_agent_note("bruno", Some(&mine), "oi").is_none());
        let other = "```json\n{\"grant_admin\":{}}\n```";
        assert!(owner_agent_note("bruno", Some(&mine), other).is_none());
    }
}
