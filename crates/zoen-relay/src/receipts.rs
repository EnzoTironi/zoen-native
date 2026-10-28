//! Bounded unsigned receipt folding and keyset replay.
//! Identity rows are never deleted; values only rise. A Sync captures an initial
//! maximum identity, then yields every pre-cut row with at least its pre-cut mark.
//! Later raises/inserts may appear. This is not a globally atomic snapshot.
mod read_time;
use crate::metrics::Metrics;
use relay_admission::receipts::{raise, Watermark, PAGE_ROWS};
use sqlx::PgPool;
use std::{
    collections::BTreeMap,
    ops::Bound::{Excluded, Included},
    sync::Mutex,
    time::Duration,
};
pub const FLUSH_EVERY: Duration = Duration::from_millis(100);
const CHUNK: usize = 2000;
/// Resource bounds for retained receipt keys, independent of SQL availability.
/// A flush holds at most one equally bounded snapshot in addition to this map.
pub const MAX_PENDING_ROWS: usize = 65_536;
pub const MAX_PENDING_KEY_BYTES: usize = 8 * 1024 * 1024;
type Marks = BTreeMap<String, BTreeMap<String, Watermark>>;
#[derive(Debug)]
pub struct Page {
    pub rows: Vec<(String, u64, u64)>,
    pub next: Option<String>,
}
#[derive(Default)]
struct Pending {
    marks: Marks,
    rows: usize,
    // Counts space+identity bytes for every row (conservatively counts shared spaces).
    key_bytes: usize,
}
pub struct Receipts {
    pending: Mutex<Pending>,
    read_times: Mutex<read_time::Pending>,
    flushing: tokio::sync::Mutex<()>,
    max_rows: usize,
    max_key_bytes: usize,
}
impl Default for Receipts {
    fn default() -> Self {
        Self::with_capacity(MAX_PENDING_ROWS, MAX_PENDING_KEY_BYTES)
    }
}
impl Receipts {
    /// Smaller cells can select stricter bounds without changing receipt semantics.
    pub fn with_capacity(max_rows: usize, max_key_bytes: usize) -> Self {
        Self {
            pending: Mutex::default(),
            read_times: Mutex::default(),
            flushing: tokio::sync::Mutex::default(),
            max_rows,
            max_key_bytes,
        }
    }
    /// False means no receipt was admitted: the caller must disconnect before
    /// forwarding it, so a reconnect can replay the client's durable watermarks.
    /// Existing rows still coalesce at capacity; no acknowledged mark is evicted.
    pub fn raise(&self, space: &str, who: &str, delivered: u64, read: u64) -> bool {
        let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(mark) = pending
            .marks
            .get_mut(space)
            .and_then(|rows| rows.get_mut(who))
        {
            *mark = raise(Some(*mark), delivered, read).watermark;
            return true;
        }
        let Some(bytes) = space.len().checked_add(who.len()) else {
            return false;
        };
        let Some(total) = pending.key_bytes.checked_add(bytes) else {
            return false;
        };
        if pending.rows >= self.max_rows || total > self.max_key_bytes {
            return false;
        }
        pending
            .marks
            .entry(space.into())
            .or_default()
            .insert(who.into(), raise(None, delivered, read).watermark);
        pending.rows += 1;
        pending.key_bytes = total;
        true
    }
    /// Retained logical key bytes and rows, not an allocator/RSS measurement.
    pub fn backlog(&self) -> (usize, usize) {
        let pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        (pending.rows, pending.key_bytes)
    }
    /// Capture the local pre-read ceiling before SQL so a flush/clear cannot hide it.
    pub async fn ceilings(
        &self,
        pool: &PgPool,
        spaces: &[String],
    ) -> Result<BTreeMap<String, String>, sqlx::Error> {
        let mut result: BTreeMap<String, String> = BTreeMap::new();
        if spaces.is_empty() {
            return Ok(result);
        }
        {
            let pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
            for space in spaces {
                if let Some((who, _)) = pending
                    .marks
                    .get(space)
                    .and_then(|rows| rows.last_key_value())
                {
                    result.insert(space.clone(), who.clone());
                }
            }
        }
        for (space, who) in crate::db::receipt_ceilings(pool, spaces).await? {
            let slot = result.entry(space).or_default();
            if who.as_str() > slot.as_str() {
                *slot = who
            }
        }
        Ok(result)
    }
    fn local_page(
        &self,
        space: &str,
        after: &str,
        ceiling: &str,
        limit: usize,
    ) -> BTreeMap<String, Watermark> {
        let mut result = BTreeMap::new();
        let pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(rows) = pending.marks.get(space) {
            for (who, mark) in rows
                .range::<str, _>((Excluded(after), Included(ceiling)))
                .take(limit)
            {
                result.insert(who.clone(), *mark);
            }
        }
        result
    }
    pub async fn page(
        &self,
        pool: &PgPool,
        space: &str,
        after: &str,
        ceiling: &str,
    ) -> Result<Page, sqlx::Error> {
        self.page_after_query(pool, space, after, ceiling, std::future::ready(()))
            .await
    }
    /// Private deterministic test seam pauses only AFTER an actual SQL query.
    /// It cannot supply rows, membership or any authenticated facts.
    async fn page_after_query(
        &self,
        pool: &PgPool,
        space: &str,
        after: &str,
        ceiling: &str,
        after_query: impl std::future::Future<Output = ()>,
    ) -> Result<Page, sqlx::Error> {
        relay_admission::receipts::validate_page(after, ceiling, PAGE_ROWS as u32)
            .map_err(|e| sqlx::Error::Protocol(format!("invalid receipt cursor: {e:?}")))?;
        let mut result = self.local_page(space, after, ceiling, PAGE_ROWS + 1);
        let rows = crate::db::receipt_page(pool, space, after, ceiling, PAGE_ROWS + 1).await?;
        after_query.await;
        for (who, d, r) in rows {
            let mark = result.entry(who).or_default();
            *mark = raise(Some(*mark), d, r).watermark;
        }
        while result.len() > PAGE_ROWS + 1 {
            result.pop_last();
        }
        let more = result.len() > PAGE_ROWS;
        let rows: Vec<_> = result
            .into_iter()
            .take(PAGE_ROWS)
            .map(|(who, mark)| {
                let (d, r) = mark.pair();
                (who, d, r)
            })
            .collect();
        let next = more.then(|| rows.last().expect("lookahead requires full page").0.clone());
        Ok(Page { rows, next })
    }
    pub async fn flush(&self, pool: &PgPool, metrics: &Metrics) -> Result<usize, sqlx::Error> {
        let _one = self.flushing.lock().await;
        let batch: Vec<_> = {
            let pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
            pending
                .marks
                .iter()
                .flat_map(|(space, rows)| {
                    rows.iter()
                        .map(|(who, mark)| ((space.clone(), who.clone()), mark.pair()))
                })
                .collect()
        };
        for chunk in batch.chunks(CHUNK) {
            // Keep the source map intact until the actual SQL statement commits.
            // On failure it already contains every unpersisted/current watermark.
            crate::db::raise_receipts(pool, chunk).await?;
            let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
            for ((space, who), pair) in chunk {
                let Some(rows) = pending.marks.get_mut(space) else {
                    continue;
                };
                if rows.get(who).is_some_and(|mark| mark.pair() == *pair) {
                    rows.remove(who);
                    if rows.is_empty() {
                        pending.marks.remove(space);
                    }
                    pending.rows -= 1;
                    pending.key_bytes -= space.len() + who.len();
                }
            }
            Metrics::inc(&metrics.receipt_flushes);
            metrics
                .receipts_flushed
                .fetch_add(chunk.len() as u64, std::sync::atomic::Ordering::Relaxed);
        }
        self.flush_read_times(pool).await?;
        Ok(batch.len())
    }
}

pub fn spawn(st: &crate::Shared) {
    let st = std::sync::Arc::downgrade(st);
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(FLUSH_EVERY);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            let Some(st) = st.upgrade() else { return };
            if let Err(error) = st.receipts.flush(&st.pool, &st.metrics).await {
                tracing::warn!(%error,"receipt flush failed; retrying next tick");
            }
        }
    });
}

#[cfg(test)]
mod tests;
