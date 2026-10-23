//! Process-local contention stripes, never ownership or stored Space state.
//!
//! Independent clients can use the same logical owner/head. Without coordination
//! their queues can repeatedly beat each other's bounded optimistic transaction.
//! Fixed stripes avoid an unbounded key registry. A collision only serializes
//! unrelated heads; a process-random hash prevents choosing that collision from
//! public partition IDs. No key, permission, result or cache is retained here.
use super::{custom, FdbBindingError};
use std::{collections::hash_map::RandomState, hash::BuildHasher, sync::OnceLock};
use tokio::{
    sync::{Mutex, MutexGuard},
    time::{timeout, Duration},
};

const STRIPES: usize = super::super::sequencer::MAX_ACTIVE_SPACES;

struct CommitLanes {
    hash: RandomState,
    locks: Vec<Mutex<()>>,
}

impl CommitLanes {
    fn new() -> Self {
        Self {
            hash: RandomState::new(),
            locks: (0..STRIPES).map(|_| Mutex::new(())).collect(),
        }
    }

    fn index(&self, head: &[u8]) -> usize {
        let hash = self.hash.hash_one(head).to_le_bytes();
        usize::from(u16::from_le_bytes([hash[0], hash[1]])) % STRIPES
    }

    async fn acquire(&self, head: &[u8]) -> Result<MutexGuard<'_, ()>, FdbBindingError> {
        let lock = self
            .locks
            .get(self.index(head))
            .ok_or_else(|| custom("append coordination unavailable"))?;
        // Queue admission has the same finite budget as one stored transaction;
        // waiting never holds an FDB transaction or changes its retry/timeout.
        let millis = u64::try_from(crate::ownership::STORE_TIMEOUT_MS)
            .map_err(|_| custom("invalid append coordination budget"))?;
        timeout(Duration::from_millis(millis), lock.lock())
            .await
            .map_err(|_| custom("append coordination deadline exceeded"))
    }
}

static LANES: OnceLock<CommitLanes> = OnceLock::new();

pub(super) async fn acquire(head: &[u8]) -> Result<MutexGuard<'static, ()>, FdbBindingError> {
    LANES.get_or_init(CommitLanes::new).acquire(head).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::FutureExt;

    #[tokio::test]
    async fn same_head_is_fifo_and_cancellation_does_not_hold_the_lane() {
        let lanes = CommitLanes::new();
        let first = lanes.acquire(b"canonical-head").await.unwrap();
        let mut cancelled = Box::pin(lanes.acquire(b"canonical-head"));
        assert!(futures_util::poll!(cancelled.as_mut()).is_pending());
        let mut next = Box::pin(lanes.acquire(b"canonical-head"));
        assert!(futures_util::poll!(next.as_mut()).is_pending());
        drop(cancelled);
        assert!(futures_util::poll!(next.as_mut()).is_pending());
        drop(first);
        let second = next
            .now_or_never()
            .expect("cancellation released queue entry")
            .unwrap();
        drop(second);
        assert!(lanes
            .acquire(b"canonical-head")
            .now_or_never()
            .unwrap()
            .is_ok());
    }

    #[tokio::test(start_paused = true)]
    async fn expired_waiter_releases_its_place_without_acquiring_the_lane() {
        let lanes = CommitLanes::new();
        let held = lanes.acquire(b"busy-head").await.unwrap();
        let mut waiting = Box::pin(lanes.acquire(b"busy-head"));
        assert!(futures_util::poll!(waiting.as_mut()).is_pending());
        tokio::time::advance(Duration::from_millis(2_000)).await;
        assert!(waiting.await.is_err());
        drop(held);
        assert!(lanes.acquire(b"busy-head").await.is_ok());
    }

    #[tokio::test]
    async fn unrelated_heads_do_not_share_a_lock_unless_their_stripes_collide() {
        let lanes = CommitLanes::new();
        let head = b"first-head";
        let first = lanes.acquire(head).await.unwrap();
        let index = lanes.index(head);
        let other = (0..STRIPES * 2)
            .map(|number| format!("independent-head-{number}"))
            .find(|candidate| lanes.index(candidate.as_bytes()) != index)
            .expect("seeded hash distributes independent heads");
        let second = lanes
            .acquire(other.as_bytes())
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(lanes.locks.len(), STRIPES);
        drop((first, second));
    }
}
