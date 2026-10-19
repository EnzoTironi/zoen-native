//! Device-local, bounded leases for explicitly opened historical file versions.
//! A canceled future releases only its waiter; no event, preference or key is stored.
use std::{
    collections::HashMap,
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
}

impl State {
    fn prune(&mut self) {
        let now = Instant::now();
        self.requests.retain(|_, waiters| {
            waiters.retain(|_, (deadline, _)| *deadline > now);
            !waiters.is_empty()
        });
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
        self.0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .requests
            .remove(key);
    }

    pub fn clear(&self) {
        self.0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .requests
            .clear();
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
}
