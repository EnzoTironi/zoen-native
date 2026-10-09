//! Abuse controls (ADR 0020, S6): GCRA token buckets keyed per device, per account and per
//! client IP. GCRA keeps one timestamp per key (the theoretical arrival time), so a bucket is
//! 8 bytes plus its key, refills continuously, and charges weighted costs (blob bytes) exactly.
//!
//! Buckets live on the node and outlive sessions: reconnecting does not refill them.
//! Defaults are below; `ZOEN_LIMITS="publish_device=20/s:60,register_ip=5/h:10"` overrides
//! any of them without a rebuild.

use std::{
    collections::HashMap,
    hash::{BuildHasher, RandomState},
    sync::{
        atomic::{AtomicU64, Ordering::Relaxed},
        Mutex,
    },
    time::{Duration, Instant},
};

const SHARDS: usize = 64;
const SWEEP_AT: usize = 50_000;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quota {
    /// Time one unit of cost takes to refill.
    pub interval: Duration,
    /// How many units can be spent at once from a full bucket.
    pub burst: u32,
}

impl Quota {
    pub const fn per_second(rate: u32, burst: u32) -> Self {
        Self::per(Duration::from_secs(1), rate, burst)
    }
    pub const fn per_minute(rate: u32, burst: u32) -> Self {
        Self::per(Duration::from_secs(60), rate, burst)
    }
    pub const fn per_hour(rate: u32, burst: u32) -> Self {
        Self::per(Duration::from_secs(3_600), rate, burst)
    }
    const fn per(period: Duration, rate: u32, burst: u32) -> Self {
        Quota {
            interval: Duration::from_nanos(period.as_nanos() as u64 / rate as u64),
            burst,
        }
    }

    /// `20/s:60`, `5/m:10`, `5/h:10`.
    pub fn parse(s: &str) -> Option<Self> {
        let (rate, rest) = s.split_once('/')?;
        let (unit, burst) = rest.split_once(':')?;
        let rate: u32 = rate.trim().parse().ok().filter(|r| *r > 0)?;
        let burst: u32 = burst.trim().parse().ok().filter(|b| *b > 0)?;
        Some(match unit.trim() {
            "s" => Quota::per_second(rate, burst),
            "m" => Quota::per_minute(rate, burst),
            "h" => Quota::per_hour(rate, burst),
            _ => return None,
        })
    }
}

pub struct Limiter {
    pub scope: &'static str,
    quota: Quota,
    epoch: Instant,
    hasher: RandomState,
    shards: Vec<Mutex<HashMap<String, u64>>>,
    sweep_at: usize,
    refused: AtomicU64,
}

impl Limiter {
    pub fn new(scope: &'static str, quota: Quota) -> Self {
        Self {
            scope,
            quota,
            epoch: Instant::now(),
            hasher: RandomState::new(),
            shards: (0..SHARDS).map(|_| Mutex::new(HashMap::new())).collect(),
            sweep_at: SWEEP_AT,
            refused: AtomicU64::new(0),
        }
    }

    pub fn quota(&self) -> Quota {
        self.quota
    }

    /// Spends one unit for `key`.
    pub fn check(&self, key: &str) -> Result<(), Duration> {
        self.check_n(key, 1)
    }

    /// Spends `cost` units for `key`, or says how long until it could.
    pub fn check_n(&self, key: &str, cost: u32) -> Result<(), Duration> {
        self.check_at(key, cost, self.epoch.elapsed())
    }

    fn check_at(&self, key: &str, cost: u32, now: Duration) -> Result<(), Duration> {
        let now = now.as_nanos() as u64;
        let t = self.quota.interval.as_nanos() as u64;
        let window = t.saturating_mul(self.quota.burst as u64);
        let shard = &self.shards[(self.hasher.hash_one(key) as usize) % SHARDS];
        let mut map = shard.lock().unwrap_or_else(|e| e.into_inner());
        let tat = map.get(key).copied().unwrap_or(now).max(now);
        let next = tat.saturating_add(t.saturating_mul(cost as u64));
        if next - now > window {
            self.refused.fetch_add(1, Relaxed);
            return Err(Duration::from_nanos(next - now - window));
        }
        if map.len() >= self.sweep_at && !map.contains_key(key) {
            map.retain(|_, v| *v > now);
        }
        map.insert(key.to_string(), next);
        Ok(())
    }

    pub fn refused(&self) -> u64 {
        self.refused.load(Relaxed)
    }
}

/// Every bucket the relay enforces.
pub struct Limits {
    /// WebSocket handshakes per client IP (each costs two signature checks).
    pub connect_ip: Limiter,
    /// New accounts per client IP.
    pub register_ip: Limiter,
    /// Events per device, and per account across all of its devices.
    pub publish_device: Limiter,
    pub publish_account: Limiter,
    /// Typing, status and read signals per device.
    pub ephemeral_device: Limiter,
    /// Request/response ops per device.
    pub request_device: Limiter,
    /// Handle lookups per account (directory enumeration).
    pub lookup_account: Limiter,
    /// Invite previews per account (code guessing).
    pub invite_account: Limiter,
    /// Uploaded blob kilobytes per device.
    pub blob_kib_device: Limiter,
    /// Backup writes per device (ADR 0045).
    pub backup_device: Limiter,
    /// Restore requests per client IP and per handle (password guessing).
    pub backup_restore_ip: Limiter,
    pub backup_restore_handle: Limiter,
}

