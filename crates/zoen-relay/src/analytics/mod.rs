//! Product metrics without content (ADR 0043, `docs/product/metricas.md`).
//!
//! The hot path only bumps counters in memory: [`Analytics`] keeps one small entry per
//! account, Space and counter touched since the last flush, keyed by the raw id **in memory
//! only**. Every [`FLUSH_EVERY`] a task turns raw ids into keyed pseudonyms and adds the
//! batch to Postgres in a handful of `unnest` upserts, so the write cost is one row per
//! active account per day per node, not one per message. Rows that name an account or a
//! Space (by pseudonym) are deleted after [`RETENTION_DAYS`]; closed days are rolled up into
//! aggregate numbers kept for good ([`report`]).

pub mod admin;
pub mod clock;
pub mod export;
pub mod report;

use std::{
    collections::{HashMap, HashSet},
    sync::Mutex,
    time::Duration,
};

use sha2::{Digest, Sha256};
use sqlx::PgPool;

pub use clock::{day_of, now_ms, today};

/// How often a node writes what it counted.
pub const FLUSH_EVERY: Duration = Duration::from_secs(15);
/// Per-account and per-Space rows older than this are deleted (D30 retention needs 31).
pub const RETENTION_DAYS: i32 = 35;

/// Latency buckets (upper bounds, ms) for send latency and time to first sync.
pub const LATENCY_BUCKETS_MS: [u64; 12] = [
    5,
    10,
    25,
    50,
    100,
    250,
    500,
    1000,
    2500,
    5000,
    10000,
    u64::MAX,
];

#[derive(Default, Clone, Debug)]
struct Acc {
    sent: i32,
    syncs: i32,
    sessions: i32,
    peers: u64,
    first_message_ms: Option<i64>,
    invited: bool,
}

#[derive(Default, Clone, Debug)]
struct SpaceAcc {
    members: i32,
    messages: i32,
}

#[derive(Default, Debug)]
struct Pending {
    accounts: HashMap<(i32, String), Acc>,
    spaces: HashMap<(i32, String), SpaceAcc>,
    counters: HashMap<(i32, String), i64>,
}

impl Pending {
    fn is_empty(&self) -> bool {
        self.accounts.is_empty() && self.spaces.is_empty() && self.counters.is_empty()
    }

    fn merge(&mut self, other: Pending) {
        for (k, a) in other.accounts {
            let e = self.accounts.entry(k).or_default();
            e.sent += a.sent;
            e.syncs += a.syncs;
            e.sessions += a.sessions;
            e.peers |= a.peers;
            e.invited |= a.invited;
            e.first_message_ms = match (e.first_message_ms, a.first_message_ms) {
                (Some(x), Some(y)) => Some(x.min(y)),
                (x, y) => x.or(y),
            };
        }
        for (k, s) in other.spaces {
            let e = self.spaces.entry(k).or_default();
            e.members = e.members.max(s.members);
            e.messages += s.messages;
        }
        for (k, v) in other.counters {
            *self.counters.entry(k).or_default() += v;
        }
    }
}

/// What a message looked like to the relay (never its content).
pub struct SentMessage<'a> {
    pub author: &'a str,
    pub space: &'a str,
    /// Everyone the Space delivers to, the author included.
    pub audience: &'a [String],
}

pub struct Analytics {
    key: [u8; 32],
    pending: Mutex<Pending>,
    /// Handle prefixes of QA/test accounts, counted apart.
    test_prefixes: Vec<String>,
}

/// QA/test handle prefixes: `ZOEN_METRICS_TEST_HANDLES` (comma-separated), or these.
pub const DEFAULT_TEST_PREFIXES: &str = "qa_,test_,e2e_,ana_s,bruno_s,load_";

