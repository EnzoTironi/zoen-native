//! Device-local, bounded leases for explicitly opened historical file versions.
//! A canceled future releases only its waiter; no event, preference or key is stored.
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Instant,
};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct FileDemandKey {
    pub item: String,
    pub version: u32,
    pub sha256: String,
    pub identity: String,
    pub device: String,
}

#[derive(Default)]
struct State {
    next: u64,
    requests: HashMap<FileDemandKey, HashMap<u64, (Instant, u64)>>,
    foreground_attempts: HashSet<FileDemandKey>,
}

impl State {
    fn prune(&mut self) {
        let now = Instant::now();
        self.requests.retain(|_, waiters| {
            waiters.retain(|_, (deadline, _)| *deadline > now);
            !waiters.is_empty()
        });
        self.foreground_attempts
            .retain(|key| self.requests.contains_key(key));
    }
}

#[derive(Clone, Default)]
pub(crate) struct FileDemands(Arc<Mutex<State>>);

impl FileDemands {
    pub fn acquire(
        &self,
        key: FileDemandKey,
        maximum_bytes: u64,
        deadline: Instant,
    ) -> Option<FileDemandLease> {
        let mut state = self.0.lock().unwrap_or_else(|p| p.into_inner());
        state.prune();
        if (state.requests.len() >= 16 && !state.requests.contains_key(&key))
            || state.requests.values().map(HashMap::len).sum::<usize>() >= 32
        {
            return None;
        }
        state.next = state.next.checked_add(1)?;
        let id = state.next;
        // One pulse per new foreground-demand episode, not per waiter or poll.
        if !state.requests.contains_key(&key) {
            state.foreground_attempts.insert(key.clone());
        }
        state
            .requests
            .entry(key.clone())
            .or_default()
            .insert(id, (deadline, maximum_bytes));
        Some(FileDemandLease {
            demands: self.clone(),
            key,
            id,
        })
    }

    pub fn active(&self) -> Vec<(FileDemandKey, u64)> {
        let mut state = self.0.lock().unwrap_or_else(|p| p.into_inner());
        state.prune();
        state
            .requests
            .iter()
            .map(|(key, waiters)| {
                (
                    key.clone(),
                    waiters.values().map(|(_, bound)| *bound).min().unwrap(),
                )
            })
            .collect()
    }

    pub fn foreground_attempts(&self) -> Vec<(FileDemandKey, u64)> {
        let mut state = self.0.lock().unwrap_or_else(|p| p.into_inner());
        state.prune();
        state
            .foreground_attempts
            .iter()
            .map(|key| {
                let waiters = &state.requests[key];
                (
                    key.clone(),
                    waiters.values().map(|(_, bound)| *bound).min().unwrap(),
                )
            })
            .collect()
    }

    /// Consume only after the engine has revalidated this exact active demand.
    pub fn consume_foreground_attempt(&self, key: &FileDemandKey) -> bool {
        let mut state = self.0.lock().unwrap_or_else(|p| p.into_inner());
        state.prune();
        state.foreground_attempts.remove(key)
    }

    #[cfg(test)]
    pub fn waiter_count(&self) -> usize {
        self.0
            .lock()
            .unwrap()
            .requests
            .values()
            .map(HashMap::len)
            .sum()
    }

    pub fn remove(&self, key: &FileDemandKey) {
        let mut state = self.0.lock().unwrap_or_else(|p| p.into_inner());
        state.requests.remove(key);
        state.foreground_attempts.remove(key);
    }

    pub fn clear(&self) {
        let mut state = self.0.lock().unwrap_or_else(|p| p.into_inner());
        state.requests.clear();
        state.foreground_attempts.clear();
    }
}

pub(crate) struct FileDemandLease {
    demands: FileDemands,
    key: FileDemandKey,
    id: u64,
}

impl FileDemandLease {
    pub fn active(&self) -> bool {
        let mut state = self.demands.0.lock().unwrap_or_else(|p| p.into_inner());
        state.prune();
        state
            .requests
            .get(&self.key)
            .is_some_and(|waiters| waiters.contains_key(&self.id))
    }
}

