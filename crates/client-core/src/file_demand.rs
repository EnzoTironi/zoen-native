//! Device-local, bounded leases for explicitly opened historical file versions.
//! A canceled future releases only its waiter; no event, preference or key is stored.
//! A poisoned registry stays unavailable: neither old leases nor new requests may
//! reuse a partially changed state. Reopening the client creates a fresh registry.
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex, MutexGuard},
    time::Instant,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DemandUnavailable;
type DemandResult<T> = Result<T, DemandUnavailable>;

impl From<DemandUnavailable> for crate::CoreError {
    fn from(_: DemandUnavailable) -> Self {
        Self::Invalid {
            reason: crate::i18n::t(
                "Os downloads estão indisponíveis. Reabra o app para tentar novamente.",
                "Downloads are unavailable. Reopen the app to try again.",
            ),
        }
    }
}

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
    fn state(&self) -> DemandResult<MutexGuard<'_, State>> {
        self.0.lock().map_err(|_| DemandUnavailable)
    }

    pub fn acquire(
        &self,
        key: FileDemandKey,
        maximum_bytes: u64,
        deadline: Instant,
    ) -> DemandResult<Option<FileDemandLease>> {
        let mut state = self.state()?;
        state.prune();
        if (state.requests.len() >= 16 && !state.requests.contains_key(&key))
            || state.requests.values().map(HashMap::len).sum::<usize>() >= 32
        {
            return Ok(None);
        }
        let Some(next) = state.next.checked_add(1) else {
            return Ok(None);
        };
        state.next = next;
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
        Ok(Some(FileDemandLease {
            demands: self.clone(),
            key,
            id,
        }))
    }

    pub fn active(&self) -> DemandResult<Vec<(FileDemandKey, u64)>> {
        let mut state = self.state()?;
        state.prune();
        state
            .requests
            .iter()
            .map(|(key, waiters)| {
                let bound = waiters
                    .values()
                    .map(|(_, bound)| *bound)
                    .min()
                    .ok_or(DemandUnavailable)?;
                Ok((key.clone(), bound))
            })
            .collect()
    }

    pub fn foreground_attempts(&self) -> DemandResult<Vec<(FileDemandKey, u64)>> {
        let mut state = self.state()?;
        state.prune();
        state
            .foreground_attempts
            .iter()
            .map(|key| {
                let waiters = state.requests.get(key).ok_or(DemandUnavailable)?;
                let bound = waiters
                    .values()
                    .map(|(_, bound)| *bound)
                    .min()
                    .ok_or(DemandUnavailable)?;
                Ok((key.clone(), bound))
            })
            .collect()
    }

    /// Consume only after the engine has revalidated this exact active demand.
    pub fn consume_foreground_attempt(&self, key: &FileDemandKey) -> DemandResult<bool> {
        let mut state = self.state()?;
        state.prune();
        Ok(state.foreground_attempts.remove(key))
    }

    #[cfg(test)]
    pub fn poison_for_test(&self) {
        let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _held = self.0.lock().unwrap();
            panic!("fixture: interrupted demand mutation");
        }));
        assert!(failure.is_err());
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

    pub fn remove(&self, key: &FileDemandKey) -> DemandResult<()> {
        let mut state = self.state()?;
        state.requests.remove(key);
        state.foreground_attempts.remove(key);
        Ok(())
    }

    pub fn clear(&self) -> DemandResult<()> {
        let mut state = self.state()?;
        state.requests.clear();
        state.foreground_attempts.clear();
        Ok(())
    }
}

pub(crate) struct FileDemandLease {
    demands: FileDemands,
    key: FileDemandKey,
    id: u64,
}

impl FileDemandLease {
    pub fn active(&self) -> DemandResult<bool> {
        let mut state = self.demands.state()?;
        state.prune();
        Ok(state
            .requests
            .get(&self.key)
            .is_some_and(|waiters| waiters.contains_key(&self.id)))
    }
}

