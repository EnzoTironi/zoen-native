//! Space → partition ownership (ADR 0018 / S4).
//!
//! A Space has at most one owner among the live sync nodes. A Space maps to a
//! partition by sha256-mod (O(1)); partitions map to nodes by rendezvous so
//! adding or removing a node moves about 1/N of partitions. The owner holds a lease with a
//! monotonically increasing fencing token; an append that carries a stale
//! token is refused. Ordering still comes from FoundationDB's conflict on the
//! Space's head key: two owners racing produce one conflict and one retry,
//! never a fork. The lease buys batching, membership caches and outbox
//! forwarding affinity.

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

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

/// In-process lease table for a single sync node (and for tests). Production
/// stores the same shape under FDB key `lease/{partition}` once NATS is in
/// (S5); until then one process is every partition's owner.
#[derive(Default)]
pub struct LeaseTable {
    /// partition → (owner, token, expiry_ms)
    leases: BTreeMap<u32, (String, u64, i64)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lease {
    pub partition: u32,
    pub owner: String,
    pub token: u64,
    pub expires_at_ms: i64,
}

impl LeaseTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Claims or renews `partition` for `owner`. A held lease belonging to
    /// someone else that has not expired is left alone (`None`). Otherwise the
    /// token goes up by one and the caller is the new owner.
    pub fn claim(
        &mut self,
        partition: u32,
        owner: &str,
        now_ms: i64,
        ttl_ms: i64,
    ) -> Option<Lease> {
        let expires = now_ms.saturating_add(ttl_ms.max(1));
        match self.leases.get(&partition) {
            Some((who, token, until)) if who != owner && *until > now_ms => None,
            Some((_, token, _)) => {
                let token = token.saturating_add(1);
                let lease = Lease {
                    partition,
                    owner: owner.to_string(),
                    token,
                    expires_at_ms: expires,
                };
                self.leases.insert(
                    partition,
                    (lease.owner.clone(), lease.token, lease.expires_at_ms),
                );
                Some(lease)
            }
            None => {
                let lease = Lease {
                    partition,
                    owner: owner.to_string(),
                    token: 1,
                    expires_at_ms: expires,
                };
                self.leases.insert(
                    partition,
                    (lease.owner.clone(), lease.token, lease.expires_at_ms),
                );
                Some(lease)
            }
        }
    }

    /// True when `owner` currently holds `partition` with exactly `token` and
    /// the lease has not expired. The fencing check every append (and every
    /// outbox clear) must pass.
    pub fn accepts(&self, partition: u32, owner: &str, token: u64, now_ms: i64) -> bool {
        matches!(
            self.leases.get(&partition),
            Some((who, t, until)) if who == owner && *t == token && *until > now_ms
        )
    }

    pub fn get(&self, partition: u32) -> Option<Lease> {
        self.leases
            .get(&partition)
            .map(|(owner, token, until)| Lease {
                partition,
                owner: owner.clone(),
                token: *token,
                expires_at_ms: *until,
            })
    }
}

/// This sync node's claimed partitions. Today it claims all of them at boot; with
/// several nodes (S5) it claims `owned_partitions(node, live)` and renews.
pub struct NodeOwner {
    pub node: String,
    leases: std::sync::Mutex<LeaseTable>,
    /// partition → fencing token currently held
    tokens: std::sync::Mutex<BTreeMap<u32, u64>>,
}

impl NodeOwner {
    pub fn claim_all(node: &str) -> Self {
        let mut leases = LeaseTable::new();
        let mut tokens = BTreeMap::new();
        let now = now_ms();
        // A year: single-node never expires; multi-node renewals land with S5.
        let ttl = 365 * 24 * 3_600 * 1_000i64;
        for p in 0..PARTITION_COUNT {
            let lease = leases.claim(p, node, now, ttl).expect("empty table");
            tokens.insert(p, lease.token);
        }
        Self {
            node: node.to_string(),
            leases: std::sync::Mutex::new(leases),
            tokens: std::sync::Mutex::new(tokens),
        }
    }

    /// Whether this node may act on `space` right now (holds a live lease for its
    /// partition with the token we claimed).
    pub fn may_append(&self, space: &str) -> bool {
        let p = partition_of(space);
        let token = {
            let t = self.tokens.lock().unwrap_or_else(|e| e.into_inner());
            match t.get(&p).copied() {
                Some(tok) => tok,
                None => return false,
            }
        };
        self.leases
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .accepts(p, &self.node, token, now_ms())
    }

    pub fn partition_of(space: &str) -> u32 {
        partition_of(space)
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
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

    #[test]
    fn fencing_token_refuses_a_stale_owner() {
        let mut t = LeaseTable::new();
        let first = t.claim(7, "alpha", 1_000, 5_000).unwrap();
        assert!(t.accepts(7, "alpha", first.token, 2_000));
        assert!(t.claim(7, "beta", 2_000, 5_000).is_none());
        // Lease expired: beta takes over with a higher token.
        let second = t.claim(7, "beta", 7_000, 5_000).unwrap();
        assert!(second.token > first.token);
        assert!(!t.accepts(7, "alpha", first.token, 7_500));
        assert!(t.accepts(7, "beta", second.token, 7_500));
        // Alpha comes back with the old token: refused.
        assert!(!t.accepts(7, "alpha", first.token, 8_000));
    }

    #[test]
    fn renew_keeps_the_same_owner_and_bumps_the_token() {
        let mut t = LeaseTable::new();
        let a = t.claim(1, "alpha", 0, 1_000).unwrap();
        let b = t.claim(1, "alpha", 500, 1_000).unwrap();
        assert_eq!(b.token, a.token + 1);
        assert!(!t.accepts(1, "alpha", a.token, 600));
        assert!(t.accepts(1, "alpha", b.token, 600));
    }
}
