use std::{future::Future, time::Duration};

use tokio::sync::watch;

/// A healthy socket is not proof that its at-most-once live fanout was complete.
/// Recheck durable cursors once per minute without turning the 50ms UI/crypto
/// maintenance pass into a log poll. This bounds repair admission, not delivery
/// latency when the relay or authorization is unavailable. An unanswered scan reconnects
/// through the existing backoff after 30s; Pong cannot renew that deadline.
#[derive(Clone, Copy)]
pub(super) struct CursorRepairPolicy {
    pub(super) interval: Duration,
    pub(super) timeout: Duration,
}
impl CursorRepairPolicy {
    pub(super) const DEFAULT: Self = Self {
        interval: Duration::from_secs(60),
        timeout: Duration::from_secs(30),
    };
}

/// O(1) bookkeeping for the ordered, untagged SyncDone responses on THIS socket.
/// A repair may queue behind existing gap/join work, but only one repair is ever
/// in flight. Earlier/later partial completions cannot impersonate its completion.
pub(super) struct CursorRepair {
    policy: CursorRepairPolicy,
    pending: usize,
    repair_position: Option<usize>,
    due: Option<tokio::time::Instant>,
    deadline: Option<tokio::time::Instant>,
}
impl CursorRepair {
    pub(super) fn new(policy: CursorRepairPolicy) -> Self {
        Self {
            policy,
            pending: 1,
            repair_position: None,
            due: None,
            deadline: None,
        }
    }
    pub(super) fn submitted(&mut self) -> bool {
        let Some(next) = self.pending.checked_add(1) else {
            return false;
        };
        self.pending = next;
        true
    }
    pub(super) fn begin(&mut self, now: tokio::time::Instant) -> bool {
        let Some(due) = self.due.filter(|due| now >= *due) else {
            return false;
        };
        if self.repair_position.is_some() || !self.submitted() {
            return false;
        }
        self.repair_position = Some(self.pending);
        self.due = None;
        // Admission delayed by another write must not renew the maximum budget.
        self.deadline = Some(due + self.policy.timeout);
        true
    }
    pub(super) fn completed(&mut self, now: tokio::time::Instant) {
        if self.pending == 0 {
            return;
        }
        self.pending -= 1;
        if let Some(position) = self.repair_position {
            if position == 1 {
                self.repair_position = None;
                self.deadline = None;
                self.due = Some(now + self.policy.interval);
            } else {
                self.repair_position = Some(position - 1);
            }
        } else if self.due.is_none() {
            // The initial full sync is first on this socket. A later unsolicited
            // SyncDone or a partial replay does not extend the next repair time.
            self.due = Some(now + self.policy.interval);
        }
    }
    pub(super) fn admission(&self) -> Option<tokio::time::Instant> {
        self.due
    }
    pub(super) fn abort_deadline(&self) -> Option<tokio::time::Instant> {
        self.deadline
            .or_else(|| self.due.map(|due| due + self.policy.timeout))
    }
    pub(super) fn in_flight(&self) -> bool {
        self.repair_position.is_some()
    }
    pub(super) fn abandon_transport(&self, expired: bool, now: tokio::time::Instant) -> bool {
        // A ready work future may win the fair select over an already-ready
        // watchdog. Never enter another unbounded close after the same budget.
        expired || self.in_flight() || self.abort_deadline().is_some_and(|until| now >= until)
    }
}

/// The guard surrounds the dispatcher, not just its read select: a blocked write
/// or request flush cannot prevent the repair deadline from being polled. Both
/// futures are local to the session and are dropped together; no task is spawned.
pub(super) async fn drive(
    work: impl Future<Output = super::Exit>,
    mut deadline: watch::Receiver<Option<tokio::time::Instant>>,
) -> (super::Exit, bool) {
    let expired = async {
        loop {
            let current = *deadline.borrow_and_update();
            match current {
                Some(until) => tokio::select! {
                    _ = tokio::time::sleep_until(until) => {
                        // A completion may already have cleared/retargeted this
                        // timer while both select arms became ready.
                        if *deadline.borrow() == Some(until) { return; }
                    },
                    changed = deadline.changed() => {
                        if changed.is_err() {
                            tokio::time::sleep_until(until).await;
                            if *deadline.borrow() == Some(until) { return; }
                        }
                    }
                },
                None => {
                    if deadline.changed().await.is_err() {
                        std::future::pending::<()>().await;
                    }
                }
            }
        }
    };
    tokio::select! {
        exit = work => (exit, false),
        _ = expired => (super::Exit::Retry("cursor reconciliation timed out".into()), true),
    }
}

/// Cleanup must not outlive the CURRENT repair budget after the dispatcher exits.
/// Stop/retirement abandons without polling another transport write. A failed
/// initial catch-up has no periodic budget yet and retains its prior close path.
pub(super) async fn close_transport<T>(
    close: impl Future<Output = T>,
    deadline: Option<tokio::time::Instant>,
    abandon: bool,
) {
    if abandon || deadline.is_some_and(|until| tokio::time::Instant::now() >= until) {
        return;
    }
    if let Some(until) = deadline {
        let _ = tokio::time::timeout_at(until, close).await;
    } else {
        let _ = close.await;
    }
}
