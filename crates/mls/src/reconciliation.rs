//! Pure membership planning shared by the persistent native and browser journals.
//! Callers must wait for ordered membership events and outstanding handshakes before
//! applying a plan; this module never consumes key packages or mutates MLS state.
use crate::{leaf_name, LeafKeys};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Owed {
    pub add: BTreeSet<String>,
    pub add_devices: BTreeSet<(String, String)>,
    pub remove: BTreeSet<String>,
}

impl Owed {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.add.is_empty() && self.add_devices.is_empty() && self.remove.is_empty()
    }
    #[must_use]
    pub fn adds_nobody(&self) -> bool {
        self.add.is_empty() && self.add_devices.is_empty()
    }
    /// The relay's consumptive claim protocol permits fifty identities per operation.
    #[must_use]
    pub fn to_claim(&self) -> Vec<String> {
        let mut ids = self.add.clone();
        ids.extend(self.add_devices.iter().map(|(id, _)| id.clone()));
        ids.into_iter().take(50).collect()
    }
}

#[derive(Clone, Copy)]
pub struct Input<'a> {
    pub me: &'a str,
    pub admin: bool,
    pub reconciling: bool,
    pub listed: &'a BTreeSet<String>,
    pub leaves: &'a LeafKeys,
    pub joins: &'a BTreeMap<String, String>,
    pub stranded: &'a BTreeSet<String>,
    pub revoked: &'a BTreeSet<String>,
}

#[must_use]
pub fn plan(input: Input<'_>) -> Option<Owed> {
    let Input {
        me,
        admin,
        reconciling,
        listed,
        leaves,
        joins,
        stranded,
        revoked,
    } = input;
    if !listed.contains(me) {
        return None;
    }
    let for_me = |name: &str| admin || name.split_once('/').is_some_and(|(id, _)| id == me);
    if !admin
        && !reconciling
        && !joins.keys().any(|name| for_me(name))
        && !stranded.iter().any(|name| for_me(name))
        && revoked.is_empty()
    {
        return None;
    }
    let group = leaves
        .keys()
        .map(|(id, _)| id.clone())
        .collect::<BTreeSet<_>>();
    let mut owed = Owed::default();
    if admin || reconciling {
        if admin {
            owed.add = listed.difference(&group).cloned().collect();
        }
        owed.remove = group.difference(listed).cloned().collect();
    }
    let has = |id: &str, device: &str| leaves.get(&(id.to_string(), device.to_string()));
    for (id, device) in leaves.keys() {
        if id == me && revoked.contains(device) {
            owed.remove.insert(leaf_name(id, device));
        }
    }
    for name in stranded.iter().filter(|name| for_me(name)) {
        if name
            .split_once('/')
            .is_some_and(|(id, device)| has(id, device).is_some())
        {
            owed.remove.insert(name.clone());
        }
    }
    for (name, stale) in joins.iter().filter(|(name, _)| for_me(name)) {
        let Some((id, device)) = name.split_once('/') else {
            continue;
        };
        if !listed.contains(id) || owed.add.contains(id) || owed.remove.contains(id) {
            continue;
        }
        match has(id, device) {
            Some(key) if hex::encode(key) == *stale => {
                owed.remove.insert(name.clone());
            }
            Some(_) => {}
            None => {
                owed.add_devices.insert((id.into(), device.into()));
            }
        }
    }
    (!owed.is_empty() || reconciling).then_some(owed)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn admins_plan_members_but_members_only_rotate_their_own_devices() {
        let listed = BTreeSet::from(["me".into(), "new".into(), "other".into()]);
        let leaves = LeafKeys::from([
            (("me".into(), "old".into()), vec![1]),
            (("other".into(), "old".into()), vec![2]),
            (("gone".into(), "x".into()), vec![3]),
        ]);
        let joins = BTreeMap::from([
            ("me/new".into(), String::new()),
            ("other/new".into(), String::new()),
        ]);
        let empty = BTreeSet::new();
        let make = |admin| Input {
            me: "me",
            admin,
            reconciling: false,
            listed: &listed,
            leaves: &leaves,
            joins: &joins,
            stranded: &empty,
            revoked: &empty,
        };
        let owner = plan(make(true)).unwrap();
        assert_eq!(owner.add, BTreeSet::from(["new".into()]));
        assert!(owner.remove.contains("gone"));
        assert_eq!(owner.add_devices.len(), 2);
        let member = plan(make(false)).unwrap();
        assert!(member.add.is_empty());
        assert!(member.remove.is_empty());
        assert_eq!(
            member.add_devices,
            BTreeSet::from([("me".into(), "new".into())])
        );
    }
    #[test]
    fn stale_leaf_is_removed_before_readding_and_wrong_stale_key_does_not_evict() {
        let listed = BTreeSet::from(["me".into()]);
        let leaves = LeafKeys::from([(("me".into(), "d".into()), vec![7])]);
        let empty = BTreeSet::new();
        let joins = BTreeMap::from([("me/d".into(), "07".into())]);
        let owed = plan(Input {
            me: "me",
            admin: false,
            reconciling: false,
            listed: &listed,
            leaves: &leaves,
            joins: &joins,
            stranded: &empty,
            revoked: &empty,
        })
        .unwrap();
        assert_eq!(owed.remove, BTreeSet::from(["me/d".into()]));
        assert!(owed.adds_nobody());
        let changed = BTreeMap::from([("me/d".into(), "08".into())]);
        assert!(plan(Input {
            me: "me",
            admin: false,
            reconciling: false,
            listed: &listed,
            leaves: &leaves,
            joins: &changed,
            stranded: &empty,
            revoked: &empty
        })
        .is_none());
    }
}
