use super::*;
use std::{
    future::{pending, ready},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
};

#[tokio::test(start_paused = true)]
async fn retryable_owner_resolution_returns_only_the_fresh_fence() {
    let mut calls = 0;
    let result = resolve(
        Instant::now() + Duration::from_secs(2),
        || {
            calls += 1;
            ready(match calls {
                1 => Err(Reject::retry("partition ownership is changing")),
                2 => Err(Reject::unavailable()),
                _ => Ok(crate::ownership::Fence {
                    partition: 7,
                    owner: "current".into(),
                    token: 12,
                }),
            })
        },
        || true,
    )
    .await
    .unwrap();
    assert_eq!(calls, 3);
    assert_eq!(result.owner, "current");
    assert_eq!(result.token, 12);
}

#[tokio::test(start_paused = true)]
async fn permanent_rejection_never_retries_even_if_its_text_looks_transient() {
    let mut calls = 0;
    let result = resolve::<(), _, _>(
        Instant::now() + Duration::from_secs(2),
        || {
            calls += 1;
            ready(Err(Reject::no("partition ownership is changing")))
        },
        || true,
    )
    .await;
    assert_eq!(calls, 1);
    assert_eq!(result, Err("partition ownership is changing".into()));
}

#[tokio::test(start_paused = true)]
async fn retryable_unavailability_expires_without_resetting_the_deadline() {
    let mut calls = 0;
    let deadline = Instant::now() + Duration::from_millis(10);
    let result = resolve::<(), _, _>(
        deadline,
        || {
            calls += 1;
            ready(Err(Reject::unavailable()))
        },
        || true,
    )
    .await;
    assert_eq!(calls, 1, "backoff is clipped to the original deadline");
    assert_eq!(result, Err(DEADLINE.into()));
    let result = resolve::<(), _, _>(
        deadline,
        || {
            calls += 1;
            ready(Ok(()))
        },
        || true,
    )
    .await;
    assert_eq!(
        calls, 1,
        "an expired deadline must not start more resolution work"
    );
    assert_eq!(result, Err(DEADLINE.into()));
}

#[tokio::test(start_paused = true)]
async fn loss_of_forwarding_stops_before_another_resolution() {
    let available = AtomicBool::new(true);
    let mut calls = 0;
    let result = resolve::<(), _, _>(
        Instant::now() + Duration::from_secs(2),
        || {
            calls += 1;
            available.store(false, Ordering::SeqCst);
            ready(Err(Reject::retry("partition ownership is changing")))
        },
        || available.load(Ordering::SeqCst),
    )
    .await;
    assert_eq!(calls, 1);
    assert_eq!(result, Err("cell forwarding unavailable".into()));
}

struct Dropped(Arc<AtomicUsize>);
impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[tokio::test(start_paused = true)]
async fn a_pending_resolver_is_cancelled_at_the_same_deadline() {
    let dropped = Arc::new(AtomicUsize::new(0));
    let result = resolve::<(), _, _>(
        Instant::now() + Duration::from_millis(10),
        || {
            let signal = dropped.clone();
            async move {
                let _lifetime = Dropped(signal);
                pending().await
            }
        },
        || true,
    )
    .await;
    assert_eq!(result, Err(DEADLINE.into()));
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
}
