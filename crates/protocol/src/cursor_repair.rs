//! Connection-local ordered cursor reconciliation, independent of host clock/runtime.
use std::time::Duration;

/// A healthy socket is not proof that its at-most-once live fanout was complete.
/// Recheck durable cursors once per minute without turning the 50ms UI/crypto
/// maintenance pass into a log poll. This bounds repair admission, not delivery
/// latency when the relay or authorization is unavailable. Initial catch-up must
/// advance verified durable cursors within 60s; after its first `SyncDone`, an
/// unanswered periodic scan reconnects after 30s. These are provisional Zoen
/// inactivity policies, not measured SLAs or Signal constants. Pong renews neither.
#[derive(Clone, Copy)]
pub struct CursorRepairPolicy {
    pub interval: Duration,
    pub timeout: Duration,
    pub initial_inactivity: Duration,
}
impl CursorRepairPolicy {
    pub const DEFAULT: Self = Self {
        interval: Duration::from_secs(60),
        timeout: Duration::from_secs(30),
        initial_inactivity: Duration::from_secs(60),
    };
}

/// O(1) bookkeeping for the ordered, untagged `SyncDone` responses on THIS socket.
/// A repair may queue behind existing gap/join work, but only one repair is ever
/// in flight. Earlier/later partial completions cannot impersonate its completion.
pub struct CursorRepair<T> {
    policy: CursorRepairPolicy,
    pending: usize,
    repair_position: Option<usize>,
    due: Option<T>,
    deadline: Option<T>,
    initial_deadline: Option<T>,
}
impl<T: Copy + Ord + std::ops::Add<Duration, Output = T>> CursorRepair<T> {
    pub fn new(policy: CursorRepairPolicy, now: T) -> Self {
        Self {
            policy,
            pending: 1,
            repair_position: None,
            due: None,
            deadline: None,
            initial_deadline: Some(now + policy.initial_inactivity),
        }
    }
    /// Call only after Applied/Confirmed AND a durable `next_seq` increase,
    /// measured under the same engine lock. The wire has no per-Space replay
    /// frontier: genuine advancement in any Space renews stream inactivity.
    /// Never revive an already-expired owner while its watchdog is also ready.
    pub fn progressed(&mut self, now: T) -> bool {
        let Some(until) = self.initial_deadline.filter(|until| now < *until) else {
            return false;
        };
        let next = now + self.policy.initial_inactivity;
        self.initial_deadline = Some(next);
        next != until
    }
    pub fn submitted(&mut self) -> bool {
        let Some(next) = self.pending.checked_add(1) else {
            return false;
        };
        self.pending = next;
        true
    }
    pub fn begin(&mut self, now: T) -> bool {
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
    pub fn completed(&mut self, now: T) {
        if self.pending == 0 {
            return;
        }
        self.pending -= 1;
        // The first ordered completion belongs to the initial full Sync.
        self.initial_deadline = None;
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
    pub fn admission(&self) -> Option<T> {
        self.due
    }
    pub fn abort_deadline(&self) -> Option<T> {
        self.initial_deadline
            .or(self.deadline)
            .or_else(|| self.due.map(|due| due + self.policy.timeout))
    }
    pub fn in_flight(&self) -> bool {
        self.repair_position.is_some()
    }
    pub fn abandon_transport(&self, expired: bool, now: T) -> bool {
        // A ready work future may win the fair select over an already-ready
        // watchdog. Never enter another unbounded close after the same budget.
        expired
            || self.initial_deadline.is_some()
            || self.in_flight()
            || self.abort_deadline().is_some_and(|until| now >= until)
    }
}
