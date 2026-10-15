//! Receipt watermarks, written behind (ADR 0030 receipts, SCALE.md §3).
//!
//! Every delivered and read receipt used to be one Postgres upsert: one write per delivered
//! copy of every message, the single largest write rate in the relay (3.5M/s at the 1B load
//! model). A watermark only ever rises and only the newest one matters, so receipts fold
//! in memory (max per person per Space) and one statement flushes the whole batch every
//! [`FLUSH_EVERY`]. Live receipts are fanned out at once, unaffected; only the copy kept for
//! replay on sync is delayed, and replay merges what is still pending on this node.
//! A crash loses at most one interval of watermarks, which the next receipt from the same
//! person in that Space supersedes.

use std::{collections::HashMap, sync::Mutex, time::Duration};

use sqlx::PgPool;

use crate::metrics::Metrics;

pub const FLUSH_EVERY: Duration = Duration::from_millis(100);
/// Rows per statement: bounds a single flush's size and its lock footprint.
const CHUNK: usize = 2_000;

type Key = (String, String);

#[derive(Default)]
pub struct Receipts {
    pending: Mutex<HashMap<Key, (u64, u64)>>,
    /// The batch being written right now: still read by replays until it commits, so a
    /// sync during a flush can't miss a watermark that is in neither the table nor `pending`.
    inflight: Mutex<HashMap<Key, (u64, u64)>>,
    /// One flush at a time (the timer and shutdown can overlap).
    flushing: tokio::sync::Mutex<()>,
}

impl Receipts {
    /// Raises (never lowers) a person's watermarks in a Space. `read` implies `delivered`.
    pub fn raise(&self, space: &str, identity: &str, delivered: u64, read: u64) {
        let (delivered, read) = clamp(delivered, read);
        let mut p = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        let w = p
            .entry((space.to_string(), identity.to_string()))
            .or_default();
        w.0 = w.0.max(delivered);
        w.1 = w.1.max(read);
    }

    /// Stored watermarks of a Space merged with the ones still waiting to be flushed here.
    pub async fn of_space(
        &self,
        pool: &PgPool,
        space: &str,
    ) -> Result<Vec<(String, i64, i64)>, sqlx::Error> {
        let mut rows: HashMap<String, (i64, i64)> = crate::db::receipts(pool, space)
            .await?
            .into_iter()
            .map(|(who, d, r)| (who, (d, r)))
            .collect();
        let p = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        let f = self.inflight.lock().unwrap_or_else(|e| e.into_inner());
        for ((s, who), (d, r)) in p.iter().chain(f.iter()) {
            if s == space {
                let w = rows.entry(who.clone()).or_default();
                w.0 = w.0.max(*d as i64);
                w.1 = w.1.max(*r as i64);
            }
        }
        Ok(rows.into_iter().map(|(who, (d, r))| (who, d, r)).collect())
    }

    /// [`Self::of_space`] for many Spaces in one query: Space → (identity, delivered, read).
    pub async fn of_spaces(
        &self,
        pool: &PgPool,
        spaces: &[String],
    ) -> Result<HashMap<String, Vec<(String, i64, i64)>>, sqlx::Error> {
        let mut by: HashMap<String, HashMap<String, (i64, i64)>> = HashMap::new();
        if spaces.is_empty() {
            return Ok(HashMap::new());
        }
        for (space, who, d, r) in crate::db::receipts_of_spaces(pool, spaces).await? {
            by.entry(space).or_default().insert(who, (d, r));
        }
        {
            let wanted: std::collections::HashSet<&str> =
                spaces.iter().map(String::as_str).collect();
            let p = self.pending.lock().unwrap_or_else(|e| e.into_inner());
            for ((s, who), (d, r)) in p.iter() {
                if wanted.contains(s.as_str()) {
                    let w = by
                        .entry(s.clone())
                        .or_default()
                        .entry(who.clone())
                        .or_default();
                    w.0 = w.0.max(*d as i64);
                    w.1 = w.1.max(*r as i64);
                }
            }
        }
        Ok(by
            .into_iter()
            .map(|(s, m)| (s, m.into_iter().map(|(w, (d, r))| (w, d, r)).collect()))
            .collect())
    }

    /// Writes everything pending; on failure the batch is folded back in for the next try.
    pub async fn flush(&self, pool: &PgPool, metrics: &Metrics) -> Result<usize, sqlx::Error> {
        let _one = self.flushing.lock().await;
        let batch: Vec<(Key, (u64, u64))> = {
            let mut p = self.pending.lock().unwrap_or_else(|e| e.into_inner());
            if p.is_empty() {
                return Ok(0);
            }
            let taken = std::mem::take(&mut *p);
            let mut f = self.inflight.lock().unwrap_or_else(|e| e.into_inner());
            f.clone_from(&taken);
            taken.into_iter().collect()
        };
        let result = self.write(pool, metrics, &batch).await;
        self.inflight
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        result
    }

    async fn write(
        &self,
        pool: &PgPool,
        metrics: &Metrics,
        batch: &[(Key, (u64, u64))],
    ) -> Result<usize, sqlx::Error> {
        let n = batch.len();
        for (i, chunk) in batch.chunks(CHUNK).enumerate() {
            if let Err(e) = crate::db::raise_receipts(pool, chunk).await {
                for ((space, who), (d, r)) in batch[i * CHUNK..].iter() {
                    self.raise(space, who, *d, *r);
                }
                return Err(e);
            }
            Metrics::inc(&metrics.receipt_flushes);
        }
        metrics
            .receipts_flushed
            .fetch_add(n as u64, std::sync::atomic::Ordering::Relaxed);
        Ok(n)
    }
}

fn clamp(delivered: u64, read: u64) -> (u64, u64) {
    // A bogus u64 must not wrap to a negative BIGINT.
    let max = i64::MAX as u64;
    (delivered.max(read).min(max), read.min(max))
}

/// Flushes on a timer for the life of the relay.
pub fn spawn(st: &crate::Shared) {
    let st = std::sync::Arc::downgrade(st);
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(FLUSH_EVERY);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            let Some(st) = st.upgrade() else { return };
            if let Err(error) = st.receipts.flush(&st.pool, &st.metrics).await {
                tracing::warn!(%error, "receipt flush failed; retrying next tick");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_to_the_highest_watermark() {
        let r = Receipts::default();
        r.raise("sp", "ana", 5, 0);
        r.raise("sp", "ana", 3, 2);
        r.raise("sp", "ana", 0, 4);
        r.raise("sp", "bia", u64::MAX, 0);
        let p = r.pending.lock().unwrap();
        assert_eq!(p[&("sp".into(), "ana".into())], (5, 4));
        assert_eq!(p[&("sp".into(), "bia".into())], (i64::MAX as u64, 0));
        assert_eq!(p.len(), 2);
    }
}
