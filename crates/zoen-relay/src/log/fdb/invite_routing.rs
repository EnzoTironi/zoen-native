//! Pre-effect owner resolution only. The caller retains one invite code, the
//! original whole-operation deadline and its existing two effect attempts.

use std::{future::Future, time::Duration};
use tokio::time::{sleep_until, timeout_at, Instant};

use crate::log::Reject;

const INITIAL_WAIT: Duration = Duration::from_millis(25);
const MAX_WAIT: Duration = Duration::from_millis(250);
const DEADLINE: &str = "relay routing deadline exceeded";

pub(super) async fn resolve<T, F, Fut>(
    deadline: Instant,
    mut resolve: F,
    forwarding_available: impl Fn() -> bool,
) -> Result<T, String>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, Reject>>,
{
    let mut delay = INITIAL_WAIT;
    loop {
        if !forwarding_available() {
            return Err("cell forwarding unavailable".into());
        }
        if Instant::now() >= deadline {
            return Err(DEADLINE.into());
        }
        match timeout_at(deadline, resolve()).await {
            Ok(Ok(fence)) => return Ok(fence),
            Ok(Err(rejection)) if rejection.permanent => return Err(rejection.reason),
            Ok(Err(_)) => {
                // A rejected resolution has not executed or forwarded an
                // invite. Wait without holding a transaction or creating a
                // second capability, then resolve a fresh persisted fence.
                sleep_until((Instant::now() + delay).min(deadline)).await;
                delay = (delay * 2).min(MAX_WAIT);
            }
            Err(_) => return Err(DEADLINE.into()),
        }
    }
}

#[cfg(test)]
mod tests;