impl Limits {
    pub fn defaults() -> Self {
        Self {
            connect_ip: Limiter::new("connect_ip", Quota::per_minute(60, 60)),
            register_ip: Limiter::new("register_ip", Quota::per_hour(10, 10)),
            publish_device: Limiter::new("publish_device", Quota::per_second(20, 60)),
            publish_account: Limiter::new("publish_account", Quota::per_second(40, 120)),
            ephemeral_device: Limiter::new("ephemeral_device", Quota::per_second(10, 30)),
            request_device: Limiter::new("request_device", Quota::per_second(50, 200)),
            lookup_account: Limiter::new("lookup_account", Quota::per_minute(60, 60)),
            invite_account: Limiter::new("invite_account", Quota::per_minute(30, 30)),
            blob_kib_device: Limiter::new(
                "blob_kib_device",
                Quota::per_minute(100 * 1024, 200 * 1024),
            ),
            backup_device: Limiter::new("backup_device", Quota::per_minute(10, 20)),
            backup_restore_ip: Limiter::new("backup_restore_ip", Quota::per_hour(60, 60)),
            backup_restore_handle: Limiter::new("backup_restore_handle", Quota::per_hour(30, 30)),
        }
    }

    /// Defaults with `spec` (`scope=rate/unit:burst,…`) applied on top. Unknown scopes and
    /// malformed quotas are errors, so a typo never silently leaves a limit at its default.
    pub fn from_spec(spec: &str) -> anyhow::Result<Self> {
        let mut l = Self::defaults();
        for item in spec.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let (scope, q) = item.split_once('=').ok_or_else(|| {
                anyhow::anyhow!("ZOEN_LIMITS: expected scope=rate/unit:burst, got {item}")
            })?;
            let quota = Quota::parse(q)
                .ok_or_else(|| anyhow::anyhow!("ZOEN_LIMITS: bad quota {q} for {scope}"))?;
            let slot = l
                .all_mut()
                .into_iter()
                .find(|x| x.scope == scope.trim())
                .ok_or_else(|| anyhow::anyhow!("ZOEN_LIMITS: unknown scope {scope}"))?;
            *slot = Limiter::new(slot.scope, quota);
        }
        Ok(l)
    }

    fn all_mut(&mut self) -> Vec<&mut Limiter> {
        vec![
            &mut self.connect_ip,
            &mut self.register_ip,
            &mut self.publish_device,
            &mut self.publish_account,
            &mut self.ephemeral_device,
            &mut self.request_device,
            &mut self.lookup_account,
            &mut self.invite_account,
            &mut self.blob_kib_device,
            &mut self.backup_device,
            &mut self.backup_restore_ip,
            &mut self.backup_restore_handle,
        ]
    }

    fn all(&self) -> [&Limiter; 12] {
        [
            &self.connect_ip,
            &self.register_ip,
            &self.publish_device,
            &self.publish_account,
            &self.ephemeral_device,
            &self.request_device,
            &self.lookup_account,
            &self.invite_account,
            &self.blob_kib_device,
            &self.backup_device,
            &self.backup_restore_ip,
            &self.backup_restore_handle,
        ]
    }

    pub fn render_metrics(&self) -> String {
        let name = "zoen_relay_rate_limited_total";
        let mut s =
            format!("# HELP {name} Actions refused by a rate limit\n# TYPE {name} counter\n");
        for l in self.all() {
            s.push_str(&format!(
                "{name}{{scope=\"{}\"}} {}\n",
                l.scope,
                l.refused()
            ));
        }
        s
    }
}

pub use roda_proto::slow_down;

#[cfg(test)]
mod tests {
    use super::*;

    fn at(ms: u64) -> Duration {
        Duration::from_millis(ms)
    }

    #[test]
    fn a_full_bucket_spends_its_burst_then_refills_at_the_rate() {
        let l = Limiter::new("t", Quota::per_second(10, 5));
        for _ in 0..5 {
            assert!(l.check_at("k", 1, at(0)).is_ok());
        }
        let wait = l.check_at("k", 1, at(0)).unwrap_err();
        assert_eq!(wait, at(100));
        assert!(l.check_at("k", 1, at(99)).is_err());
        assert!(l.check_at("k", 1, at(100)).is_ok());
        assert!(l.check_at("other", 1, at(100)).is_ok());
        assert_eq!(l.refused(), 2);
    }

    #[test]
    fn weighted_costs_charge_exactly() {
        let l = Limiter::new("t", Quota::per_second(1000, 1000));
        assert!(l.check_at("dev", 600, at(0)).is_ok());
        assert!(l.check_at("dev", 600, at(0)).is_err());
        assert!(l.check_at("dev", 600, at(200)).is_ok());
    }

    #[test]
    fn idle_keys_are_swept_when_a_shard_grows() {
        let mut l = Limiter::new("t", Quota::per_second(1000, 10));
        l.sweep_at = 4;
        let total =
            |l: &Limiter| -> usize { l.shards.iter().map(|s| s.lock().unwrap().len()).sum() };
        for i in 0..2_000 {
            let _ = l.check_at(&format!("old{i}"), 1, at(0));
        }
        assert!(total(&l) >= 2_000 - SHARDS * 4);
        for i in 0..2_000 {
            let _ = l.check_at(&format!("new{i}"), 1, at(10_000));
        }
        assert!(total(&l) <= 2_000 + SHARDS * 4, "{}", total(&l));
    }

    #[test]
    fn specs_override_defaults_and_refuse_typos() {
        let l = Limits::from_spec("publish_device=2/s:3, register_ip=1/h:1").unwrap();
        assert_eq!(l.publish_device.quota(), Quota::per_second(2, 3));
        assert_eq!(l.register_ip.quota(), Quota::per_hour(1, 1));
        assert_eq!(
            l.lookup_account.quota(),
            Limits::defaults().lookup_account.quota()
        );
        assert!(Limits::from_spec("publish_devise=2/s:3").is_err());
        assert!(Limits::from_spec("publish_device=2/x:3").is_err());
        assert!(Limits::from_spec("publish_device=0/s:3").is_err());
    }
}
