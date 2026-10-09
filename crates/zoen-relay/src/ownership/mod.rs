//! Persisted cell ownership, checked inside the log transaction (ADR 0018).

pub mod forward;
pub mod registry;

use foundationdb::{
    options::TransactionOption,
    tuple::{pack, unpack, Subspace},
    Database, FdbBindingError, Transaction,
};
use futures_util::future::try_join_all;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

/// How many partitions the cell is carved into. Power of two so a later
/// split-by-bit is a clean remap of half the Spaces.
pub const PARTITION_COUNT: u32 = 4096;

/// Space → partition. One sha256, first four bytes as big-endian u32 mod n.
/// O(1) on the publish hot path. Adding a power-of-two split remaps half the
/// Spaces by bit; a non-power change remaps freely (rendezvous is reserved
/// for the much smaller live-node set below).
pub fn partition_of(space: &str) -> u32 {
    partition_of_n(space, PARTITION_COUNT)
}

pub fn partition_of_n(space: &str, n: u32) -> u32 {
    assert!(n > 0);
    let dig = Sha256::digest(space.as_bytes());
    let v = u32::from_be_bytes([dig[0], dig[1], dig[2], dig[3]]);
    v % n
}

/// Which partitions this node owns given the live set of node ids. Every
/// partition is assigned to exactly one live node by the same rendezvous
/// score over `(node, partition)`.
pub fn owned_partitions(node: &str, live: &[String]) -> Vec<u32> {
    assert!(!live.is_empty());
    assert!(live.iter().any(|n| n == node));
    (0..PARTITION_COUNT)
        .filter(|&p| owner_of_partition(p, live) == node)
        .collect()
}

fn owner_of_partition(partition: u32, live: &[String]) -> &str {
    let mut best: Option<(&str, [u8; 32])> = None;
    for n in live {
        let mut h = Sha256::new();
        h.update(partition.to_be_bytes());
        h.update([0]);
        h.update(n.as_bytes());
        let dig: [u8; 32] = h.finalize().into();
        if best.as_ref().is_none_or(|(_, b)| dig > *b) {
            best = Some((n.as_str(), dig));
        }
    }
    best.expect("live non-empty").0
}

/// A generation changes on takeover or reacquisition after expiry, not on renewal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fence {
    pub partition: u32,
    pub owner: String,
    pub token: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lease {
    pub fence: Fence,
    pub expires_version: i64,
}

/// FDB versions normally advance at 1,000,000/s. This is a liveness setting,
/// not a wall-clock promise. Database recovery can advance versions faster.
pub const VERSIONS_PER_MS: i64 = 1_000;
pub const MAX_NODES: usize = 4096;
pub const STORE_TIMEOUT_MS: i32 = 2_000;

pub fn bound_transaction(trx: &Transaction) -> Result<(), FdbBindingError> {
    trx.set_option(TransactionOption::Timeout(STORE_TIMEOUT_MS))?;
    trx.set_option(TransactionOption::RetryLimit(5))?;
    Ok(())
}

fn corrupt(reason: impl Into<String>) -> FdbBindingError {
    FdbBindingError::CustomError(Box::new(crate::log::StoreError(reason.into())))
}

pub fn lease_key(root: &Subspace, partition: u32) -> Vec<u8> {
    root.pack(&("ownership", "lease", partition as i64))
}

/// Non-snapshot: a renewal or takeover invalidates a concurrent mutation's read.
pub async fn lease_in(
    trx: &Transaction,
    root: &Subspace,
    partition: u32,
) -> Result<Option<Lease>, FdbBindingError> {
    trx.get(&lease_key(root, partition), false)
        .await?
        .map(|v| {
            let (owner, token, expires_version): (String, i64, i64) =
                unpack(&v).map_err(|e| corrupt(e.to_string()))?;
            Ok(Lease {
                fence: Fence {
                    partition,
                    owner,
                    token,
                },
                expires_version,
            })
        })
        .transpose()
}

