//! Test scheduling only; the actual process-global production semaphore is unchanged.
//! Ordinary relay fixtures remain concurrent. The capacity-exhaustion test needs
//! exclusive ownership so another fixture cannot consume one of its four slots.
use std::sync::{Arc, OnceLock};
use tokio::sync::{OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock};

static TEST_LANE: OnceLock<Arc<RwLock<()>>> = OnceLock::new();

pub(super) struct Lease {
    _shared: Option<OwnedRwLockReadGuard<()>>,
    _exclusive: Option<OwnedRwLockWriteGuard<()>>,
}
fn lane() -> Arc<RwLock<()>> {
    TEST_LANE.get_or_init(|| Arc::new(RwLock::new(()))).clone()
}
pub(super) async fn shared() -> Lease {
    Lease::shared(lane()).await
}
pub(super) async fn exclusive() -> Lease {
    Lease::exclusive(lane()).await
}
impl Lease {
    async fn shared(lane: Arc<RwLock<()>>) -> Self {
        Self {
            _shared: Some(lane.read_owned().await),
            _exclusive: None,
        }
    }
    async fn exclusive(lane: Arc<RwLock<()>>) -> Self {
        Self {
            _shared: None,
            _exclusive: Some(lane.write_owned().await),
        }
    }
}

#[tokio::test]
async fn ordinary_fixtures_overlap_but_capacity_fixture_waits_for_every_shared_lease() {
    // Use a separate instance of the exact scheduling primitive so this unit
    // regression cannot itself retain a process-global production test lease.
    let lane = Arc::new(RwLock::new(()));
    let first = Lease::shared(lane.clone()).await;
    let second = Lease::shared(lane.clone()).await;
    let mut writer = Box::pin(Lease::exclusive(lane.clone()));
    assert!(futures_util::poll!(writer.as_mut()).is_pending());
    drop(first);
    assert!(futures_util::poll!(writer.as_mut()).is_pending());
    drop(second);
    let held = writer.await;
    let mut reader = Box::pin(Lease::shared(lane));
    assert!(futures_util::poll!(reader.as_mut()).is_pending());
    drop(held);
    drop(reader.await);
}
