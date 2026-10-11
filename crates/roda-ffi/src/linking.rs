//! The engine's side of linked devices (ADR 0045): siblings in every group, unlinked ones
//! out, the history bundle and older pages between two devices of the same person.

use std::collections::{BTreeSet, HashMap};

use roda_log::{content_hash_of, verify_author, Author, Signer};
use roda_proto::DeviceRecord;
use roda_types::*;

use crate::engine::Engine;
use crate::i18n::t;
use crate::link::{Bundle, DeviceMsg, IdentityBox, Peer};
use crate::net::tracing_like;
use crate::sync::AccountMeta;
use crate::CoreError;

type R<T> = Result<T, CoreError>;

const META_ACTIVE: &str = "devices.active";
const META_REVOKED: &str = "devices.revoked";
/// The device that linked this one: it answers for older history.
pub const META_PRIMARY: &str = "link.primary";

/// Messages per chat in the first bundle (`ZOEN_HISTORY_RECENT`).
pub fn history_recent() -> usize {
    env_num("ZOEN_HISTORY_RECENT", 200)
}

/// Messages per older page (`ZOEN_HISTORY_PAGE`).
pub fn history_page() -> u32 {
    env_num("ZOEN_HISTORY_PAGE", 50) as u32
}

fn env_num(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|n| *n > 0)
        .unwrap_or(default)
}

fn set_of(v: Option<String>) -> BTreeSet<String> {
    v.and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_default()
}

/// Linked-device bookkeeping beside the network state.
#[derive(Default)]
pub struct LinkNet {
    /// Other devices of this person this one linked with (from the vault).
    pub peers: HashMap<String, Peer>,
    /// Sealed messages for other devices of ours, waiting for the network task.
    pub outbox: Vec<(String, Vec<u8>)>,
    /// Answers to our page requests: request id -> (messages opened, older ones left).
    pub answers: HashMap<String, (u32, bool)>,
    /// Ask the relay for our device list on this connection.
    pub ask_devices: bool,
}

impl Engine {
    /// An account handed over by another device of the same person.
    pub(crate) fn create_linked_account(
        &mut self,
        b: &IdentityBox,
        identity: &Signer,
        device: Signer,
        relay_url: &str,
    ) -> R<()> {
        if self.net.account.is_some() {
            return Err(CoreError::Invalid {
                reason: t(
                    "Este aparelho já tem uma conta.",
                    "This device already has an account.",
                ),
            });
        }
        if !self.is_empty() {
            self.wipe()?;
        }
        let me = Identity {
            owner_proof: None,
            id: identity.id(),
            kind: IdentityKind::Person,
            name: b.name.clone(),
            handle: b.handle.clone(),
            tint_hex: crate::sync::tint_for(&identity.id()),
            glyph: None,
            owner: None,
            bio: String::new(),
        };
        self.store.put_identity(&me, None)?;
        self.identity_order.push(me.id.clone());
        self.identities.insert(me.id.clone(), me.clone());
        self.store.set_meta("me", &me.id)?;
        self.me = Some(me.id.clone());
        if !b.profile_key.is_empty() {
            self.store
                .put_profile_key(&me.id, &b.profile_key, b.profile_key_version)?;
            if let Ok(fields) = hex::decode(&b.profile_fields) {
                self.store
                    .set_profile_fields(&me.id, b.profile_seen_version, &fields)?;
            }
        }
        self.save_linked_account(AccountMeta {
            identity: me.id.clone(),
            device: device.id(),
            cert: b.cert.clone(),
            relay_url: relay_url.trim_end_matches('/').to_string(),
            registered: true,
        })?;
        self.net.author = Some(Author::certified(me.id.clone(), device, b.cert.clone()));
        self.store.set_meta(META_PRIMARY, &b.primary)?;
        let mut active = set_of(self.store.meta(META_ACTIVE)?);
        active.insert(b.primary.clone());
        self.store.set_meta(
            META_ACTIVE,
            &serde_json::to_string(&active).unwrap_or_default(),
        )?;
        Ok(())
    }

    /// What the new device needs of the profile: the key, versions and fields.
    pub(crate) fn profile_for_link(&self) -> R<(String, u64, u64, String)> {
        let me = self.me_id()?;
        Ok(match self.store.profile_key(&me)? {
            Some(row) => (
                row.key,
                row.key_version,
                row.seen_version,
                hex::encode(row.fields.unwrap_or_default()),
            ),
            None => (String::new(), 0, 0, String::new()),
        })
    }

    pub(crate) fn add_peer(&mut self, peer: Peer) {
        self.net.link.peers.insert(peer.device.clone(), peer);
    }

