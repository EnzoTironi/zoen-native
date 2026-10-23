//! One monotonic startup-wait budget, borrowed by the final online export gate.
use client_core::{Client, ConnectionDto};
use std::time::Duration;
use tokio::time::Instant;

pub(crate) struct SyncBudget {
    timeout_ms: u64,
    deadline: Option<Instant>,
}

impl SyncBudget {
    pub(crate) fn new(timeout_ms: u64) -> Self {
        Self {
            timeout_ms,
            deadline: None,
        }
    }

    pub(crate) async fn wait_startup(&mut self, client: &Client) -> ConnectionDto {
        let timeout = match self.deadline {
            None => {
                // The first wait retains the existing argument exactly. Later
                // consumers cannot restart that same command's clock.
                self.deadline = Some(Instant::now() + Duration::from_millis(self.timeout_ms));
                self.timeout_ms
            }
            Some(deadline) => remaining_ms(deadline),
        };
        client.wait_until_idle(timeout).await
    }

    pub(crate) fn deadline(&self) -> Instant {
        // No startup (the explicit offline path) grants no future wait window.
        self.deadline.unwrap_or_else(Instant::now)
    }
}

pub(super) fn remaining_ms(deadline: Instant) -> u64 {
    u64::try_from(
        deadline
            .saturating_duration_since(Instant::now())
            .as_millis(),
    )
    .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unstarted_or_expired_budget_never_grants_a_fresh_window() {
        let budget = SyncBudget::new(60_000);
        assert!(budget.deadline() <= Instant::now());
        assert_eq!(remaining_ms(Instant::now()), 0);
        assert_eq!(remaining_ms(Instant::now() - Duration::from_secs(2)), 0);
    }

    #[tokio::test]
    async fn a_second_startup_wait_cannot_reset_the_original_deadline() {
        let client = Client::open(":memory:".into(), "en".into()).unwrap();
        let mut budget = SyncBudget::new(0);
        assert_eq!(budget.wait_startup(&client).await.state, "offline");
        let first = budget.deadline();
        assert_eq!(budget.wait_startup(&client).await.state, "offline");
        assert_eq!(budget.deadline(), first);
        assert_eq!(remaining_ms(first), 0);
    }
}
