//! Ephemeral complete-read resources, never an authority or durable cursor store.
//! Four dedicated connections cannot exhaust the ordinary/admission/delivery pools.
use crate::Shared;
use protocol::complete_reads::{self as reads, Header, Kind, Page, Request};
use sqlx::{postgres::PgPoolOptions, PgPool, Postgres, Transaction};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

mod queries;

pub const MAX_SNAPSHOTS: usize = 4;
pub const MAX_AGE: Duration = Duration::from_secs(30);
static GLOBAL_SNAPSHOTS: std::sync::OnceLock<Arc<Semaphore>> = std::sync::OnceLock::new();
pub struct Resources {
    pool: PgPool,
    slots: Arc<Semaphore>,
}
impl Resources {
    pub async fn close(&self) {
        self.pool.close().await;
    }
    pub fn new(pool: &PgPool) -> Self {
        let pool = PgPoolOptions::new()
            .min_connections(0)
            .max_connections(MAX_SNAPSHOTS as u32)
            .acquire_timeout(Duration::from_secs(1))
            .connect_lazy_with((*pool.connect_options()).clone());
        Self {
            pool,
            slots: GLOBAL_SNAPSHOTS
                .get_or_init(|| Arc::new(Semaphore::new(MAX_SNAPSHOTS)))
                .clone(),
        }
    }
}

pub(crate) struct Snapshot {
    tx: Transaction<'static, Postgres>,
    _permit: OwnedSemaphorePermit,
    kind: Kind,
    token: String,
    identity: String,
    device: String,
    header: Option<Header>,
    owner: Option<String>,
    last: String,
    ordinal: u32,
    count: usize,
    bytes: usize,
    digest: Vec<u8>,
    started: Instant,
}
impl Snapshot {
    pub(crate) fn expired(&self) -> bool {
        self.started.elapsed() >= MAX_AGE
    }
    pub(crate) fn matches(&self, request: &Request) -> bool {
        request.kind == self.kind as i32
            && request.snapshot == self.token
            && request.ordinal == self.ordinal
    }
    pub(crate) async fn begin(
        st: &Shared,
        identity: &str,
        device: &str,
        kind: Kind,
    ) -> Result<Self, String> {
        let permit = st
            .read_snapshots
            .slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| "complete_read_busy")?;
        // The FDB membership version precedes the paired PG snapshot. This is NOT
        // an atomic cross-store snapshot, nor current permission to execute tools.
        let version = if kind == Kind::AgentState {
            Some(
                st.log
                    .membership_read_version()
                    .await
                    .map_err(|e| e.to_string())?,
            )
        } else {
            None
        };
        let mut tx = st
            .read_snapshots
            .pool
            .begin()
            .await
            .map_err(|e| e.to_string())?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        // Server-side safety if a client abandons an active transaction. Session
        // drop/cancel/age also release it; rollback is SQLx Transaction's RAII path.
        sqlx::query("SET LOCAL idle_in_transaction_session_timeout = '35s'")
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        sqlx::query("SET LOCAL statement_timeout = '4s'")
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        if let Some(version) = version {
            queries::membership(&mut tx, st, identity, version).await?;
        }
        let (header, owner) = if kind == Kind::AgentState {
            let (header, owner, claimed) = queries::header_in(&mut *tx, identity).await?;
            if claimed != device || header.status == "revoked" {
                return Err("agent device changed".into());
            }
            (Some(header), Some(owner))
        } else {
            (None, None)
        };
        let mut token = [0_u8; 32];
        getrandom::getrandom(&mut token).map_err(|_| "snapshot entropy unavailable")?;
        Ok(Self {
            tx,
            _permit: permit,
            kind,
            token: hex::encode(token),
            identity: identity.into(),
            device: device.into(),
            header,
            owner,
            last: String::new(),
            ordinal: 0,
            count: 0,
            bytes: 0,
            digest: Vec::new(),
            started: Instant::now(),
        })
    }
    pub(crate) async fn next(&mut self, st: &Shared) -> Result<Page, String> {
        if self.expired() {
            return Err("complete_read_expired".into());
        }
        // Session admission and the actual socket writer independently recheck the
        // current certified device on EVERY page. Also fence current runtime claim,
        // owner/status/definition against this retained snapshot's original header.
        if self.kind == Kind::AgentState {
            let (header, owner, claimed) = queries::header_in(&st.pool, &self.identity).await?;
            if Some(&header) != self.header.as_ref()
                || Some(&owner) != self.owner.as_ref()
                || claimed != self.device
            {
                return Err("complete_read_stale".into());
            }
        }
        let mut p = Page {
            kind: self.kind as i32,
            snapshot: self.token.clone(),
            ordinal: self.ordinal,
            identity: self.identity.clone(),
            device: self.device.clone(),
            header: (self.ordinal == 0).then(|| self.header.clone()).flatten(),
            devices: Vec::new(),
            cursors: Vec::new(),
            complete: false,
            count: 0,
            digest: Vec::new(),
        };
        let n = match self.kind {
            Kind::Devices => {
                p.devices = queries::devices(&mut self.tx, &self.identity, &self.last).await?;
                p.devices.len()
            }
            Kind::AgentState => {
                p.cursors = queries::cursors(&mut self.tx, &self.identity, &self.last).await?;
                p.cursors.len()
            }
        };
        if let Some(key) = p
            .devices
            .last()
            .map(|d| &d.device)
            .or_else(|| p.cursors.last().map(|c| &c.space))
        {
            self.last = key.clone();
        }
        self.count = self.count.checked_add(n).ok_or(reads::PROFILE_EXCEEDED)?;
        if self.count > self.kind.max_rows() {
            return Err(reads::PROFILE_EXCEEDED.into());
        }
        p.count = u32::try_from(self.count).map_err(|_| reads::PROFILE_EXCEEDED)?;
        p.complete = n < reads::PAGE_ROWS;
        p.digest = p.rolling_digest(&self.digest);
        if !p.valid() {
            return Err(reads::PROFILE_EXCEEDED.into());
        }
        self.bytes = self
            .bytes
            .checked_add(p.wire_len())
            .ok_or(reads::PROFILE_EXCEEDED)?;
        if self.bytes > reads::TOTAL_BYTES {
            return Err(reads::PROFILE_EXCEEDED.into());
        }
        self.ordinal = self.ordinal.checked_add(1).ok_or(reads::PROFILE_EXCEEDED)?;
        self.digest = p.digest.clone();
        Ok(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn leases_are_process_global_even_between_independent_app_state_pools() {
        let pool = PgPoolOptions::new()
            .connect_lazy("postgres://unused:unused@127.0.0.1:1/complete_read_limits")
            .unwrap();
        let first = Resources::new(&pool);
        let second = Resources::new(&pool);
        assert!(Arc::ptr_eq(&first.slots, &second.slots));
        let mut held = Vec::new();
        for _ in 0..MAX_SNAPSHOTS {
            held.push(first.slots.clone().try_acquire_owned().unwrap());
        }
        assert!(second.slots.clone().try_acquire_owned().is_err());
        drop(held.pop());
        let replacement = second.slots.clone().try_acquire_owned().unwrap();
        assert!(first.slots.clone().try_acquire_owned().is_err());
        drop(replacement);
        drop(held);
        assert_eq!(first.slots.available_permits(), MAX_SNAPSHOTS);
        first.close().await;
        second.close().await;
        pool.close().await;
    }
}