impl Analytics {
    /// Loads the pseudonym key: `ZOEN_METRICS_KEY`, or one generated once and kept in
    /// Postgres (dev and tests).
    pub async fn load(pool: &PgPool) -> anyhow::Result<Analytics> {
        let key: [u8; 32] = match std::env::var("ZOEN_METRICS_KEY") {
            Ok(k) if !k.is_empty() => Sha256::digest(k.as_bytes()).into(),
            _ => {
                let mut fresh = [0u8; 32];
                getrandom::getrandom(&mut fresh)?;
                sqlx::query("INSERT INTO metrics_key (key) VALUES ($1) ON CONFLICT DO NOTHING")
                    .bind(fresh.to_vec())
                    .execute(pool)
                    .await?;
                let stored: Vec<u8> = sqlx::query_scalar("SELECT key FROM metrics_key")
                    .fetch_one(pool)
                    .await?;
                stored
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("metrics_key isn't 32 bytes"))?
            }
        };
        let prefixes = std::env::var("ZOEN_METRICS_TEST_HANDLES")
            .unwrap_or_else(|_| DEFAULT_TEST_PREFIXES.into());
        Ok(Analytics::with_key(key, &prefixes))
    }

    pub fn with_key(key: [u8; 32], test_prefixes: &str) -> Analytics {
        Analytics {
            key,
            pending: Mutex::new(Pending::default()),
            test_prefixes: test_prefixes
                .split(',')
                .map(|s| s.trim().to_lowercase())
                .filter(|s| !s.is_empty())
                .collect(),
        }
    }

    /// 16 bytes of SHA-256(key ‖ tag ‖ id): stable while the key is, useless without it.
    pub fn pseudonym(&self, tag: &[u8], id: &str) -> Vec<u8> {
        Sha256::new()
            .chain_update(self.key)
            .chain_update(tag)
            .chain_update(id.as_bytes())
            .finalize()[..16]
            .to_vec()
    }

    /// The account's unit for experiments (ADR 0044): what devices bucket on, so they and
    /// the relay agree on a variant without the relay ever handing out the account id.
    pub fn unit(&self, identity: &str) -> String {
        hex::encode(self.pseudonym(b"unit\0", identity))
    }

    pub fn is_test_handle(&self, handle: &str) -> bool {
        let h = handle.to_lowercase();
        self.test_prefixes.iter().any(|p| h.starts_with(p.as_str()))
    }

    fn with<R>(&self, f: impl FnOnce(&mut Pending) -> R) -> R {
        f(&mut self.pending.lock().unwrap_or_else(|e| e.into_inner()))
    }

    fn peer_bit(&self, peer: &str) -> u64 {
        1u64 << (self.pseudonym(b"peer\0", peer)[0] & 63)
    }

    pub fn message_sent(&self, m: SentMessage<'_>) {
        let now = now_ms();
        let day = day_of(now);
        let peers = m
            .audience
            .iter()
            .filter(|p| p.as_str() != m.author)
            .fold(0u64, |acc, p| acc | self.peer_bit(p));
        self.with(|p| {
            let a = p.accounts.entry((day, m.author.to_owned())).or_default();
            a.sent += 1;
            a.peers |= peers;
            a.first_message_ms.get_or_insert(now);
            let s = p.spaces.entry((day, m.space.to_owned())).or_default();
            s.members = s.members.max(m.audience.len() as i32);
            s.messages += 1;
        });
    }

    pub fn synced(&self, identity: &str) {
        let day = today();
        self.with(|p| {
            p.accounts
                .entry((day, identity.to_owned()))
                .or_default()
                .syncs += 1
        });
    }

    pub fn session(&self, identity: &str) {
        let day = today();
        self.with(|p| {
            p.accounts
                .entry((day, identity.to_owned()))
                .or_default()
                .sessions += 1
        });
    }

    pub fn invite_accepted(&self, identity: &str) {
        let day = today();
        self.with(|p| {
            p.accounts
                .entry((day, identity.to_owned()))
                .or_default()
                .invited = true
        });
        self.count("invites_accepted", 1);
    }

    /// A global counter for today (`name` is one of a fixed set, never data).
    pub fn count(&self, name: &str, n: i64) {
        let day = today();
        self.with(|p| *p.counters.entry((day, name.to_owned())).or_default() += n);
    }

    /// One observation into a fixed latency histogram (`<name>_le_<bound>` counters).
    pub fn latency(&self, name: &str, elapsed: Duration) {
        let ms = elapsed.as_millis() as u64;
        let bound = LATENCY_BUCKETS_MS
            .iter()
            .find(|b| ms <= **b)
            .copied()
            .unwrap_or(u64::MAX);
        let label = if bound == u64::MAX {
            format!("{name}_le_inf")
        } else {
            format!("{name}_le_{bound}")
        };
        self.count(&label, 1);
    }

    /// Writes everything counted since the last flush. On failure the batch is kept and
    /// retried with the next one.
    pub async fn flush(&self, pool: &PgPool) -> anyhow::Result<()> {
        let batch = self.with(std::mem::take);
        if batch.is_empty() {
            return Ok(());
        }
        let r = self.write(pool, &batch).await;
        if r.is_err() {
            self.with(|p| p.merge(batch));
        }
        r
    }

    async fn write(&self, pool: &PgPool, b: &Pending) -> anyhow::Result<()> {
        let ids: Vec<String> = b
            .accounts
            .keys()
            .map(|(_, id)| id.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        // What the directory already knows, read in one query: kind, handle (for the test
        // flag only, never stored) and signup time.
        let rows: Vec<(String, String, Option<String>, i64)> = sqlx::query_as(
            "SELECT id, kind, handle, (extract(epoch FROM created_at) * 1000)::bigint
             FROM identities WHERE id = ANY($1)",
        )
        .bind(&ids)
        .fetch_all(pool)
        .await?;
        let info: HashMap<String, (i16, bool, i64)> = rows
            .into_iter()
            .map(|(id, kind, handle, created)| {
                let test = handle.as_deref().is_some_and(|h| self.is_test_handle(h));
                (id, (i16::from(kind == "Agent"), test, created))
            })
            .collect();

        let mut tx = pool.begin().await?;

        // Accounts young enough to matter for activation and cohorts.
        let horizon = now_ms() - i64::from(RETENTION_DAYS) * 86_400_000;
        let mut acc_rows: HashMap<String, (bool, Option<i64>)> = HashMap::new();
        for ((_, id), a) in &b.accounts {
            let e = acc_rows.entry(id.clone()).or_default();
            e.0 |= a.invited;
            e.1 = match (e.1, a.first_message_ms) {
                (Some(x), Some(y)) => Some(x.min(y)),
                (x, y) => x.or(y),
            };
        }
        let (mut a_pid, mut a_signup, mut a_kind, mut a_test, mut a_inv, mut a_first) =
            (vec![], vec![], vec![], vec![], vec![], vec![]);
        for (id, (invited, first)) in &acc_rows {
            let Some((kind, test, created)) = info.get(id) else {
                continue;
            };
            if *created < horizon {
                continue;
            }
            a_pid.push(self.pseudonym(b"acct\0", id));
            a_signup.push(*created);
            a_kind.push(*kind);
            a_test.push(*test);
            a_inv.push(*invited);
            a_first.push(*first);
        }
        if !a_pid.is_empty() {
            sqlx::query(
                "INSERT INTO metrics_accounts (pid, signup_at, kind, test, invited, first_message_at)
                 SELECT p, to_timestamp(s / 1000.0), k, t, i, to_timestamp(f / 1000.0)
                 FROM unnest($1::bytea[], $2::bigint[], $3::smallint[], $4::bool[], $5::bool[], $6::bigint[])
                   AS x(p, s, k, t, i, f)
                 ON CONFLICT (pid) DO UPDATE SET
                   invited = metrics_accounts.invited OR excluded.invited,
                   first_message_at = LEAST(metrics_accounts.first_message_at, excluded.first_message_at)",
            )
            .bind(&a_pid)
            .bind(&a_signup)
            .bind(&a_kind)
            .bind(&a_test)
            .bind(&a_inv)
            .bind(&a_first)
            .execute(&mut *tx)
            .await?;
        }

        let (mut day, mut pid, mut kind, mut test, mut cohort) =
            (vec![], vec![], vec![], vec![], vec![]);
        let (mut sent, mut syncs, mut sessions, mut peers) = (vec![], vec![], vec![], vec![]);
        for ((d, id), a) in &b.accounts {
            let Some((k, t, created)) = info.get(id) else {
                continue; // not registered (yet): nothing to count
            };
            day.push(*d);
            pid.push(self.pseudonym(b"acct\0", id));
            kind.push(*k);
            test.push(*t);
            cohort.push((*created >= horizon).then(|| day_of(*created)));
            sent.push(a.sent);
            syncs.push(a.syncs);
            sessions.push(a.sessions);
            peers.push(a.peers as i64);
        }
        if !day.is_empty() {
            sqlx::query(
                "INSERT INTO metrics_activity (day, pid, kind, test, cohort, sent, syncs, sessions, peers)
                 SELECT DATE '1970-01-01' + d, p, k, t, DATE '1970-01-01' + c, s, y, n, b
                 FROM unnest($1::int[], $2::bytea[], $3::smallint[], $4::bool[], $5::int[],
                             $6::int[], $7::int[], $8::int[], $9::bigint[]) AS x(d, p, k, t, c, s, y, n, b)
                 ON CONFLICT (day, pid) DO UPDATE SET
                   sent = metrics_activity.sent + excluded.sent,
                   syncs = metrics_activity.syncs + excluded.syncs,
                   sessions = metrics_activity.sessions + excluded.sessions,
                   peers = metrics_activity.peers | excluded.peers",
            )
            .bind(&day)
            .bind(&pid)
            .bind(&kind)
            .bind(&test)
            .bind(&cohort)
            .bind(&sent)
            .bind(&syncs)
            .bind(&sessions)
            .bind(&peers)
            .execute(&mut *tx)
            .await?;
        }

        if !b.spaces.is_empty() {
            let (mut d, mut s, mut m, mut n) = (vec![], vec![], vec![], vec![]);
            for ((day, space), a) in &b.spaces {
                d.push(*day);
                s.push(self.pseudonym(b"space\0", space));
                m.push(a.members);
                n.push(a.messages);
            }
            sqlx::query(
                "INSERT INTO metrics_spaces (day, sid, members, messages)
                 SELECT DATE '1970-01-01' + d, s, m, n
                 FROM unnest($1::int[], $2::bytea[], $3::int[], $4::int[]) AS x(d, s, m, n)
                 ON CONFLICT (day, sid) DO UPDATE SET
                   members = GREATEST(metrics_spaces.members, excluded.members),
                   messages = metrics_spaces.messages + excluded.messages",
            )
            .bind(&d)
            .bind(&s)
            .bind(&m)
            .bind(&n)
            .execute(&mut *tx)
            .await?;
        }

        if !b.counters.is_empty() {
            let (mut d, mut name, mut v) = (vec![], vec![], vec![]);
            for ((day, k), n) in &b.counters {
                d.push(*day);
                name.push(k.clone());
                v.push(*n);
            }
            sqlx::query(
                "INSERT INTO metrics_counters (day, name, value)
                 SELECT DATE '1970-01-01' + d, n, v
                 FROM unnest($1::int[], $2::text[], $3::bigint[]) AS x(d, n, v)
                 ON CONFLICT (day, name) DO UPDATE SET value = metrics_counters.value + excluded.value",
            )
            .bind(&d)
            .bind(&name)
            .bind(&v)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Deletes per-account and per-Space rows past the retention window.
    pub async fn expire(&self, pool: &PgPool) -> anyhow::Result<()> {
        let cutoff = today() - RETENTION_DAYS;
        sqlx::query("DELETE FROM metrics_activity WHERE day < DATE '1970-01-01' + $1")
            .bind(cutoff)
            .execute(pool)
            .await?;
        sqlx::query("DELETE FROM metrics_spaces WHERE day < DATE '1970-01-01' + $1")
            .bind(cutoff)
            .execute(pool)
            .await?;
        sqlx::query("DELETE FROM metrics_accounts WHERE signup_at < to_timestamp($1 / 1000.0)")
            .bind(now_ms() - i64::from(RETENTION_DAYS) * 86_400_000)
            .execute(pool)
            .await?;
        Ok(())
    }
}

/// Flushes on a timer, rolls closed days up and exports them, and expires old rows.
pub fn spawn(state: crate::Shared) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(FLUSH_EVERY);
        let mut last_maintenance = None::<std::time::Instant>;
        loop {
            tick.tick().await;
            if let Err(e) = state.analytics.flush(&state.pool).await {
                tracing::warn!(error = %e, "metrics flush failed; kept for the next one");
            }
            if last_maintenance.is_none_or(|t| t.elapsed() > Duration::from_secs(600)) {
                last_maintenance = Some(std::time::Instant::now());
                if let Err(e) = report::rollup(&state.pool).await {
                    tracing::warn!(error = %e, "metrics rollup failed");
                }
                if let Err(e) = state.analytics.expire(&state.pool).await {
                    tracing::warn!(error = %e, "metrics expiry failed");
                }
                export::run(&state.pool).await;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pseudonyms_are_keyed_and_tagged() {
        let a = Analytics::with_key([1; 32], "");
        let b = Analytics::with_key([2; 32], "");
        let id = "a3f9c2e17b4d5e6f";
        assert_eq!(a.pseudonym(b"acct\0", id), a.pseudonym(b"acct\0", id));
        assert_ne!(a.pseudonym(b"acct\0", id), b.pseudonym(b"acct\0", id));
        assert_ne!(a.pseudonym(b"acct\0", id), a.pseudonym(b"peer\0", id));
        assert_eq!(a.pseudonym(b"acct\0", id).len(), 16);
    }

    #[test]
    fn test_handles_by_prefix() {
        let a = Analytics::with_key([0; 32], "qa_, Bruno_s");
        assert!(a.is_test_handle("qa_ana"));
        assert!(a.is_test_handle("bruno_s123"));
        assert!(!a.is_test_handle("ana"));
    }

    #[test]
    fn counts_merge_when_a_flush_fails() {
        let a = Analytics::with_key([0; 32], "");
        let aud = vec!["x".to_string(), "y".to_string(), "z".to_string()];
        a.message_sent(SentMessage {
            author: "x",
            space: "s",
            audience: &aud,
        });
        let first = a.with(std::mem::take);
        a.message_sent(SentMessage {
            author: "x",
            space: "s",
            audience: &aud,
        });
        a.with(|p| p.merge(first));
        a.with(|p| {
            let acc = p.accounts.values().next().unwrap();
            assert_eq!(acc.sent, 2);
            assert!(acc.peers.count_ones() >= 1 && acc.peers.count_ones() <= 2);
            assert_eq!(p.spaces.values().next().unwrap().messages, 2);
        });
    }
}
