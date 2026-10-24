use super::*;

fn missing_receiver(staged: &StagedRecovery, space: &str) -> Engine {
    let mut engine = restored_receiver(staged);
    engine.net.synced.insert(space.into());
    assert!(engine.wanted_recovery_contexts().is_empty());
    assert!(!engine.mls_recovery_pass().unwrap());
    assert_eq!(engine.wanted_recovery_contexts().len(), 1);
    engine
}

#[test]
fn failed_download_keeps_candidate_until_valid_bytes_arrive() {
    let space = "download-retry";
    let staged = staged_recovery(":memory:", space);
    let mut engine = missing_receiver(&staged, space);
    let reference = engine.wanted_recovery_contexts().remove(0);
    assert!(!engine.recovery_context_arrived(&reference, b"invalid bytes"));
    assert!(!engine.mls_recovery_pass().unwrap());
    assert_eq!(
        engine.wanted_recovery_contexts(),
        std::slice::from_ref(&reference)
    );
    assert!(engine.wants_recovery_context(&reference));
    assert!(engine.recovery_context_arrived(&reference, &staged.context_blob));
    assert!(engine.wanted_recovery_contexts().is_empty());
    assert!(engine.mls_recovery_pass().unwrap());
    assert!(engine.device().unwrap().recovery_pending(space));
}

#[test]
fn changed_context_rejects_old_candidate_and_inflight_response() {
    let space = "download-context-change";
    let staged = staged_recovery(":memory:", space);
    let mut engine = missing_receiver(&staged, space);
    let original = engine.wanted_recovery_contexts().remove(0);
    // Exercise every signed-reference field, including a same-blob change.
    for changed in [
        ContextRef {
            epoch: original.epoch + 1,
            ..original.clone()
        },
        ContextRef {
            device: "replacement-device".into(),
            ..original.clone()
        },
        ContextRef {
            blob: "0".repeat(64),
            ..original.clone()
        },
    ] {
        engine
            .store
            .set_meta(
                &context_meta(space),
                &serde_json::to_string(&changed).unwrap(),
            )
            .unwrap();
        assert!(engine.wanted_recovery_contexts().is_empty());
        assert!(!engine.wants_recovery_context(&original));
        assert!(!engine.recovery_context_arrived(&original, &staged.context_blob));
        assert!(engine
            .store
            .recovery_blob(space, &original.blob)
            .unwrap()
            .is_none());
        engine.net.mls.wake_recovery(space);
        assert!(!engine.mls_recovery_pass().unwrap());
        assert_eq!(engine.wanted_recovery_contexts(), [changed]);
        engine
            .net
            .mls
            .recovery_downloads
            .insert(space.into(), original.clone());
    }
}

#[test]
fn ready_group_rejects_stale_candidate_and_inflight_response() {
    let space = "download-ready-change";
    let staged = staged_recovery(":memory:", space);
    let mut engine = missing_receiver(&staged, space);
    let reference = engine.wanted_recovery_contexts().remove(0);
    engine.create_mls_group(space).unwrap();
    assert_eq!(
        engine.device().unwrap().group_state(space).unwrap(),
        GroupState::Ready
    );
    assert!(engine.wanted_recovery_contexts().is_empty());
    assert!(!engine.wants_recovery_context(&reference));
    assert!(!engine.recovery_context_arrived(&reference, &staged.context_blob));
    assert!(engine
        .store
        .recovery_blob(space, &reference.blob)
        .unwrap()
        .is_none());
}

#[test]
fn removed_membership_rejects_stale_candidate_and_inflight_response() {
    let space = "download-member-change";
    let staged = staged_recovery(":memory:", space);
    let mut engine = missing_receiver(&staged, space);
    let reference = engine.wanted_recovery_contexts().remove(0);
    let me = engine.me.clone().unwrap();
    engine
        .state
        .spaces
        .get_mut(space)
        .unwrap()
        .members
        .retain(|(identity, _)| identity != &me);
    assert!(engine.wanted_recovery_contexts().is_empty());
    assert!(!engine.wants_recovery_context(&reference));
    assert!(!engine.recovery_context_arrived(&reference, &staged.context_blob));
    assert!(engine
        .store
        .recovery_blob(space, &reference.blob)
        .unwrap()
        .is_none());
}
