use relay_admission::mls_projection::{
    apply, prune_floor, pruned_cursor, transition, Holds, Write, ADDED, PRUNE_BATCH,
};
use relay_admission::Effect;
use std::collections::BTreeMap;
use types::{Privacy, Role, SpaceKind};
fn members() -> BTreeMap<String, Role> {
    BTreeMap::from([("owner".into(), Role::Owner), ("peer".into(), Role::Member)])
}

#[test]
fn encrypted_genesis_and_add_create_holds_before_any_welcome_can_be_pruned() {
    let effect = Effect::Create {
        kind: SpaceKind::Direct,
        privacy: Privacy::EndToEnd,
    };
    let plan = transition(
        &effect,
        None,
        "owner",
        0,
        1000,
        &BTreeMap::new(),
        &Holds::new(),
    )
    .unwrap();
    let (mut mls, mut holds) = (None, Holds::new());
    apply(&mut mls, &mut holds, &plan);
    assert_eq!(mls, Some((0, String::new())));
    assert_eq!(holds.get(&("owner".into(), ADDED.into())), Some(&(0, 1000)));
    let plan = transition(
        &Effect::Add {
            identity: "peer".into(),
            role: Role::Member,
            by_invite: false,
        },
        Some(Privacy::EndToEnd),
        "owner",
        1,
        1001,
        &members(),
        &holds,
    )
    .unwrap();
    apply(&mut mls, &mut holds, &plan);
    assert_eq!(holds.get(&("peer".into(), ADDED.into())), Some(&(1, 1001)));
    assert_eq!(
        prune_floor(Some(Privacy::EndToEnd), &members(), &holds, 1002, 3000),
        Some(0)
    );
}

#[test]
fn a_worker_checkpoint_does_not_create_a_cold_siblings_retention_hold() {
    for native_announced in [false, true] {
        let people = members();
        let mut epoch = None;
        let mut holds = Holds::from([
            (("owner".into(), ADDED.into()), (0, 1000)),
            (("peer".into(), ADDED.into()), (1, 1000)),
        ]);
        if native_announced {
            let writes = transition(
                &Effect::Hold {
                    device: "cold-native".into(),
                },
                Some(Privacy::EndToEnd),
                "peer",
                2,
                1001,
                &people,
                &holds,
            )
            .unwrap();
            apply(&mut epoch, &mut holds, &writes);
        }
        // Both warm devices checkpoint beyond a Welcome at sequence 3. The
        // shared identity sentinel is removed by peer/worker's checkpoint.
        for (author, device) in [("owner", "owner-device"), ("peer", "worker")] {
            let writes = transition(
                &Effect::Checkpoint {
                    device: device.into(),
                    upto: 8,
                },
                Some(Privacy::EndToEnd),
                author,
                9,
                1002,
                &people,
                &holds,
            )
            .unwrap();
            apply(&mut epoch, &mut holds, &writes);
        }
        assert!(!holds.contains_key(&("peer".into(), ADDED.into())));
        assert_eq!(
            holds.contains_key(&("peer".into(), "cold-native".into())),
            native_announced
        );
        assert_eq!(
            prune_floor(Some(Privacy::EndToEnd), &people, &holds, 1003, 3000),
            Some(if native_announced { 2 } else { 8 })
        );
    }
}

#[test]
fn encryption_installs_every_member_hold_and_removal_drops_all_their_devices() {
    let plan = transition(
        &Effect::Encrypt,
        Some(Privacy::Closed),
        "owner",
        8,
        1000,
        &members(),
        &Holds::new(),
    )
    .unwrap();
    let (mut mls, mut holds) = (None, Holds::new());
    apply(&mut mls, &mut holds, &plan);
    assert_eq!(holds.len(), 2);
    holds.insert(("peer".into(), "device".into()), (12, 1100));
    let plan = transition(
        &Effect::Remove {
            identity: "peer".into(),
        },
        Some(Privacy::EndToEnd),
        "owner",
        13,
        1200,
        &members(),
        &holds,
    )
    .unwrap();
    apply(&mut mls, &mut holds, &plan);
    assert_eq!(
        holds.keys().collect::<Vec<_>>(),
        vec![&("owner".into(), ADDED.into())]
    );
}

