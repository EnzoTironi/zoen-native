use protocol::Cursor;
use relay_admission::sync::{cursor_read, include_current, SyncRead};
fn cursor(space: &str, next_seq: u64) -> Cursor {
    Cursor {
        space: space.into(),
        next_seq,
    }
}
#[test]
fn native_removed_history_includes_only_its_removal_and_never_later_events() {
    assert!(cursor_read(cursor("unknown", 0), false, None).is_none());
    for next in [0, 7, 8] {
        assert_eq!(
            cursor_read(cursor("removed", next), false, Some(8)),
            Some(SyncRead {
                space: "removed".into(),
                from: next,
                through: Some(8)
            })
        );
    }
    assert!(cursor_read(cursor("removed", 9), false, Some(8)).is_none());
    assert!(cursor_read(cursor("genesis", 0), false, Some(0)).is_some());
    assert!(cursor_read(cursor("genesis", 1), false, Some(0)).is_none());
}
#[test]
fn native_sync_all_keeps_requested_order_repeats_and_adds_each_current_space_once() {
    let mut plan = vec![
        SyncRead {
            space: "b".into(),
            from: 9,
            through: None,
        },
        SyncRead {
            space: "old".into(),
            from: 5,
            through: Some(5),
        },
        SyncRead {
            space: "b".into(),
            from: 3,
            through: None,
        },
    ];
    let original = plan.clone();
    let current = vec!["a".into(), "b".into(), "c".into(), "a".into()];
    include_current(&mut plan, &current, false);
    assert_eq!(plan, original);
    include_current(&mut plan, &current, true);
    assert_eq!(&plan[..3], original);
    assert_eq!(
        plan[3..],
        [
            SyncRead {
                space: "a".into(),
                from: 0,
                through: None
            },
            SyncRead {
                space: "c".into(),
                from: 0,
                through: None
            }
        ]
    );
}
#[test]
fn native_rejoin_and_full_unsigned_sequences_never_keep_a_stale_removal_ceiling() {
    for seq in [0, 9_007_199_254_740_993, u64::MAX] {
        assert_eq!(
            cursor_read(cursor("rejoined", seq), true, Some(8)),
            Some(SyncRead {
                space: "rejoined".into(),
                from: seq,
                through: None
            })
        );
        assert_eq!(
            cursor_read(cursor("removed", seq), false, Some(u64::MAX))
                .unwrap()
                .through,
            Some(u64::MAX)
        );
    }
}