pub fn accepts(lease: Option<&Lease>, fence: &Fence, space: &str, version: i64) -> bool {
    fence.partition == partition_of(space)
        && lease.is_some_and(|l| l.fence == *fence && l.expires_version > version)
}

/// Placement is advisory. Only the persisted lease and the mutation's conflict
/// read grant authority. A disconnected node cannot renew or commit to the cell.
pub struct NodeOwner {
    pub node: String,
    db: Arc<Database>,
    root: Subspace,
    ttl_versions: i64,
    interval: Duration,
    maintained: Mutex<Option<Instant>>,
}

impl NodeOwner {
    pub fn new(db: Arc<Database>, root: Subspace, node: String, ttl: Duration) -> Self {
        assert!(!node.is_empty());
        assert!(ttl >= Duration::from_millis(100));
        Self {
            node,
            db,
            root,
            ttl_versions: i64::try_from(ttl.as_millis()).expect("lease duration") * VERSIONS_PER_MS,
            interval: ttl / 3,
            maintained: Mutex::new(None),
        }
    }

    pub fn ttl_from_env() -> anyhow::Result<Duration> {
        let ms: u64 = std::env::var("ZOEN_OWNER_LEASE_MS")
            .unwrap_or_else(|_| "15000".into())
            .parse()?;
        anyhow::ensure!(
            (1000..=60000).contains(&ms),
            "ZOEN_OWNER_LEASE_MS must be 1000..=60000"
        );
        Ok(Duration::from_millis(ms))
    }

    /// Heartbeat, discover the live set, and renew/claim the preferred partitions.
    /// Undesired partitions stop renewing and hand over only after expiry.
    pub async fn maintain(&self) -> anyhow::Result<()> {
        let mut last = self.maintained.lock().await;
        self.maintain_in_store().await?;
        *last = Some(Instant::now());
        Ok(())
    }

    async fn maintain_in_store(&self) -> anyhow::Result<()> {
        // Commit cleanup independently of admission. One retry handles an expired
        // overflow page; larger legacy registries continue their sweep next time.
        for _ in 0..2 {
            let result = self
                .db
                .run(|trx, _| async move {
                    bound_transaction(&trx)?;
                    let version = trx.get_read_version().await?;
                    let until = version
                        .checked_add(self.ttl_versions)
                        .ok_or_else(|| corrupt("lease expiry overflow"))?;
                    let Some(live) =
                        registry::register_in(&trx, &self.root, &self.node, version, until).await?
                    else {
                        return Ok(None);
                    };
                    let partitions = owned_partitions(&self.node, &live);
                    let leases =
                        try_join_all(partitions.iter().map(|&p| lease_in(&trx, &self.root, p)))
                            .await?;
                    let mut renewals = 0usize;
                    let mut claims = 0usize;
                    for (p, old) in partitions.into_iter().zip(leases) {
                        let token = match old.as_ref() {
                            Some(l)
                                if l.expires_version > version && l.fence.owner != self.node =>
                            {
                                continue
                            }
                            Some(l) if l.expires_version > version => l.fence.token,
                            Some(l) => l
                                .fence
                                .token
                                .checked_add(1)
                                .ok_or_else(|| corrupt("fencing token overflow"))?,
                            None => 1,
                        };
                        if old.as_ref().is_some_and(|l| {
                            l.expires_version > version && l.fence.owner == self.node
                        }) {
                            renewals += 1;
                        } else {
                            claims += 1;
                        }
                        trx.set(
                            &lease_key(&self.root, p),
                            &pack(&(self.node.as_str(), token, until)),
                        );
                    }
                    Ok(Some((version, renewals, claims)))
                })
                .await?;
            if let Some((version, renewals, claims)) = result {
                tracing::debug!(node = %self.node, version, renewals, claims, "ownership maintenance committed");
                return Ok(());
            }
        }
        anyhow::bail!("ownership registry limit reached; bounded cleanup committed")
    }

