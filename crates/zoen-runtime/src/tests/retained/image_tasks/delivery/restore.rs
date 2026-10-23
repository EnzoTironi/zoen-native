//! Restore actual source/dedupe AND original Runtime capsule/Review after a
//! consumed Publish. SQL is proven byte-equivalent over this read-only effect;
//! this is not a pg_dump/full-cluster deployment restore claim.
use super::*;
use foundationdb::{options::StreamingMode, tuple::Subspace, RangeOption};

pub(super) struct Snapshot {
    ranges: Vec<(Vec<u8>, Vec<u8>)>,
    values: Vec<(Vec<u8>, Vec<u8>)>,
    sql_digest: String,
}

async fn sql_digest(f: &Fixture) -> String {
    let mut transaction = f.world.runtime.finance.pool.begin().await.unwrap();
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *transaction)
        .await
        .unwrap();
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT tablename::TEXT FROM pg_tables WHERE schemaname='public' ORDER BY tablename",
    )
    .fetch_all(&mut *transaction)
    .await
    .unwrap();
    let mut snapshot = Vec::new();
    for table in tables {
        // Names come from this isolated fixture database's actual catalog.
        let quoted = table.replace('"', "\"\"");
        let rows: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT row_to_json(r)::TEXT FROM public.\"{quoted}\" r ORDER BY row_to_json(r)::TEXT",
        )))
        .fetch_all(&mut *transaction)
        .await
        .unwrap();
        snapshot.push((table, rows));
    }
    transaction.commit().await.unwrap();
    // Failure evidence contains the digest, never private pairing/credential rows.
    hash(&serde_json::to_vec(&snapshot).unwrap())
}

impl Snapshot {
    pub(super) async fn capture(f: &Fixture) -> Self {
        let ranges = vec![
            f.world.runtime.execution.root.range(),
            Subspace::all()
                .subspace(&("zoen", f.world.relay_cell.as_deref().unwrap()))
                .range(),
        ];
        let transaction = f.world.runtime.execution.transaction().await.unwrap();
        let mut values = Vec::new();
        for range in &ranges {
            let mut options = RangeOption::from(range.clone());
            options.mode = StreamingMode::WantAll;
            options.limit = Some(4096);
            let rows = transaction.get_range(&options, 1, false).await.unwrap();
            assert!(
                !rows.more(),
                "restore fixture must capture every key, never a truncated page"
            );
            values.extend(
                rows.iter()
                    .map(|row| (row.key().to_vec(), row.value().to_vec())),
            );
        }
        assert!(
            values
                .iter()
                .map(|(key, value)| key.len() + value.len())
                .sum::<usize>()
                < 5_000_000,
            "bounded fixture restore must fit a single actual FDB transaction"
        );
        Self {
            ranges,
            values,
            sql_digest: sql_digest(f).await,
        }
    }

    pub(super) async fn restore(&self, f: &Fixture) {
        assert_eq!(
            sql_digest(f).await,
            self.sql_digest,
            "SQL did not change between snapshot and this read-only relay publication"
        );
        let transaction = f.world.runtime.execution.transaction().await.unwrap();
        for (begin, end) in &self.ranges {
            transaction.clear_range(begin, end);
        }
        for (key, value) in &self.values {
            transaction.set(key, value);
        }
        transaction.commit().await.unwrap();
        let actual = Self::capture(f).await;
        assert!(
            actual.values == self.values,
            "all captured source/Runtime bytes must be restored"
        );
        assert_eq!(actual.sql_digest, self.sql_digest);
    }
}

pub(super) async fn refuses_consumed_original(
    f: &mut Fixture,
    intent: &crate::ImageReviewIntent,
    target: &crate::ImageDeliveryTarget,
    before: &Snapshot,
    original: &(String, String, Envelope),
) {
    let after = Snapshot::capture(f).await;
    assert_eq!(before.sql_digest, after.sql_digest);
    before.restore(f).await;
    output::cold(f).await;
    let restored_head = head(f).await;
    assert_eq!(
        proof::review::original(&f.world.runtime, intent).await,
        *original
    );
    assert_eq!(
        proof::review::stage(&f.world.runtime, intent, target)
            .await
            .unwrap_err(),
        RuntimeError::RestoreUnreconciled
    );
    assert_eq!(
        head(f).await,
        restored_head,
        "consumed original cannot append after snapshot restore"
    );
    assert_eq!(
        proof::review::original(&f.world.runtime, intent).await,
        *original
    );
    assert_eq!(
        f.world.count("SELECT count(*) FROM runtime_claims").await,
        0
    );
    assert_eq!(f.world.http.sends.load(Ordering::SeqCst), 0);
    assert_eq!(sql_digest(f).await, before.sql_digest);
    // Restore exact known post-Accepted bytes to continue this same fixture;
    // neither restore touches or recreates the independently consumed record.
    after.restore(f).await;
    output::cold(f).await;
    println!("managed image publication restore: actual source/log/dedupe and original Runtime capsule/Review restored after CP/before append; unchanged actual SQL snapshot; consumed original refused without new append/provider/claim PASS (not full deployment backup/restore)");
}