    /// A device we just linked: it belongs in every group we are in.
    pub(crate) fn device_linked(&mut self, device: &str) -> R<()> {
        let mut active = set_of(self.store.meta(META_ACTIVE)?);
        active.insert(device.to_string());
        self.store.set_meta(
            META_ACTIVE,
            &serde_json::to_string(&active).unwrap_or_default(),
        )?;
        self.mls_siblings()
    }

    /// The relay's list of our devices.
    pub(crate) fn devices_arrived(&mut self, list: Vec<DeviceRecord>) {
        let active: BTreeSet<String> = list
            .iter()
            .filter(|d| !d.revoked)
            .map(|d| d.device.clone())
            .collect();
        let revoked: BTreeSet<String> = list
            .iter()
            .filter(|d| d.revoked)
            .map(|d| d.device.clone())
            .collect();
        let _ = self.store.set_meta(
            META_ACTIVE,
            &serde_json::to_string(&active).unwrap_or_default(),
        );
        let _ = self.store.set_meta(
            META_REVOKED,
            &serde_json::to_string(&revoked).unwrap_or_default(),
        );
        if let Err(e) = self.mls_siblings() {
            tracing_like(&format!("sibling devices: {e}"));
        }
        let spaces: Vec<SpaceId> = self.net.synced.iter().cloned().collect();
        self.net.mls.mark_dirty(spaces);
    }

    pub(crate) fn revoked_devices(&self) -> BTreeSet<String> {
        set_of(self.store.meta(META_REVOKED).ok().flatten())
    }

    pub(crate) fn unlinked(&mut self, device: &str) -> R<()> {
        let mut active = set_of(self.store.meta(META_ACTIVE)?);
        active.remove(device);
        let mut revoked = self.revoked_devices();
        revoked.insert(device.to_string());
        self.store.set_meta(
            META_ACTIVE,
            &serde_json::to_string(&active).unwrap_or_default(),
        )?;
        self.store.set_meta(
            META_REVOKED,
            &serde_json::to_string(&revoked).unwrap_or_default(),
        )?;
        let spaces: Vec<SpaceId> = self.net.synced.iter().cloned().collect();
        self.net.mls.mark_dirty(spaces);
        self.net.wake();
        Ok(())
    }

    /// Our other linked devices missing from a group we are in: ask for their leaf once.
    pub(crate) fn mls_siblings(&mut self) -> R<()> {
        let Some(mine) = self.net.account.as_ref().map(|a| a.device.clone()) else {
            return Ok(());
        };
        let me = self.me_id()?;
        let revoked = self.revoked_devices();
        let siblings: Vec<String> = set_of(self.store.meta(META_ACTIVE)?)
            .into_iter()
            .filter(|d| *d != mine && !revoked.contains(d))
            .collect();
        if siblings.is_empty() {
            return Ok(());
        }
        let spaces: Vec<SpaceId> = self
            .net
            .synced
            .iter()
            .filter(|s| self.is_e2e(s))
            .cloned()
            .collect();
        for space in spaces {
            let listed = self.mls_roster(&space).is_some_and(|r| r.contains_key(&me));
            let Some(leaves) = self.mls_leaves(&space) else {
                continue;
            };
            if !listed {
                continue;
            }
            for d in &siblings {
                let asked = format!("link.asked:{space}:{d}");
                if leaves.contains(&(me.clone(), d.clone())) || self.store.meta(&asked)?.is_some() {
                    continue;
                }
                self.append(&space, &me, EventBody::DeviceJoining { device: d.clone() })?;
                self.store.set_meta(&asked, "1")?;
            }
        }
        Ok(())
    }

    /// Groups we are in, and how many of them hold `device` (one of ours).
    pub(crate) fn link_progress(&self, device: &str) -> (u32, u32) {
        let Ok(me) = self.me_id() else {
            return (0, 0);
        };
        let mut done = 0;
        let mut total = 0;
        for space in self.net.synced.iter().filter(|s| self.is_e2e(s)) {
            if let Some(leaves) = self.mls_leaves(space) {
                total += 1;
                if leaves.contains(&(me.clone(), device.to_string())) {
                    done += 1;
                }
            }
        }
        (done, total)
    }

    /// Opened messages of `space` before `before`, the newest `limit`, and whether older
    /// ones are left.
    fn opened_before(&self, space: &str, before: u64, limit: usize) -> (Vec<Event>, bool) {
        let Some(log) = self.logs.get(space) else {
            return (Vec::new(), false);
        };
        let opened: Vec<&Event> = log
            .events()
            .iter()
            .filter(|e| e.seq < before && e.sealed_wire.is_some())
            .collect();
        let start = opened.len().saturating_sub(limit);
        (
            opened[start..].iter().map(|e| (*e).clone()).collect(),
            start > 0,
        )
    }