impl Drop for FileDemandLease {
    fn drop(&mut self) {
        let mut state = self.demands.0.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(waiters) = state.requests.get_mut(&self.key) {
            waiters.remove(&self.id);
            if waiters.is_empty() {
                state.requests.remove(&self.key);
                state.foreground_attempts.remove(&self.key);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn key(item: usize) -> FileDemandKey {
        FileDemandKey {
            item: format!("item-{item}"),
            version: 1,
            sha256: "synthetic".into(),
            identity: "owned".into(),
            device: "device".into(),
        }
    }

    #[test]
    fn file_demand_capacity_expiry_and_waiter_drop_preserve_other_requests() {
        let demands = FileDemands::default();
        let deadline = Instant::now() + Duration::from_secs(1);
        let mut leases = Vec::new();
        for item in 0..16 {
            leases.push(demands.acquire(key(item), 1024, deadline).unwrap());
        }
        assert!(
            demands.acquire(key(16), 1024, deadline).is_none(),
            "distinct versions bounded"
        );
        for _ in 16..32 {
            leases.push(demands.acquire(key(0), 1024, deadline).unwrap());
        }
        assert!(
            demands.acquire(key(0), 1024, deadline).is_none(),
            "coalesced waiters bounded too"
        );
        leases.pop();
        assert_eq!(demands.waiter_count(), 31);
        assert_eq!(demands.active().len(), 16);
        let released = leases.remove(1);
        drop(released);
        assert_eq!(demands.active().len(), 15);
        assert!(demands.acquire(key(16), 1024, deadline).is_some());
        demands.clear();
        assert!(leases.iter().all(|lease| !lease.active()));
        let expired = demands
            .acquire(key(1), 1024, Instant::now() - Duration::from_millis(1))
            .unwrap();
        assert!(!expired.active());
        assert!(demands.active().is_empty());
    }

    #[test]
    fn foreground_attempt_is_once_per_episode_not_joiner_snapshot_or_wake() {
        let demands = FileDemands::default();
        let deadline = Instant::now() + Duration::from_secs(5);
        let first = demands.acquire(key(0), 1024, deadline).unwrap();
        let joined = demands.acquire(key(0), 512, deadline).unwrap();
        for _ in 0..3 {
            assert_eq!(demands.foreground_attempts(), [(key(0), 512)]);
            assert_eq!(demands.active(), [(key(0), 512)]);
        }
        assert!(!demands.consume_foreground_attempt(&key(1)));
        assert!(demands.consume_foreground_attempt(&key(0)));
        assert!(!demands.consume_foreground_attempt(&key(0)));
        let later_joiner = demands.acquire(key(0), 512, deadline).unwrap();
        drop(first);
        assert!(demands.foreground_attempts().is_empty());
        drop(joined);
        assert!(demands.foreground_attempts().is_empty());
        drop(later_joiner);
        let fresh = demands.acquire(key(0), 1024, deadline).unwrap();
        assert_eq!(demands.foreground_attempts(), [(key(0), 1024)]);
        drop(fresh);
        assert!(demands.foreground_attempts().is_empty());
        assert!(!demands.consume_foreground_attempt(&key(0)));
    }

    #[test]
    fn canceled_expired_removed_and_cleared_demands_discard_unconsumed_attempts() {
        let demands = FileDemands::default();
        let expired = demands
            .acquire(key(0), 1024, Instant::now() - Duration::from_millis(1))
            .unwrap();
        assert!(demands.foreground_attempts().is_empty());
        assert!(!demands.consume_foreground_attempt(&key(0)));
        drop(expired);
        let deadline = Instant::now() + Duration::from_secs(5);
        let removed = demands.acquire(key(1), 1024, deadline).unwrap();
        demands.remove(&key(1));
        assert!(!demands.consume_foreground_attempt(&key(1)));
        let cleared = demands.acquire(key(2), 1024, deadline).unwrap();
        demands.clear();
        assert!(!demands.consume_foreground_attempt(&key(2)));
        assert!(!removed.active() && !cleared.active());
        assert!(demands.foreground_attempts().is_empty());
    }
}
