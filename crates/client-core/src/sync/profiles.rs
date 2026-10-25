//! Directory/profile reconciliation and transient presence.

use super::{
    normalize_handle, t, CoreError, Engine, Event, EventBody, HashSet, Identity, IdentityId,
    IdentityKind, R,
};

impl Engine {
    pub(super) fn recompute_unknown(&mut self) {
        let mut unknown = HashSet::new();
        for s in self.state.spaces.values() {
            for (m, _) in &s.members {
                if !self.identities.contains_key(m) {
                    unknown.insert(m.clone());
                }
            }
            for e in &s.entries {
                if !self.identities.contains_key(&e.author) {
                    unknown.insert(e.author.clone());
                }
            }
        }
        self.net.unknown = unknown;
    }

    pub(crate) fn note_unknown(&mut self, e: &Event) {
        if !self.identities.contains_key(&e.author) {
            self.net.unknown.insert(e.author.clone());
        }
        if let EventBody::MemberAdded { identity, .. } = &e.body {
            if !self.identities.contains_key(identity) {
                self.net.unknown.insert(identity.clone());
            }
        }
    }

    pub fn take_unknown(&mut self) -> Vec<IdentityId> {
        self.net.unknown.iter().take(500).cloned().collect()
    }

    /// The relay answered a profile lookup: whoever it didn't know stays unnamed
    /// (until the next launch) instead of being asked for again and again.
    pub fn profiles_answered(&mut self, asked: &[IdentityId]) {
        for id in asked {
            self.net.unknown.remove(id);
        }
    }

    /// Directory entries from the relay: handles and public keys. A person's name and bio
    /// come from their encrypted profile when this device holds the key, else the @handle.
    pub fn put_profiles(&mut self, profiles: Vec<Identity>) -> R<()> {
        let mut reproject_agents = false;
        for mut p in profiles {
            if p.id.len() != 64 || !event_log::agent_owner::profile_authorized(&p) {
                continue;
            }
            // Never let the directory rename someone this device holds keys for.
            if self.signers.contains_key(&p.id) || self.me.as_deref() == Some(p.id.as_str()) {
                continue;
            }
            if self.identities.get(&p.id).is_some_and(|known| {
                // Unsigned legacy directory metadata cannot establish an owner pin.
                known.owner_proof.is_some()
                    && event_log::agent_owner::profile_authorized(known)
                    && (known.kind != p.kind
                        || known.owner != p.owner
                        || known.owner_proof != p.owner_proof)
            }) {
                continue;
            }
            reproject_agents |= p.kind == IdentityKind::Agent
                && self
                    .identities
                    .get(&p.id)
                    .is_none_or(|old| old.kind != p.kind || old.owner != p.owner);
            self.overlay_profile(&mut p);
            self.store.put_identity(&p, None)?;
            if !self.identities.contains_key(&p.id) {
                self.identity_order.push(p.id.clone());
            }
            self.net.unknown.remove(&p.id);
            self.identities.insert(p.id.clone(), p);
            self.index_dirty = true;
        }
        if reproject_agents {
            self.reload()?;
        }
        Ok(())
    }

    pub fn my_profile(&self) -> Option<Identity> {
        let id = self.net.account.as_ref()?.identity.clone();
        self.identities.get(&id).cloned()
    }

    pub fn update_profile(&mut self, name: &str, handle: &str, bio: &str) -> R<()> {
        let mut me = self.my_profile().ok_or_else(|| CoreError::NotFound {
            what: "account".into(),
        })?;
        me.handle = normalize_handle(handle).ok_or_else(|| CoreError::Invalid {
            reason: t("@ inválido.", "Invalid @."),
        })?;
        if !name.trim().is_empty() {
            me.name = name.trim().to_string();
        }
        me.bio = bio.chars().take(280).collect();
        self.store.put_identity(&me, None)?;
        let (name, bio) = (me.name.clone(), me.bio.clone());
        self.identities.insert(me.id.clone(), me);
        self.set_profile_text(&name, &bio)?;
        self.set_registered(false)?;
        self.net.wake();
        Ok(())
    }

    pub fn set_presence(&mut self, who: &str, online: bool) {
        self.net.presence.insert(who.to_string(), online);
    }

    pub fn is_online(&self, who: &str) -> bool {
        self.net.presence.get(who).copied().unwrap_or(false)
    }
}