    /// The first bundle for a newly linked device: the recent window of every chat.
    pub(crate) fn history_bundle(&self) -> Bundle {
        let mut events = Vec::new();
        for space in self.net.synced.iter().filter(|s| self.is_e2e(s)) {
            let (mut page, _) = self.opened_before(space, u64::MAX, history_recent());
            events.append(&mut page);
        }
        Bundle { events }
    }

    /// Puts opened messages from another device of ours in place of the sealed entries this
    /// device kept. Each must be exactly the entry the relay ordered (same wire hash, which
    /// a pruned stub keeps too), signed by its author. Returns how many it took.
    pub(crate) fn import_opened(&mut self, events: Vec<Event>) -> R<u32> {
        let mut taken = 0;
        for mut e in events {
            let Some(local) = self
                .logs
                .get(&e.space)
                .and_then(|l| l.events().get(e.seq as usize))
                .cloned()
            else {
                continue;
            };
            let fits = matches!(local.body, EventBody::Sealed { .. })
                && local.sealed_wire.is_none()
                && e.sealed_wire.as_deref() == Some(content_hash_of(&local).as_str())
                && e.client_id == local.client_id
                && e.author == local.author
                && e.space == local.space
                && !matches!(e.body, EventBody::Sealed { .. })
                && verify_author(&e).is_ok();
            if !fits {
                continue;
            }
            e.prev = local.prev.clone();
            e.hash = local.hash.clone();
            if self.store.replace_event(&e)? {
                taken += 1;
            }
        }
        if taken > 0 {
            self.reload()?;
        }
        Ok(taken)
    }

    /// The oldest message this device has opened in `space` (where older pages start).
    pub(crate) fn oldest_opened(&self, space: &str) -> u64 {
        self.logs
            .get(space)
            .and_then(|l| {
                l.events()
                    .iter()
                    .find(|e| e.sealed_wire.is_some())
                    .map(|e| e.seq)
                    .or(Some(l.next_seq()))
            })
            .unwrap_or(0)
    }

    /// Asks the device that linked us for an older page of `space`. `None` when there is
    /// no such device.
    pub(crate) fn request_page(&mut self, space: &str) -> Option<String> {
        let primary = self.store.meta(META_PRIMARY).ok().flatten()?;
        // The other device said there is nothing older.
        if self
            .store
            .meta(&complete_meta(space))
            .ok()
            .flatten()
            .is_some()
        {
            return None;
        }
        let peer = self.net.link.peers.get(&primary)?.clone();
        let req = new_id("pg");
        let msg = DeviceMsg::PageRequest {
            req: req.clone(),
            space: space.to_string(),
            before: self.oldest_opened(space),
            limit: history_page(),
        };
        let sealed = peer.seal(&msg)?;
        self.net.link.outbox.push((primary, sealed));
        self.net.wake();
        Some(req)
    }

    pub(crate) fn page_answer(&mut self, req: &str) -> Option<(u32, bool)> {
        self.net.link.answers.remove(req)
    }

    pub(crate) fn take_device_outbox(&mut self) -> Vec<(String, Vec<u8>)> {
        std::mem::take(&mut self.net.link.outbox)
    }

    pub(crate) fn take_ask_devices(&mut self) -> bool {
        std::mem::take(&mut self.net.link.ask_devices)
    }

    /// Sealed bytes from another device of ours.
    pub fn device_message(&mut self, from: &str, to: &str, sealed: &[u8]) {
        tracing_like(&format!(
            "device message from {} to {}",
            &from[..12.min(from.len())],
            &to[..12.min(to.len())]
        ));
        if self.net.account.as_ref().map(|a| a.device.as_str()) != Some(to) {
            return;
        }
        let Some(peer) = self.net.link.peers.get(from).cloned() else {
            tracing_like(&format!("a device message from an unknown device {from}"));
            return;
        };
        let Some(msg) = peer.open(sealed) else {
            tracing_like("a device message that doesn't open");
            return;
        };
        match msg {
            DeviceMsg::PageRequest {
                req,
                space,
                before,
                limit,
            } => {
                let (events, more) = self.opened_before(&space, before, limit.min(500) as usize);
                let page = DeviceMsg::Page {
                    req,
                    space,
                    events,
                    more,
                };
                if let Some(sealed) = peer.seal(&page) {
                    self.net.link.outbox.push((from.to_string(), sealed));
                    self.net.wake();
                }
            }
            DeviceMsg::Page {
                req,
                space,
                events,
                more,
            } => {
                let n = self.import_opened(events).unwrap_or(0);
                if !more {
                    let _ = self.store.set_meta(&complete_meta(&space), "1");
                }
                self.net.link.answers.insert(req, (n, more));
            }
        }
    }
}

/// Set once the device that linked this one has sent the oldest page of a Space.
fn complete_meta(space: &str) -> String {
    format!("link.complete:{space}")
}