impl Drop for FileDemandLease {
    fn drop(&mut self) {
        // A poisoned registry cannot authorize any waiter. Do not inspect or
        // reset it during unwinding, and never recycle its waiter identifiers.
        let Ok(mut state) = self.demands.state() else {
            return;
        };
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
    fn poisoned_registry_refuses_existing_leases_and_new_foreground_authority() {
        let demands = FileDemands::default();
        let deadline = Instant::now() + Duration::from_secs(5);
        let lease = demands.acquire(key(0), 1024, deadline).unwrap().unwrap();
        let poisoned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _held = demands.0.lock().unwrap();
            panic!("fixture: interrupted demand mutation");
        }));
        assert!(poisoned.is_err());
        assert!(demands.0.is_poisoned());
        assert!(matches!(lease.active(), Err(DemandUnavailable)));
        assert!(matches!(
            demands.acquire(key(1), 1024, deadline),
            Err(DemandUnavailable)
        ));
        assert!(matches!(demands.active(), Err(DemandUnavailable)));
        assert!(matches!(
            demands.foreground_attempts(),
            Err(DemandUnavailable)
        ));
        assert!(matches!(
            demands.consume_foreground_attempt(&key(0)),
            Err(DemandUnavailable)
        ));
        assert!(matches!(demands.remove(&key(0)), Err(DemandUnavailable)));
        assert!(matches!(demands.clear(), Err(DemandUnavailable)));
        drop(lease);
        assert!(demands.0.is_poisoned(), "drop must not reset suspect state");
        assert!(demands.acquire(key(0), 1024, deadline).is_err());
    }

    #[test]
    fn file_demand_capacity_expiry_and_waiter_drop_preserve_other_requests() {
        let demands = FileDemands::default();
        let deadline = Instant::now() + Duration::from_secs(1);
        let mut leases = Vec::new();
        for item in 0..16 {
            leases.push(demands.acquire(key(item), 1024, deadline).unwrap().unwrap());
        }
        assert!(
            demands.acquire(key(16), 1024, deadline).unwrap().is_none(),
            "distinct versions bounded"
        );
        for _ in 16..32 {
            leases.push(demands.acquire(key(0), 1024, deadline).unwrap().unwrap());
        }
        assert!(
            demands.acquire(key(0), 1024, deadline).unwrap().is_none(),
            "coalesced waiters bounded too"
        );
        leases.pop();
        assert_eq!(demands.waiter_count(), 31);
        assert_eq!(demands.active().unwrap().len(), 16);
        let released = leases.remove(1);
        drop(released);
        assert_eq!(demands.active().unwrap().len(), 15);
        assert!(demands.acquire(key(16), 1024, deadline).unwrap().is_some());
        demands.clear().unwrap();
        assert!(leases.iter().all(|lease| !lease.active().unwrap()));
        let expired = demands
            .acquire(key(1), 1024, Instant::now() - Duration::from_millis(1))
            .unwrap()
            .unwrap();
        assert!(!expired.active().unwrap());
        assert!(demands.active().unwrap().is_empty());
    }

    #[test]
    fn foreground_attempt_is_once_per_episode_not_joiner_snapshot_or_wake() {
        let demands = FileDemands::default();
        let deadline = Instant::now() + Duration::from_secs(5);
        let first = demands.acquire(key(0), 1024, deadline).unwrap().unwrap();
        let joined = demands.acquire(key(0), 512, deadline).unwrap().unwrap();
        for _ in 0..3 {
            assert_eq!(demands.foreground_attempts().unwrap(), [(key(0), 512)]);
            assert_eq!(demands.active().unwrap(), [(key(0), 512)]);
        }
        assert!(!demands.consume_foreground_attempt(&key(1)).unwrap());
        assert!(demands.consume_foreground_attempt(&key(0)).unwrap());
        assert!(!demands.consume_foreground_attempt(&key(0)).unwrap());
        let later_joiner = demands.acquire(key(0), 512, deadline).unwrap().unwrap();
        drop(first);
        assert!(demands.foreground_attempts().unwrap().is_empty());
        drop(joined);
        assert!(demands.foreground_attempts().unwrap().is_empty());
        drop(later_joiner);
        let fresh = demands.acquire(key(0), 1024, deadline).unwrap().unwrap();
        assert_eq!(demands.foreground_attempts().unwrap(), [(key(0), 1024)]);
        drop(fresh);
        assert!(demands.foreground_attempts().unwrap().is_empty());
        assert!(!demands.consume_foreground_attempt(&key(0)).unwrap());
    }

    #[test]
    fn canceled_expired_removed_and_cleared_demands_discard_unconsumed_attempts() {
        let demands = FileDemands::default();
        let expired = demands
            .acquire(key(0), 1024, Instant::now() - Duration::from_millis(1))
            .unwrap()
            .unwrap();
        assert!(demands.foreground_attempts().unwrap().is_empty());
        assert!(!demands.consume_foreground_attempt(&key(0)).unwrap());
        drop(expired);
        let deadline = Instant::now() + Duration::from_secs(5);
        let removed = demands.acquire(key(1), 1024, deadline).unwrap().unwrap();
        demands.remove(&key(1)).unwrap();
        assert!(!demands.consume_foreground_attempt(&key(1)).unwrap());
        let cleared = demands.acquire(key(2), 1024, deadline).unwrap().unwrap();
        demands.clear().unwrap();
        assert!(!demands.consume_foreground_attempt(&key(2)).unwrap());
        assert!(!removed.active().unwrap() && !cleared.active().unwrap());
        assert!(demands.foreground_attempts().unwrap().is_empty());
    }
}
