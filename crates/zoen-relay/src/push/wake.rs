use super::{Outcome, Pusher, Target};

#[async_trait::async_trait]
trait DeviceTokens: Send + Sync {
    async fn targets(&self, identities: &[String]) -> Result<Vec<Target>, ()>;
    async fn remove_dead(&self, token: &str);
}

#[async_trait::async_trait]
impl DeviceTokens for sqlx::PgPool {
    async fn targets(&self, identities: &[String]) -> Result<Vec<Target>, ()> {
        // Existing SQL joins the current, unrevoked device record. Presence is not
        // authorization and cannot prove a sibling iPhone is executing in background.
        crate::db::push_targets(self, identities)
            .await
            .map_err(|_| ())
    }

    async fn remove_dead(&self, token: &str) {
        let _ = crate::db::drop_push_token(self, token).await;
    }
}

#[async_trait::async_trait]
trait WakeTransport: Send + Sync {
    async fn wake(&self, target: &Target) -> Outcome;
}

#[async_trait::async_trait]
impl WakeTransport for Pusher {
    async fn wake(&self, target: &Target) -> Outcome {
        Pusher::wake(self, target).await
    }
}

/// Counts attempts, not provider delivery. No identity, token or provider reason.
#[derive(Default, Debug, PartialEq, Eq)]
pub(crate) struct WakeAttempts {
    targets: usize,
    sent: usize,
    gone: usize,
    failed: usize,
    disabled: usize,
    lookup_failed: bool,
}

async fn dispatch(
    tokens: &impl DeviceTokens,
    transport: &impl WakeTransport,
    author: &str,
    audience: &[String],
) -> WakeAttempts {
    let recipients: Vec<_> = audience
        .iter()
        .filter(|id| *id != author)
        .cloned()
        .collect();
    if recipients.is_empty() {
        return WakeAttempts::default();
    }
    let Ok(targets) = tokens.targets(&recipients).await else {
        return WakeAttempts {
            lookup_failed: true,
            ..WakeAttempts::default()
        };
    };
    let mut observed = WakeAttempts {
        targets: targets.len(),
        ..WakeAttempts::default()
    };
    for target in targets {
        match transport.wake(&target).await {
            Outcome::Sent => observed.sent += 1,
            Outcome::Gone => {
                observed.gone += 1;
                tokens.remove_dead(&target.token).await;
            }
            Outcome::Failed(_) => observed.failed += 1,
            Outcome::Disabled => observed.disabled += 1,
        }
    }
    observed
}

pub(crate) async fn wake_registered_devices(
    pool: &sqlx::PgPool,
    pusher: &Pusher,
    author: &str,
    audience: &[String],
) {
    let count = dispatch(pool, pusher, author, audience).await;
    // A live WS is not a delivery receipt. This deliberately attempts every
    // authorized recipient device, including accounts with an online sibling.
    // Existing whole-job admission remains outside this function. No new retry.
    tracing::debug!(
        targets = count.targets,
        sent = count.sent,
        gone = count.gone,
        failed = count.failed,
        disabled = count.disabled,
        lookup_failed = count.lookup_failed,
        "content-free device wake attempts"
    );
}

#[cfg(test)]
mod tests;
