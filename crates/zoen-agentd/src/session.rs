//! One tool's sandbox for one task: started lazily on the first call that needs it,
//! suspended after an idle period, torn down at the end (ADR 0028 §1). Running time is
//! charged to the owner's daily budget; suspended time is free.

use crate::budget::{BudgetError, DailyBudget, Usage};
use crate::sandbox::{
    ExecOutput, ExecRequest, Lease, SandboxError, SandboxProvider, SandboxSpec, SnapshotRef,
};
use roda_types::IdentityId;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error(transparent)]
    Budget(#[from] BudgetError),
    #[error(transparent)]
    Sandbox(#[from] SandboxError),
    #[error("the task is over; this sandbox is gone")]
    Finished,
}

enum State {
    /// Nothing started yet (and nothing paid).
    Cold,
    Running {
        lease: Lease,
        since: Instant,
        last_used: Instant,
    },
    Suspended {
        snap: SnapshotRef,
    },
    Done,
}

pub struct ToolSession {
    provider: Arc<dyn SandboxProvider>,
    spec: SandboxSpec,
    owner: IdentityId,
    idle: Duration,
    budget: Arc<Mutex<DailyBudget>>,
    state: tokio::sync::Mutex<State>,
    usage: Mutex<Vec<Usage>>,
}

/// Running time rounded up to the second; any running period costs at least one.
fn secs_ceil(d: Duration) -> u64 {
    (d.as_millis().div_ceil(1000) as u64).max(1)
}

impl ToolSession {
    pub fn new(
        provider: Arc<dyn SandboxProvider>,
        spec: SandboxSpec,
        owner: IdentityId,
        idle: Duration,
        budget: Arc<Mutex<DailyBudget>>,
    ) -> Self {
        ToolSession {
            provider,
            spec,
            owner,
            idle,
            budget,
            state: tokio::sync::Mutex::new(State::Cold),
            usage: Mutex::new(vec![]),
        }
    }

    /// "cold", "running", "suspended" or "done".
    pub async fn state(&self) -> &'static str {
        match &*self.state.lock().await {
            State::Cold => "cold",
            State::Running { .. } => "running",
            State::Suspended { .. } => "suspended",
            State::Done => "done",
        }
    }

    pub fn usage(&self) -> Vec<Usage> {
        self.usage.lock().unwrap().clone()
    }

    fn charge(&self, since: Instant, now_ms: i64) {
        let u =
            self.budget
                .lock()
                .unwrap()
                .charge(self.spec.tier, secs_ceil(since.elapsed()), now_ms);
        self.usage.lock().unwrap().push(u);
    }

    pub async fn exec(
        &self,
        mut req: ExecRequest,
        now_ms: i64,
    ) -> Result<ExecOutput, SessionError> {
        let mut st = self.state.lock().await;
        let running_uncharged = match &*st {
            State::Running { since, .. } => secs_ceil(since.elapsed()),
            State::Done => return Err(SessionError::Finished),
            _ => 0,
        };
        let left = self.budget.lock().unwrap().admit(self.spec.tier, now_ms)?;
        let left = left.saturating_sub(running_uncharged);
        if left == 0 {
            return Err(BudgetError::Exhausted {
                tier: self.spec.tier,
            }
            .into());
        }
        let lease = match std::mem::replace(&mut *st, State::Cold) {
            State::Cold => self.provider.acquire(&self.spec, &self.owner).await?,
            State::Suspended { snap } => self.provider.resume(&snap).await?,
            State::Running { lease, since, .. } => {
                *st = State::Running {
                    lease: lease.clone(),
                    since,
                    last_used: Instant::now(),
                };
                lease
            }
            State::Done => unreachable!(),
        };
        if let State::Cold = &*st {
            let now = Instant::now();
            *st = State::Running {
                lease: lease.clone(),
                since: now,
                last_used: now,
            };
        }
        req.timeout = req.timeout.min(Duration::from_secs(left));
        let out = self.provider.exec(&lease, req).await;
        if let State::Running { last_used, .. } = &mut *st {
            *last_used = Instant::now();
        }
        Ok(out?)
    }

    /// Suspends the sandbox if it has been idle long enough. Returns true if it did.
    pub async fn tick(&self, now_ms: i64) -> Result<bool, SessionError> {
        let mut st = self.state.lock().await;
        let State::Running {
            lease,
            since,
            last_used,
        } = &*st
        else {
            return Ok(false);
        };
        if last_used.elapsed() < self.idle {
            return Ok(false);
        }
        let (lease, since) = (lease.clone(), *since);
        self.charge(since, now_ms);
        match self.provider.suspend(&lease).await {
            Ok(snap) => *st = State::Suspended { snap },
            Err(SandboxError::Unsupported(_)) => {
                self.provider.release(lease).await?;
                *st = State::Cold;
            }
            Err(e) => return Err(e.into()),
        }
        Ok(true)
    }

    /// The task is over: charge what ran and delete everything.
    pub async fn finish(&self, now_ms: i64) -> Result<(), SessionError> {
        let mut st = self.state.lock().await;
        match std::mem::replace(&mut *st, State::Done) {
            State::Running { lease, since, .. } => {
                self.charge(since, now_ms);
                self.provider.release(lease).await?;
            }
            State::Suspended { snap } => {
                let lease = self.provider.resume(&snap).await?;
                self.provider.release(lease).await?;
            }
            State::Cold | State::Done => {}
        }
        Ok(())
    }
}
