//! Structured notes an agent sends its owner (`agent_proposal`, `ask_owner`, `booking`). They carry
//! power (approve an agent, release a gated draft), so the apps render them as cards only
//! when the author is the owner's own agent; from anyone else they stay plain text.

use roda_log::agent_owner::profile_authorized;
use roda_types::{Identity, IdentityKind, ItemContent};

use crate::RodaEngine;

/// The note keys the apps render as cards.
const KEYS: [&str; 3] = ["agent_proposal", "ask_owner", "booking"];

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

    /// The `booking` note an agent sends its owner for version `version` of a mini-app
    /// (a client booked or cancelled), with the conversation it happened in. `None` when
    /// that version is not a client's booking change.
    pub fn booking_note(&self, item_id: String, version: u32) -> Option<String> {
        let e = self.lock();
        let it = e.state.items.get(item_id.as_str())?;
        let i = (version as usize).checked_sub(1)?;
        let (prev, next) = (it.versions.get(i.checked_sub(1)?)?, it.versions.get(i)?);
        let (ItemContent::App(p), ItemContent::App(n)) = (&prev.content, &next.content) else {
            return None;
        };
        let mut note = crate::apps::owner_note_for(p, n, next.author.as_str())?;
        note["booking"]["space"] = it.space.as_str().into();
        Some(note.to_string())
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

    fn sched(bookings: serde_json::Value, events: serde_json::Value) -> roda_types::AppDoc {
        let st = serde_json::json!({"timezone": "America/Sao_Paulo", "bookings": bookings, "events": events});
        roda_types::AppDoc {
            app: "schedule".into(),
            resource_uri: String::new(),
            title: "Agenda".into(),
            state_json: st.to_string(),
        }
    }

    #[test]
    fn a_clients_booking_and_cancel_become_owner_notes() {
        use crate::apps::owner_note_for;
        use serde_json::json;
        let b = |status: &str| {
            json!([{"id": "b1", "byId": "carla", "byName": "Carla",
            "serviceName": "Corte", "startMs": 1_893_970_800_000_i64, "status": status}])
        };
        let empty = sched(json!([]), json!([]));
        let booked = sched(b("pending"), json!([]));
        let note = owner_note_for(&empty, &booked, "carla").expect("new booking");
        assert_eq!(note["booking"]["kind"], "new");
        assert_eq!(note["booking"]["client"], "Carla");
        let s = format!("```json\n{note}\n```");
        let bruno = Signer::generate();
        assert!(owner_agent_note(&bruno.id(), Some(&owned_agent(&bruno)), &s).is_some());

        let withdrawn = sched(b("cancelled"), json!([]));
        assert_eq!(
            owner_note_for(&booked, &withdrawn, "carla").unwrap()["booking"]["kind"],
            "cancelled"
        );
        let confirmed = sched(b("confirmed"), json!([]));
        let asked = sched(
            b("confirmed"),
            json!([{"bookingId": "b1", "kind": "cancel_request", "status": "open"}]),
        );
        assert_eq!(
            owner_note_for(&confirmed, &asked, "carla").unwrap()["booking"]["kind"],
            "cancelled"
        );

        // Staff confirming or declining, or someone else's write: nothing for the owner.
        assert!(owner_note_for(&booked, &confirmed, "bot").is_none());
        assert!(owner_note_for(&booked, &withdrawn, "bot").is_none());
        assert!(owner_note_for(&empty, &booked, "dani").is_none());
    }
}
