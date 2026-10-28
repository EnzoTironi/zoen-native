//! Timestamp intervals have their own bounded retention, not watermark provenance.
use super::Receipts;
use protocol::read_time::ReadTime;
use std::collections::BTreeMap;

type Key = (String, String, u64, u64);
#[derive(Default)]
pub(super) struct Pending {
    rows: BTreeMap<Key, ReadTime>,
    bytes: usize,
}
impl Receipts {
    /// Called only after authenticated membership admission. Refusal disconnects for replay.
    pub fn raise_time(&self, space: &str, who: &str, time: &ReadTime) -> bool {
        if !time.valid() {
            return false;
        }
        let key = (space.into(), who.into(), time.after_seq, time.through_seq);
        let mut pending = self.read_times.lock().unwrap_or_else(|e| e.into_inner());
        if pending.rows.contains_key(&key) {
            return true;
        }
        let Some(bytes) = space
            .len()
            .checked_add(who.len())
            .and_then(|n| n.checked_add(24))
        else {
            return false;
        };
        let Some(total) = pending.bytes.checked_add(bytes) else {
            return false;
        };
        if pending.rows.len() >= self.max_rows || total > self.max_key_bytes {
            return false;
        }
        pending.rows.insert(key, time.clone());
        pending.bytes = total;
        true
    }
    pub(super) async fn flush_read_times(&self, pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
        let batch: Vec<_> = {
            let p = self.read_times.lock().unwrap_or_else(|e| e.into_inner());
            p.rows
                .iter()
                .map(|(key, time)| (key.clone(), time.clone()))
                .collect()
        };
        for chunk in batch.chunks(super::CHUNK) {
            crate::db::read_time::save(pool, chunk).await?;
            let mut p = self.read_times.lock().unwrap_or_else(|e| e.into_inner());
            for (key, time) in chunk {
                if p.rows.get(key) == Some(time) {
                    p.rows.remove(key);
                    p.bytes -= key.0.len() + key.1.len() + 24;
                }
            }
        }
        Ok(())
    }
    /// Caller has checked current reader/peer membership and the small-space audience.
    pub async fn read_times(
        &self,
        pool: &sqlx::PgPool,
        space: &str,
        peers: &[String],
    ) -> Result<Vec<(String, ReadTime)>, sqlx::Error> {
        let mut rows = crate::db::read_time::read(pool, space, peers).await?;
        let pending = self.read_times.lock().unwrap_or_else(|e| e.into_inner());
        for ((s, who, _, _), time) in &pending.rows {
            if s == space
                && peers.contains(who)
                && !rows.iter().any(|(id, t)| {
                    id == who && t.after_seq == time.after_seq && t.through_seq == time.through_seq
                })
            {
                rows.push((who.clone(), time.clone()));
            }
        }
        rows.sort_by(|a, b| {
            (&a.0, a.1.through_seq, a.1.after_seq).cmp(&(&b.0, b.1.through_seq, b.1.after_seq))
        });
        // The input peer count is itself bounded by the ordinary receipt audience.
        let mut counts = BTreeMap::<String, usize>::new();
        rows.reverse();
        rows.retain(|(who, _)| {
            let n = counts.entry(who.clone()).or_default();
            *n += 1;
            *n <= protocol::read_time::MAX_INTERVALS
        });
        rows.reverse();
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timestamp_queue_keeps_first_claim_and_refuses_capacity_without_eviction() {
        let r = Receipts::with_capacity(1, 100);
        let report = ReadTime {
            after_seq: 4,
            through_seq: 8,
            reported_at_ms: 1234,
        };
        assert!(r.raise_time("s", "p", &report));
        assert!(r.raise_time(
            "s",
            "p",
            &ReadTime {
                reported_at_ms: 9999,
                ..report.clone()
            }
        ));
        assert!(!r.raise_time("s", "other", &report));
        let pending = r.read_times.lock().unwrap();
        assert_eq!(pending.rows.len(), 1);
        assert_eq!(pending.rows.values().next().unwrap(), &report);
    }
    #[test]
    fn invalid_timestamp_interval_is_not_admitted() {
        let r = Receipts::default();
        assert!(!r.raise_time(
            "s",
            "p",
            &ReadTime {
                after_seq: 3,
                through_seq: 3,
                reported_at_ms: 10
            }
        ));
        assert!(r.read_times.lock().unwrap().rows.is_empty());
    }
}

#[cfg(test)]
mod sql_tests {
    use super::*;
    #[tokio::test]
    async fn actual_sql_reopen_preserves_first_peer_time_and_expires_only_time_evidence(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use futures_util::FutureExt;
        use sqlx::{Connection, PgConnection};
        let admin = std::env::var("ZOEN_TEST_PG")?;
        let name = format!(
            "zoen_read_time_{}",
            &event_log::Signer::generate()?.id()[..16]
        );
        let space = types::new_id("sp")?;
        let peer = event_log::Signer::generate()?.id();
        let options = admin
            .parse::<sqlx::postgres::PgConnectOptions>()?
            .database(&name);
        let mut connection = PgConnection::connect(&admin).await?;
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {name}")))
            .execute(&mut connection)
            .await?;
        let pool = sqlx::PgPool::connect_lazy_with(options);
        let outcome = std::panic::AssertUnwindSafe(async {
            sqlx::raw_sql(include_str!("../../migrations/0030_receipts.sql"))
                .execute(&pool)
                .await?;
            sqlx::raw_sql(include_str!(
                "../../migrations/0041_read_time_intervals.sql"
            ))
            .execute(&pool)
            .await?;
            let report = ReadTime {
                after_seq: 5,
                through_seq: 9,
                reported_at_ms: 1234,
            };
            let live = Receipts::default();
            assert!(live.raise(&space, &peer, 0, 9));
            assert!(live.raise_time(&space, &peer, &report));
            live.flush(&pool, &crate::metrics::Metrics::default())
                .await?;
            let reopened = Receipts::default();
            let peers = vec![peer.clone()];
            assert_eq!(
                reopened.read_times(&pool, &space, &peers).await?,
                vec![(peer.clone(), report.clone())]
            );
            assert!(reopened.raise_time(
                &space,
                &peer,
                &ReadTime {
                    reported_at_ms: 9999,
                    ..report.clone()
                }
            ));
            reopened
                .flush(&pool, &crate::metrics::Metrics::default())
                .await?;
            assert_eq!(
                reopened.read_times(&pool, &space, &peers).await?[0]
                    .1
                    .reported_at_ms,
                1234
            );
            sqlx::query(
                "UPDATE space_read_times SET observed_at=clock_timestamp()-INTERVAL '8 days'",
            )
            .execute(&pool)
            .await?;
            assert!(reopened.read_times(&pool, &space, &peers).await?.is_empty());
            // First page has an empty cursor and this sole identity as its inclusive ceiling.
            let page = reopened.page(&pool, &space, "", &peer).await?;
            assert_eq!(page.rows, vec![(peer.clone(), 9, 9)]);
            assert!(page.next.is_none());
            Ok::<(), Box<dyn std::error::Error>>(())
        })
        .catch_unwind()
        .await;
        pool.close().await;
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP DATABASE {name} WITH (FORCE)"
        )))
        .execute(&mut connection)
        .await?;
        match outcome {
            Ok(result) => result,
            Err(panic) => std::panic::resume_unwind(panic),
        }
    }
}
