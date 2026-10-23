//! Read-only evidence that logical restore never rewinds operational ownership.
use super::*;
use foundationdb::Transaction;
use zoen_relay::ownership::{MAX_NODES, PARTITION_COUNT};

pub(super) async fn state(
    transaction: &Transaction,
    range: &(Vec<u8>, Vec<u8>),
) -> Vec<(Vec<u8>, Vec<u8>)> {
    // Actual ownership has one lease per partition, at most MAX_NODES registry
    // entries and the registry-admission marker. This is not the restored set.
    let max_rows = usize::try_from(PARTITION_COUNT).unwrap() + MAX_NODES + 1;
    let mut options = RangeOption::from(range.clone());
    options.mode = StreamingMode::WantAll;
    options.target_bytes = 32 * 1024;
    let mut values = Vec::new();
    let mut bytes = 0usize;
    for iteration in 1..=max_rows + 1 {
        options.limit = Some(max_rows + 1 - values.len());
        let rows = transaction
            .get_range(&options, iteration, false)
            .await
            .unwrap();
        assert!(
            !rows.is_empty() || !rows.more(),
            "ownership page must advance"
        );
        for row in rows.iter() {
            assert!(
                values.len() < max_rows,
                "ownership evidence exceeds finite registry bound"
            );
            assert!(range.0.as_slice() <= row.key() && row.key() < range.1.as_slice());
            assert!(
                values
                    .last()
                    .is_none_or(|(key, _): &(Vec<u8>, Vec<u8>)| key.as_slice() < row.key()),
                "ownership evidence must not duplicate keys"
            );
            bytes = bytes
                .checked_add(row.key().len())
                .and_then(|n| n.checked_add(row.value().len()))
                .unwrap();
            assert!(
                bytes < 5_000_000,
                "ownership evidence exceeds bounded read budget"
            );
            values.push((row.key().to_vec(), row.value().to_vec()));
        }
        if !rows.more() {
            assert!(!values.is_empty(), "actual cell ownership must exist");
            return values;
        }
        options.begin = KeySelector::first_greater_than(rows.last().unwrap().key().to_vec());
    }
    panic!("ownership evidence must include the complete bounded range");
}
