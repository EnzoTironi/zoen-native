//! Confirmed membership and encrypted profile-key shares.

use super::*;

#[cfg(test)]
mod tests;

impl Engine {
    // ── shares ──

    /// Reacts to a sequenced or local event: membership changes may need shares, a share
    /// addressed to me brings a key.
    pub(crate) fn note_profile_event(&mut self, e: &Event) {
        match &e.body {
            EventBody::SpaceCreated { .. } | EventBody::MemberAdded { .. } => {
                self.net.profiles.shares_dirty = true;
            }
            EventBody::ProfileKeyShared { shares, .. } => {
                let Some(me) = self.me.clone() else { return };
                if e.author == me {
                    return;
                }
                let Some(agreement) = &self.net.profiles.agreement else {
                    return;
                };
                let Some(share) = shares.iter().find(|s| s.to == me) else {
                    return;
                };
                let sealed = SealedShare {
                    ephemeral: share.ephemeral.clone(),
                    sealed: share.sealed.clone(),
                };
                let Some((key, version)) =
                    open_share(agreement, &sealed, &share_context(&e.space, &e.author, &me))
                else {
                    return;
                };
                if matches!(
                    self.store
                        .put_profile_key(&e.author, &key.to_hex(), version),
                    Ok(true)
                ) {
                    self.net.profiles.need_profiles.insert(e.author.clone());
                }
            }
            _ => {}
        }
    }

    pub(super) fn already_shared(&mut self, me: &str, version: u64) -> R<HashSet<IdentityId>> {
        if let Some((v, set)) = &self.net.profiles.shared {
            if *v == version {
                return Ok(set.clone());
            }
        }
        let mut set = HashSet::new();
        let mut take = |e: &Event| {
            if let EventBody::ProfileKeyShared { version: v, shares } = &e.body {
                if e.author == me && *v == version {
                    set.extend(shares.iter().map(|s| s.to.clone()));
                }
            }
        };
        for log in self.net.synced.iter().filter_map(|s| self.logs.get(s)) {
            log.events().iter().for_each(&mut take);
        }
        for p in self.store.outbox()? {
            if !p.failed {
                take(&p.event);
            }
        }
        self.net.profiles.shared = Some((version, set.clone()));
        Ok(set)
    }

    /// Shares my key with everyone I share a relay Space with who doesn't hold it yet.
    /// People whose agreement key isn't known are queued for a lookup (which re-runs this).
    pub fn profile_share_pass(&mut self) -> R<bool> {
        if !self.net.profiles.supported || !self.net.profiles.shares_dirty {
            return Ok(false);
        }
        let result = self.write_profile_shares();
        if result.is_ok() {
            self.net.profiles.shares_dirty = false;
        } else {
            // Some encrypted shares may already be durable. The next pass must
            // rebuild from those exact events instead of an incomplete cache.
            self.net.profiles.shared = None;
        }
        result
    }

    fn write_profile_shares(&mut self) -> R<bool> {
        let (Some(me), Some(author)) = (self.me.clone(), self.net.author.clone()) else {
            return Ok(false);
        };
        let (key, key_version, _, _) = self.my_fields(&me)?;
        let blocked: HashSet<IdentityId> = self.store.blocked()?.into_iter().collect();
        let mut done = self.already_shared(&me, key_version)?;
        let mut spaces: Vec<SpaceId> = self
            .net
            .synced
            .iter()
            .filter(|s| self.state.spaces.contains_key(*s))
            .cloned()
            .collect();
        spaces.sort();
        let mut wrote = false;
        for space in spaces {
            let members: Vec<IdentityId> = self.state.spaces[&space]
                .members
                .iter()
                .map(|(m, _)| m.clone())
                .collect();
            if !members.contains(&me) {
                continue;
            }
            let mut shares = Vec::new();
            for m in members {
                if m == me
                    || done.contains(&m)
                    || blocked.contains(&m)
                    || self.net.profiles.no_agreement.contains(&m)
                    || self
                        .identities
                        .get(&m)
                        .is_some_and(|i| i.kind == IdentityKind::Agent)
                {
                    continue;
                }
                let Some(public) = self.store.peer_agreement_key(&m)? else {
                    self.net.profiles.need_agreement.insert(m);
                    continue;
                };
                if let Some(s) =
                    seal_share(&key, key_version, &public, &share_context(&space, &me, &m))
                {
                    done.insert(m.clone());
                    shares.push(ProfileKeyShare {
                        to: m,
                        ephemeral: s.ephemeral,
                        sealed: s.sealed,
                    });
                }
            }
            for chunk in shares.chunks(SHARES_PER_EVENT) {
                self.append_synced(
                    &space,
                    &author,
                    now_ms(),
                    EventBody::ProfileKeyShared {
                        version: key_version,
                        shares: chunk.to_vec(),
                    },
                )?;
                wrote = true;
            }
        }
        self.net.profiles.shared = Some((key_version, done));
        Ok(wrote)
    }
}
