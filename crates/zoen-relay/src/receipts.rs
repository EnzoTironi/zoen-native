//! Exact unsigned receipt folding and bounded keyset replay.
//! Identity rows are never deleted; values only rise. A Sync captures an initial
//! maximum identity, then yields every pre-cut row with at least its pre-cut mark.
//! Later raises/inserts may appear. This is not a globally atomic snapshot.
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
type Key = (String, String);
type Marks = BTreeMap<Key, Watermark>;
#[derive(Debug)]
pub struct Page {
    pub rows: Vec<(String, u64, u64)>,
    pub next: Option<String>,
}
#[derive(Default)]
pub struct Receipts {
    pending: Mutex<Marks>,
    inflight: Mutex<Marks>,
    flushing: tokio::sync::Mutex<()>,
}
impl Receipts {
    pub fn raise(&self, space: &str, who: &str, delivered: u64, read: u64) {
        let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        let mark = pending.entry((space.into(), who.into())).or_default();
        *mark = raise(Some(*mark), delivered, read).watermark;
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
            let inflight = self.inflight.lock().unwrap_or_else(|e| e.into_inner());
            for space in spaces {
                for marks in [&*pending, &*inflight] {
                    if let Some(((.., who), _)) = marks
                        .range((
                            Included((space.clone(), String::new())),
                            Included((space.clone(), "\u{10ffff}".into())),
                        ))
                        .next_back()
                    {
                        let slot = result.entry(space.clone()).or_default();
                        if who.as_str() > slot.as_str() {
                            *slot = who.clone()
                        }
                    }
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
        let inflight = self.inflight.lock().unwrap_or_else(|e| e.into_inner());
        for marks in [&*pending, &*inflight] {
            for ((_, who), mark) in marks
                .range((
                    Excluded((space.into(), after.into())),
                    Included((space.into(), ceiling.into())),
                ))
                .take(limit)
            {
                let existing = result.entry(who.clone()).or_default();
                let (d, r) = mark.pair();
                *existing = raise(Some(*existing), d, r).watermark;
                if result.len() > limit {
                    result.pop_last();
                }
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
            let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
            if pending.is_empty() {
                return Ok(0);
            }
            let taken = std::mem::take(&mut *pending);
            self.inflight
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone_from(&taken);
            taken
                .into_iter()
                .map(|(key, mark)| (key, mark.pair()))
                .collect()
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
        batch: &[crate::db::ReceiptRow],
    ) -> Result<usize, sqlx::Error> {
        for (index, chunk) in batch.chunks(CHUNK).enumerate() {
            if let Err(error) = crate::db::raise_receipts(pool, chunk).await {
                for ((space, who), (d, r)) in &batch[index * CHUNK..] {
                    self.raise(space, who, *d, *r)
                }
                return Err(error);
            }
            Metrics::inc(&metrics.receipt_flushes);
        }
        metrics
            .receipts_flushed
            .fetch_add(batch.len() as u64, std::sync::atomic::Ordering::Relaxed);
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
mod tests {
    use super::*;
    #[test]
    fn folds_unsigned_without_clamp() {
        let r = Receipts::default();
        r.raise("sp", "a", 5, 4);
        r.raise("sp", "a", 3, u64::MAX);
        assert_eq!(
            r.pending.lock().unwrap()[&("sp".into(), "a".into())].pair(),
            (u64::MAX, u64::MAX)
        );
    }
    #[test]
    fn bounded_local_keyset_includes_pending_and_inflight_without_cross_space_rows() {
        let r = Receipts::default();
        for n in 0..400 {
            r.raise("sp", &format!("{n:064x}"), n, 0);
        }
        r.raise("other", "000", u64::MAX, 0);
        let rows = r.local_page("sp", "", &format!("{:064x}", 399), PAGE_ROWS + 1);
        assert_eq!(rows.len(), PAGE_ROWS + 1);
        assert_eq!(rows.first_key_value().unwrap().0, &format!("{:064x}", 0));
        assert_eq!(
            rows.last_key_value().unwrap().0,
            &format!("{:064x}", PAGE_ROWS)
        );
    }
    async fn owned_database() -> (PgPool, String, String) {
        use sqlx::{Connection, PgConnection};
        let admin =
            std::env::var("ZOEN_TEST_PG").expect("set ZOEN_TEST_PG for actual receipt race tests");
        let name = format!(
            "zoen_receipt_race_{}",
            &event_log::Signer::generate().id()[..16]
        );
        let mut connection = PgConnection::connect(&admin).await.unwrap();
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {name}")))
            .execute(&mut connection)
            .await
            .unwrap();
        let options = admin
            .parse::<sqlx::postgres::PgConnectOptions>()
            .unwrap()
            .database(&name);
        let pool = PgPool::connect_with(options).await.unwrap();
        sqlx::query(include_str!("../migrations/0030_receipts.sql"))
            .execute(&pool)
            .await
            .unwrap();
        (pool, admin, name)
    }
    async fn dispose_database(pool: PgPool, admin: &str, name: &str) {
        use sqlx::{Connection, PgConnection};
        pool.close().await;
        let mut connection = PgConnection::connect(admin).await.unwrap();
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP DATABASE {name} WITH (FORCE)"
        )))
        .execute(&mut connection)
        .await
        .unwrap();
    }
    #[tokio::test]
    async fn local_prequery_capture_survives_actual_sql_snapshot_then_flush_and_clear() {
        let (pool, admin, name) = owned_database().await;
        let receipts = std::sync::Arc::new(Receipts::default());
        let who = "ab".repeat(32);
        receipts.raise("sp", &who, 7, 5);
        let (read_tx, read_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let task = {
            let r = receipts.clone();
            let pool = pool.clone();
            let who = who.clone();
            tokio::spawn(async move {
                r.page_after_query(&pool, "sp", "", &who, async move {
                    read_tx.send(()).unwrap();
                    release_rx.await.unwrap();
                })
                .await
                .unwrap()
            })
        };
        read_rx.await.unwrap(); // The actual PostgreSQL SELECT already returned no rows.
        receipts.flush(&pool, &Metrics::default()).await.unwrap();
        assert!(receipts.pending.lock().unwrap().is_empty());
        assert!(receipts.inflight.lock().unwrap().is_empty());
        release_tx.send(()).unwrap();
        let actual = task.await.unwrap();
        dispose_database(pool, &admin, &name).await;
        assert_eq!(actual.rows, vec![(who, 7, 5)]); // Previously lost between SELECT and postquery merge.
    }
    #[tokio::test]
    async fn actual_blocked_upsert_leaves_inflight_visible_to_concurrent_page() {
        let (pool, admin, name) = owned_database().await;
        let who = "cd".repeat(32);
        crate::db::raise_receipts(&pool, &[(("sp".into(), who.clone()), (0, 0))])
            .await
            .unwrap();
        let mut lock = pool.begin().await.unwrap();
        sqlx::query("SELECT identity FROM space_receipts WHERE space='sp' FOR UPDATE")
            .fetch_all(&mut *lock)
            .await
            .unwrap();
        let receipts = std::sync::Arc::new(Receipts::default());
        receipts.raise("sp", &who, u64::MAX, u64::MAX - 1);
        let task = {
            let r = receipts.clone();
            let p = pool.clone();
            tokio::spawn(async move { r.flush(&p, &Metrics::default()).await.unwrap() })
        };
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if !receipts.inflight.lock().unwrap().is_empty() {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let actual = receipts.page(&pool, "sp", "", &who).await.unwrap(); // SQL sees old0; actual inflight supplies full u64.
        lock.commit().await.unwrap();
        task.await.unwrap();
        dispose_database(pool, &admin, &name).await;
        assert_eq!(actual.rows, vec![(who, u64::MAX, u64::MAX - 1)]);
    }
}
