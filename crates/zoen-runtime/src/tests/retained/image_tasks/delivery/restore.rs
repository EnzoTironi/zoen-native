//! Restore actual source/dedupe AND original Runtime capsule/Review after a
//! consumed Publish. Registered WS reconnects legitimately touch last_seen;
//! operational partition ownership remains live and is never rewound. This is
//! scoped logical-state rollback, not pg_dump/full-cluster deployment restore.
use super::*;
mod ownership;
mod sql;
use foundationdb::{options::StreamingMode, tuple::Subspace, KeySelector, RangeOption};

pub(super) struct Snapshot {
    ranges: Vec<(Vec<u8>, Vec<u8>)>,
    values: Vec<(Vec<u8>, Vec<u8>)>,
    sql: sql::Snapshot,
    ownership: (Vec<u8>, Vec<u8>),
}

impl Snapshot {
    pub(super) async fn capture(f: &Fixture) -> Self {
        let cell = Subspace::all().subspace(&("zoen", f.world.relay_cell.as_deref().unwrap()));
        let (cell_begin, cell_end) = cell.range();
        let ownership = cell.subspace(&("ownership",)).range();
        assert!(cell_begin < ownership.0 && ownership.0 < ownership.1 && ownership.1 < cell_end);
        // All cell data is restored except the exact operational ownership range.
        // The fixed 4096 partition leases are not part of this logical snapshot.
        let ranges = vec![
            f.world.runtime.execution.root.range(),
            (cell_begin, ownership.0.clone()),
            (ownership.1.clone(), cell_end),
        ];
        for (index, (begin, end)) in ranges.iter().enumerate() {
            assert!(begin < end);
            assert!(
                end <= &ownership.0 || &ownership.1 <= begin,
                "capture/clear range must not overlap operational ownership"
            );
            for (other_begin, other_end) in &ranges[..index] {
                assert!(
                    end <= other_begin || other_end <= begin,
                    "capture ranges must be disjoint"
                );
            }
        }
        let transaction = f.world.runtime.execution.transaction().await.unwrap();
        let mut values = Vec::new();
        let mut total_bytes = 0usize;
        let mut pages = 0usize;
        for range in &ranges {
            let mut previous: Option<Vec<u8>> = None;
            let mut options = RangeOption::from(range.clone());
            options.mode = StreamingMode::WantAll;
            // A soft response size deliberately exercises normal paging; WantAll
            // does not promise the complete range in one response.
            options.target_bytes = 16 * 1024;
            let mut complete = false;
            for iteration in 1..=4097 {
                options.limit = Some(4097 - values.len());
                let rows = transaction
                    .get_range(&options, iteration, false)
                    .await
                    .unwrap();
                pages += 1;
                assert!(
                    !rows.is_empty() || !rows.more(),
                    "snapshot page must advance"
                );
                for row in rows.iter() {
                    assert!(
                        range.0.as_slice() <= row.key() && row.key() < range.1.as_slice(),
                        "snapshot key must remain in its exact capture range"
                    );
                    assert!(
                        previous.as_deref().is_none_or(|last| last < row.key()),
                        "snapshot pages must be strictly ordered without duplicate keys"
                    );
                    previous = Some(row.key().to_vec());
                    assert!(
                        values.len() < 4096,
                        "restore fixture exceeds total key bound"
                    );
                    total_bytes = total_bytes
                        .checked_add(row.key().len())
                        .and_then(|n| n.checked_add(row.value().len()))
                        .unwrap();
                    assert!(
                        total_bytes < 5_000_000,
                        "bounded fixture restore must fit a single actual FDB transaction"
                    );
                    values.push((row.key().to_vec(), row.value().to_vec()));
                }
                if !rows.more() {
                    complete = true;
                    break;
                }
                let last = rows.last().expect("nonterminal page must contain a key");
                options.begin = KeySelector::first_greater_than(last.key().to_vec());
            }
            assert!(
                complete,
                "restore fixture must capture every key, never a truncated range"
            );
        }
        println!(
            "managed restore snapshot: {} keys / {} bytes / {} pages in one FDB transaction",
            values.len(),
            total_bytes,
            pages
        );
        Self {
            ranges,
            values,
            sql: sql::Snapshot::capture(f).await,
            ownership,
        }
    }

    pub(super) async fn restore(&self, f: &Fixture, current_sql: &sql::Snapshot) {
        current_sql.assert_exact(&sql::Snapshot::capture(f).await);
        let transaction = f.world.runtime.execution.transaction().await.unwrap();
        // Compare current full ownership bytes in the same actual transaction;
        // heartbeats outside this operation may legitimately renew those leases.
        let live_ownership = ownership::state(&transaction, &self.ownership).await;
        for (begin, end) in &self.ranges {
            assert!(end <= &self.ownership.0 || &self.ownership.1 <= begin);
            transaction.clear_range(begin, end);
        }
        for (key, value) in &self.values {
            transaction.set(key, value);
        }
        assert_eq!(
            ownership::state(&transaction, &self.ownership).await,
            live_ownership,
            "logical restore must preserve every live ownership byte and fence"
        );
        transaction.commit().await.unwrap();
        println!("managed restore: preserved {} complete current ownership records without rewinding fences", live_ownership.len());
        let actual = Self::capture(f).await;
        assert!(
            actual.values == self.values,
            "all captured source/Runtime bytes must be restored"
        );
        current_sql.assert_exact(&actual.sql);
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
    before
        .sql
        .assert_only_authenticated_ws_touch(&after.sql, &f.world.agents[0]);
    before.restore(f, &after.sql).await;
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
    after.sql.assert_exact(&sql::Snapshot::capture(f).await);
    // Restore exact known post-Accepted bytes to continue this same fixture;
    // neither restore touches or recreates the independently consumed record.
    after.restore(f, &after.sql).await;
    output::cold(f).await;
    println!("managed image publication restore: actual source/log/dedupe and original Runtime capsule/Review restored after CP/before append; current complete SQL preserved including authoritative checkpoint and monotonic authenticated reconnect timestamp; consumed original refused without new append/provider/claim PASS (not full deployment backup/restore)");
}