    async fn maintain_if_due(
        &self,
        force: bool,
        can_renew: &(impl Fn() -> bool + Sync),
    ) -> anyhow::Result<()> {
        let mut last = self.maintained.lock().await;
        anyhow::ensure!(can_renew(), "cell forwarding unavailable");
        if force || last.is_none_or(|t| t.elapsed() >= self.interval) {
            self.maintain_in_store().await?;
            *last = Some(Instant::now());
        }
        Ok(())
    }

    pub async fn resolve(
        &self,
        space: &str,
        can_renew: impl Fn() -> bool + Sync,
    ) -> Result<Fence, crate::log::Reject> {
        self.maintain_if_due(false, &can_renew)
            .await
            .map_err(|_| crate::log::Reject::unavailable())?;
        for attempt in 0..2 {
            let lease = self
                .db
                .run(|trx, _| async move {
                    bound_transaction(&trx)?;
                    let version = trx.get_read_version().await?;
                    Ok(lease_in(&trx, &self.root, partition_of(space))
                        .await?
                        .filter(|l| l.expires_version > version))
                })
                .await
                .map_err(|_| crate::log::Reject::unavailable())?;
            if let Some(l) = lease {
                return Ok(l.fence);
            }
            if attempt == 0 {
                self.maintain_if_due(true, &can_renew)
                    .await
                    .map_err(|_| crate::log::Reject::unavailable())?;
            }
        }
        Err(crate::log::Reject::retry("partition ownership is changing"))
    }

    pub async fn ready(&self) -> bool {
        self.db
            .run(|trx, _| async move {
                bound_transaction(&trx)?;
                let version = trx.get_read_version().await?;
                let key = self.root.pack(&("ownership", "nodes", &self.node));
                Ok(trx
                    .get(&key, false)
                    .await?
                    .is_some_and(|v| unpack::<i64>(&v).is_ok_and(|expiry| expiry > version)))
            })
            .await
            .unwrap_or(false)
    }

    pub fn spawn(owner: &Arc<Self>, should_renew: impl Fn() -> bool + Send + Sync + 'static) {
        let weak = Arc::downgrade(owner);
        let interval = owner.interval;
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                let Some(owner) = weak.upgrade() else { break };
                if !should_renew() {
                    continue;
                }
                if let Err(e) = owner.maintain_if_due(false, &should_renew).await {
                    tracing::warn!(error = %e, "ownership renewal failed");
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_mod_is_stable_and_balanced() {
        assert_eq!(partition_of("sp_01jqabc"), partition_of("sp_01jqabc"));
        assert_ne!(partition_of("sp_a"), partition_of("sp_b"));
        let mut counts = [0u32; 16];
        for i in 0..16_000u32 {
            counts[partition_of_n(&format!("sp_{i}"), 16) as usize] += 1;
        }
        let avg = 16_000f64 / 16.0;
        for c in counts {
            assert!(
                (c as f64 - avg).abs() / avg < 0.15,
                "partition load {c} far from {avg}"
            );
        }
    }

    #[test]
    fn adding_a_node_moves_about_one_nth() {
        let a = vec!["alpha".into()];
        let ab = vec!["alpha".into(), "beta".into()];
        let before = owned_partitions("alpha", &a);
        assert_eq!(before.len(), PARTITION_COUNT as usize);
        let after_a = owned_partitions("alpha", &ab);
        let after_b = owned_partitions("beta", &ab);
        assert_eq!(after_a.len() + after_b.len(), PARTITION_COUNT as usize);
        let moved = before.iter().filter(|p| !after_a.contains(p)).count();
        let expected = PARTITION_COUNT as f64 / 2.0;
        assert!(
            (moved as f64 - expected).abs() / expected < 0.05,
            "moved {moved}, expected ~{expected}"
        );
    }
}