#[test]
fn checkpoints_never_regress_but_equal_checkpoint_refreshes_live_hold() {
    let mut holds = Holds::from([
        (("owner".into(), ADDED.into()), (0, 100)),
        (("owner".into(), "dev".into()), (7, 200)),
    ]);
    let (mut mls, people) = (Some((3, "dev".into())), members());
    let old = transition(
        &Effect::Checkpoint {
            device: "dev".into(),
            upto: 6,
        },
        Some(Privacy::EndToEnd),
        "owner",
        9,
        900,
        &people,
        &holds,
    )
    .unwrap();
    assert_eq!(
        old,
        vec![Write::ClearHold {
            identity: "owner".into(),
            device: ADDED.into()
        }]
    );
    apply(&mut mls, &mut holds, &old);
    assert_eq!(holds.get(&("owner".into(), "dev".into())), Some(&(7, 200)));
    let same = transition(
        &Effect::Checkpoint {
            device: "dev".into(),
            upto: 7,
        },
        Some(Privacy::EndToEnd),
        "owner",
        10,
        1000,
        &people,
        &holds,
    )
    .unwrap();
    apply(&mut mls, &mut holds, &same);
    assert_eq!(holds.get(&("owner".into(), "dev".into())), Some(&(7, 1000)));
    assert_eq!(
        transition(
            &Effect::Checkpoint {
                device: String::new(),
                upto: 7
            },
            Some(Privacy::EndToEnd),
            "owner",
            10,
            1000,
            &people,
            &holds
        )
        .unwrap()
        .len(),
        0
    );
}

#[test]
fn current_epoch_and_device_join_writes_preserve_native_meaning_and_no_epoch_wraps() {
    let mut holds = Holds::new();
    let mut mls = Some((5, "first".into()));
    let plan = transition(
        &Effect::Commit {
            epoch: 5,
            device: "next".into(),
        },
        Some(Privacy::EndToEnd),
        "owner",
        10,
        100,
        &members(),
        &holds,
    )
    .unwrap();
    apply(&mut mls, &mut holds, &plan);
    assert_eq!(mls, Some((6, "next".into())));
    let plan = transition(
        &Effect::Hold {
            device: "joining".into(),
        },
        Some(Privacy::EndToEnd),
        "owner",
        11,
        101,
        &members(),
        &holds,
    )
    .unwrap();
    apply(&mut mls, &mut holds, &plan);
    assert_eq!(
        holds.get(&("owner".into(), "joining".into())),
        Some(&(11, 101))
    );
    assert_eq!(
        transition(
            &Effect::Commit {
                epoch: u64::MAX,
                device: "next".into()
            },
            Some(Privacy::EndToEnd),
            "owner",
            12,
            102,
            &members(),
            &holds
        )
        .unwrap_err(),
        "MLS epoch exhausted"
    );
}

#[test]
fn missing_holds_fence_pruning_and_native_expired_device_ceiling_is_exact() {
    let people = members();
    let mut holds = Holds::from([(("owner".into(), "dev".into()), (4, 900))]);
    assert_eq!(
        prune_floor(Some(Privacy::EndToEnd), &people, &holds, 1000, 100),
        None
    );
    holds.insert(("peer".into(), ADDED.into()), (2, 999));
    assert_eq!(
        prune_floor(Some(Privacy::EndToEnd), &people, &holds, 1000, 100),
        Some(2)
    );
    holds.insert(("peer".into(), "other".into()), (9, 1000));
    holds.remove(&("peer".into(), ADDED.into()));
    assert_eq!(
        prune_floor(Some(Privacy::EndToEnd), &people, &holds, 1000, 100),
        Some(9)
    );
    assert_eq!(
        prune_floor(Some(Privacy::Closed), &people, &holds, 1000, 100),
        None
    );
    assert_eq!(
        prune_floor(Some(Privacy::EndToEnd), &BTreeMap::new(), &holds, 1000, 100),
        None
    );
    assert_eq!(
        prune_floor(Some(Privacy::EndToEnd), &people, &holds, i64::MAX, 1),
        None
    );
    assert_eq!(
        prune_floor(Some(Privacy::EndToEnd), &people, &holds, i64::MIN, 1),
        Some(4)
    );
}

#[test]
fn bounded_pruning_cursor_keeps_sparse_rows_and_cannot_skip_a_full_batch() {
    assert_eq!(pruned_cursor(0, 1000, PRUNE_BATCH, Some(300)).unwrap(), 301);
    assert_eq!(pruned_cursor(301, 1000, 2, Some(999)).unwrap(), 1000);
    assert_eq!(pruned_cursor(1000, 1000, 0, None).unwrap(), 1000);
    for value in [
        pruned_cursor(2, 1, 0, None),
        pruned_cursor(0, 1000, 257, Some(999)),
        pruned_cursor(0, 1000, 0, Some(1)),
        pruned_cursor(0, 1000, 1, None),
        pruned_cursor(0, 1000, 1, Some(1000)),
    ] {
        assert!(value.is_err());
    }
}

#[test]
fn retention_configuration_uses_native_seconds_and_handles_integer_overflow() {
    use relay_admission::mls_projection::{ceiling_ms, DEFAULT_PRUNE_CEILING_MS};
    assert_eq!(ceiling_ms(Some("60")), 60000);
    for value in [
        None,
        Some(""),
        Some("-1"),
        Some("0"),
        Some("9223372036854775807"),
    ] {
        assert_eq!(ceiling_ms(value), DEFAULT_PRUNE_CEILING_MS);
    }
}
