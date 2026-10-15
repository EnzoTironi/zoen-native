//! Structured notes an agent sends its owner (`agent_proposal`, `ask_owner`). They carry
//! power (approve an agent, release a gated draft), so the apps render them as cards only
//! when the author is the owner's own agent; from anyone else they stay plain text.

use roda_log::agent_owner::profile_authorized;
use roda_types::{Identity, IdentityKind};

use crate::RodaEngine;

/// The note keys the apps render as cards.
const KEYS: [&str; 2] = ["agent_proposal", "ask_owner"];

/// The note's JSON when `author` is an Agent owned by `me` and `text` is one fenced note.
pub(crate) fn owner_agent_note(me: &str, author: Option<&Identity>, text: &str) -> Option<String> {
    let a = author?;
    // The stored `owner` field alone is a claim; only a profile whose ownership proof
    // verifies (signed by one of the owner's certified devices) may speak with power.
    if a.kind != IdentityKind::Agent
        || a.owner.as_deref() != Some(me)
        || a.id == me
        || !profile_authorized(a)
    {
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
    use roda_log::{agent_owner::authorize, Author, Signer};

    /// An agent profile carrying a real ownership proof from `owner`'s certified device.
    fn owned_agent(owner: &Signer) -> Identity {
        let agent = Signer::generate().id();
        let proof = authorize(&Author::device(owner, Signer::generate()), &agent).unwrap();
        Identity {
            owner_proof: Some(Box::new(proof)),
            ..id(&agent, IdentityKind::Agent, Some(&owner.id()))
        }
    }

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
        let bruno = Signer::generate();
        let me = bruno.id();
        let mine = owned_agent(&bruno);
        assert!(owner_agent_note(&me, Some(&mine), NOTE).is_some());
        let ask = "```json\n{\"ask_owner\":{\"draft\":\"x\"}}\n```";
        assert!(owner_agent_note(&me, Some(&mine), ask).is_some());

        // Someone else's agent, a person, an unknown author: plain text.
        let theirs = owned_agent(&Signer::generate());
        assert!(owner_agent_note(&me, Some(&theirs), NOTE).is_none());
        let carla = id("carla", IdentityKind::Person, None);
        assert!(owner_agent_note(&me, Some(&carla), NOTE).is_none());
        assert!(owner_agent_note(&me, None, NOTE).is_none());

        // Other shapes from our own agent stay text too.
        assert!(owner_agent_note(&me, Some(&mine), "oi").is_none());
        let other = "```json\n{\"grant_admin\":{}}\n```";
        assert!(owner_agent_note(&me, Some(&mine), other).is_none());
    }

    #[test]
    fn a_claimed_owner_without_a_valid_proof_gets_no_card() {
        let bruno = Signer::generate();
        let me = bruno.id();
        // Claims Bruno as owner but carries no proof.
        let bare = id(&Signer::generate().id(), IdentityKind::Agent, Some(&me));
        assert!(owner_agent_note(&me, Some(&bare), NOTE).is_none());
        // Carries a proof, but it was signed by someone else's device.
        let forged = Identity {
            owner: Some(me.clone()),
            ..owned_agent(&Signer::generate())
        };
        assert!(owner_agent_note(&me, Some(&forged), NOTE).is_none());
    }
}
