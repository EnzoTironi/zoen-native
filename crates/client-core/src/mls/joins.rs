//! Device joins, rejoin requests and rewelcome settlement.
use super::{
    identity_set, join_set, joins_meta, leaf_name, mls_err, rewelcome_meta, split_leaf, BTreeMap,
    BTreeSet, Engine, Event, EventBody, SpaceId, R,
};

impl Engine {
    pub(super) fn joins(&self, space: &str) -> BTreeMap<String, String> {
        self.store
            .meta(&joins_meta(space))
            .ok()
            .flatten()
            .and_then(|v| serde_json::from_str(&v).ok())
            .unwrap_or_default()
    }

    pub(super) fn set_joins(&self, space: &str, joins: &BTreeMap<String, String>) {
        let _ = if joins.is_empty() {
            self.store.meta_delete(&joins_meta(space))
        } else {
            self.store.set_meta(
                &joins_meta(space),
                &serde_json::to_string(joins).unwrap_or_default(),
            )
        };
    }

    /// A `DeviceJoining` landed: remember which leaf (if any) the device had at this point
    /// of the log, so every member agrees on when it has a fresh one.
    pub(crate) fn mls_device_joining(&mut self, e: &Event) {
        let EventBody::DeviceJoining { device } = &e.body else {
            return;
        };
        if !self.is_e2e(&e.space) {
            return;
        }
        let mine = self.net.account.as_ref().map(|a| a.device.as_str());
        if mine == Some(device.as_str()) {
            return;
        }
        let Ok(leaves) = self
            .device()
            .and_then(|d| d.leaves(&e.space).map_err(mls_err))
        else {
            return;
        };
        let stale = leaves
            .get(&(e.author.clone(), device.clone()))
            .map(hex::encode)
            .unwrap_or_default();
        let mut joins = self.joins(&e.space);
        joins.insert(leaf_name(&e.author, device), stale);
        self.set_joins(&e.space, &joins);
        self.net.mls.mark_group_dirty(e.space.clone());
    }

    /// Drops joins that are done: the device has a leaf with a new key, or its identity
    /// left. Nothing settles while our own commit is still on its way.
    pub(super) fn settle_joins(&mut self, space: &str) {
        let mut joins = self.joins(space);
        if joins.is_empty() {
            return;
        }
        let Ok(device) = self.device() else {
            return;
        };
        if device.pending(space) {
            return;
        }
        let Ok(leaves) = device.leaves(space) else {
            return;
        };
        drop(device);
        let roster = self.net.mls.rosters.get(space).cloned().unwrap_or_default();
        joins.retain(|name, stale| {
            let Some((id, dev)) = split_leaf(name) else {
                return false;
            };
            if !roster.contains_key(id) {
                return false;
            }
            match leaves.get(&(id.to_string(), dev.to_string())) {
                Some(key) => hex::encode(key) == *stale,
                None => true,
            }
        });
        self.set_joins(space, &joins);
    }

    /// After a catch-up: asks for a fresh leaf in each group this device fell out of.
    pub fn mls_ask_rejoin(&mut self) -> R<()> {
        let spaces: Vec<SpaceId> = self.net.mls.rejoin.drain().collect();
        let Some(device) = self.net.account.as_ref().map(|a| a.device.clone()) else {
            return Ok(());
        };
        let me = self.me_id()?;
        for space in spaces {
            let listed = self
                .net
                .mls
                .rosters
                .get(&space)
                .is_some_and(|r| r.contains_key(&me));
            if listed {
                self.append(
                    &space,
                    &me,
                    EventBody::DeviceJoining {
                        device: device.clone(),
                    },
                )?;
            }
        }
        Ok(())
    }

    /// People waiting for a fresh add whose old leaf is gone from our group: from here on
    /// they're simply listed and missing, and the next reconcile adds them. Nothing settles
    /// while our own commit is still on its way.
    pub(super) fn settle_rewelcome(&mut self, space: &str) {
        let key = rewelcome_meta(space);
        let waiting = identity_set(self.store.meta(&key).ok().flatten());
        if waiting.is_empty() {
            return;
        }
        let Ok(device) = self.device() else {
            return;
        };
        if device.pending(space) {
            return;
        }
        let Ok(leaves) = device.leaves(space).map_err(mls_err) else {
            return;
        };
        drop(device);
        let has = |name: &str| {
            split_leaf(name)
                .is_some_and(|(id, dev)| leaves.contains_key(&(id.to_string(), dev.to_string())))
        };
        let (left, gone): (BTreeSet<_>, BTreeSet<_>) =
            waiting.into_iter().partition(|name| has(name));
        // Their leaf is out: they are added again like any device joining.
        if !gone.is_empty() {
            let mut joins = self.joins(space);
            for name in gone {
                joins.entry(name).or_default();
            }
            self.set_joins(space, &joins);
        }
        let _ = if left.is_empty() {
            self.store.meta_delete(&key)
        } else {
            self.store.set_meta(&key, &join_set(&left))
        };
    }
}
