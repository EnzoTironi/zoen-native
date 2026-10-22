use super::*;
struct Synthetic {
    calls: Arc<AtomicUsize>,
    fail: bool,
}
impl DecisionProvider for Synthetic {
    fn name(&self) -> String {
        "synthetic-no-io".into()
    }
    fn on_device(&self) -> bool {
        true
    }
    fn decide(&self, _: &DecisionRequest) -> Result<DecisionResponse, GateError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            Err(GateError::Unavailable("synthetic".into()))
        } else {
            Ok(DecisionResponse::default())
        }
    }
}
fn request() -> DecisionRequest {
    DecisionRequest {
        input: "synthetic".into(),
        questions: vec![],
    }
}
fn provider(budget: Arc<Budget>, calls: Arc<AtomicUsize>, fail: bool) -> BoundedClef {
    BoundedClef {
        inner: Box::new(Synthetic { calls, fail }),
        budget,
    }
}
fn expected() -> Expected {
    Expected {
        item: "synthetic-deck".into(),
        version: 1,
        head: "11".repeat(32),
        worker_binding: "88".repeat(32),
    }
}
#[test]
fn one_attempt_marker_refuses_reentry_before_any_provider_setup() {
    let path = std::env::temp_dir().join(types::new_id("canonical-provider-one-attempt"));
    std::fs::create_dir(&path).unwrap();
    mark_once(&path).unwrap();
    assert_eq!(
        mark_once(&path).unwrap_err().kind(),
        std::io::ErrorKind::AlreadyExists
    );
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn reservation_is_persisted_before_the_delegate_observes_the_request() {
    struct ObservesReservation(std::path::PathBuf);
    impl DecisionProvider for ObservesReservation {
        fn name(&self) -> String {
            "synthetic-observer".into()
        }
        fn on_device(&self) -> bool {
            true
        }
        fn decide(&self, _: &DecisionRequest) -> Result<DecisionResponse, GateError> {
            let saved: Value = serde_json::from_slice(&std::fs::read(&self.0).unwrap()).unwrap();
            assert_eq!(saved["clefDispatches"], 1);
            assert_eq!(saved["trace"][0]["state"], "started");
            Ok(DecisionResponse::default())
        }
    }
    let path = std::env::temp_dir().join(types::new_id("canonical-provider-reservation"));
    std::fs::create_dir(&path).unwrap();
    let budget = Budget::with_evidence(&path);
    BoundedClef {
        inner: Box::new(ObservesReservation(path.join("provider-trace.json"))),
        budget,
    }
    .decide(&request())
    .unwrap();
    std::fs::remove_dir_all(path).unwrap();
}
#[tokio::test]
async fn signed_fixture_transfer_uses_the_real_owner_command_and_recovers_the_same_approved_request(
) {
    let crate::deckcraft::tests::Fixture {
        path,
        client,
        vault,
        owner_vault,
        owner,
        source,
        base,
    } = crate::deckcraft::tests::fixture();
    let account = client.account().unwrap();
    let space = client.item(base.item_id.clone()).unwrap().space_id;
    let turn = client
        .agent_binary_turn(&base.item_id, &source, &owner, None, None)
        .unwrap();
    let inbound = client
        .timeline(space.clone())
        .unwrap()
        .into_iter()
        .find(|e| e.id == source)
        .unwrap();
    let mut definition = crate::deckcraft::tools()
        .into_iter()
        .find(|t| t.name == "read_deckcraft")
        .unwrap();
    definition.name = crate::deckcraft::model_name("read_deckcraft", &base.item_id).unwrap();
    let arguments = json!({"artifactId":base.item_id});
    let continuation = json!({"version":1,"policy_sha256":"77".repeat(32),
            "envelope":{"agent":account.identity_id,"device":account.device_id,"owner":owner,"space":space,"from":owner,"inbound_sha256":sha(text_of(&inbound).unwrap().as_bytes())},
            "call":{"id":"synthetic-owner-api-read","name":definition.name,"arguments":arguments},"definition":definition,
            "steps":[],"remaining":[],"round":0,"rule":"model:low-confidence"});
    let request = client
        .stage_tool_approval(
            &turn,
            client_core::AgentToolApprovalInput {
                canonical_tool: "read_deckcraft".into(),
                arguments,
                worker_binding: "88".repeat(32),
                continuation_json: continuation.to_string(),
                gate_rule: "model:low-confidence".into(),
            },
        )
        .unwrap();
    drop(client);
    approve_as_owner(&path, owner_vault, &space, &request).await;
    let reopened = canonical_tool_approval_tests::reopen(&path, vault);
    let approved = reopened
        .requests()
        .into_iter()
        .find(|r| r.id == request.id)
        .unwrap();
    assert_eq!(approved.status, client_core::RequestStatus::Approved);
    assert_eq!(
        approved.tool_approval.unwrap().content_hash,
        request.tool_approval.unwrap().content_hash
    );
    assert_eq!(
        reopened
            .app_binary_read(base.item_id, None, None)
            .unwrap()
            .version,
        1
    );
    drop(reopened);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn clef_concurrent_reservation_refuses_before_a_fifth_delegate() {
    let budget = Budget::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let provider = Arc::new(provider(budget.clone(), calls.clone(), false));
    let threads: Vec<_> = (0..12)
        .map(|_| {
            let p = provider.clone();
            std::thread::spawn(move || p.decide(&request()).is_ok())
        })
        .collect();
    assert_eq!(
        threads
            .into_iter()
            .map(|t| t.join().unwrap())
            .filter(|ok| *ok)
            .count(),
        4
    );
    assert_eq!(calls.load(Ordering::SeqCst), 4);
    assert_eq!(budget.snapshot()["clefDispatches"], 4);
}
#[test]
fn provider_failures_and_wrapper_recreation_never_refund_a_reserved_dispatch() {
    let budget = Budget::new();
    let calls = Arc::new(AtomicUsize::new(0));
    for _ in 0..5 {
        assert!(provider(budget.clone(), calls.clone(), true)
            .decide(&request())
            .is_err());
    }
    assert_eq!(calls.load(Ordering::SeqCst), 4);
    assert_eq!(budget.snapshot()["refusedBeforeDelegation"], 1);
}
#[test]
fn expired_deadline_refuses_both_kinds_before_delegation() {
    let budget = Budget {
        deadline: Instant::now(),
        counts: Mutex::default(),
        evidence: None,
    };
    assert!(budget.reserve("clef").is_err());
    assert!(budget.reserve("codex").is_err());
    assert_eq!(budget.snapshot()["clefDispatches"], 0);
    assert_eq!(budget.snapshot()["codexTurnsStarted"], 0);
}
#[tokio::test]
async fn brain_failures_and_rewrapped_context_keep_one_cumulative_three_turn_budget() {
    let budget = Budget::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let definition = definition();
    let cx = TurnContext {
        agent: "synthetic",
        device: "synthetic",
        owner: "synthetic",
        definition: &definition,
    };
    let inbound = Turn {
        space: "synthetic".into(),
        from: "synthetic".into(),
        text: "synthetic".into(),
    };
    for _ in 0..4 {
        let counter = calls.clone();
        let inner = zoen_agentd::citizen::testing::ControlledToolBrain {
            evaluate: Box::new(move |_| {
                counter.fetch_add(1, Ordering::SeqCst);
                Err("synthetic failure".into())
            }),
        };
        assert!(
            guarded(Box::new(inner), &budget, expected(), "88".repeat(32))
                .think(cx, &[], &inbound)
                .await
                .is_err()
        );
    }
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    assert_eq!(budget.snapshot()["codexTurnsStarted"], 3);
}
#[test]
fn model_route_guard_rejects_extra_calls_foreign_arguments_and_repeated_steps() {
    let expected = expected();
    let read = expected.read();
    assert!(expected
        .admit(
            &expected.worker_binding,
            &[],
            &BrainOut::CallTools(vec![read.clone()])
        )
        .is_ok());
    assert!(expected
        .admit(
            &expected.worker_binding,
            &[],
            &BrainOut::CallTools(vec![read.clone(), read.clone()])
        )
        .is_err());
    let mut foreign = read.clone();
    foreign.arguments["artifactId"] = json!("foreign");
    assert!(expected
        .admit(
            &expected.worker_binding,
            &[],
            &BrainOut::CallTools(vec![foreign])
        )
        .is_err());
    let steps = vec![verified_read(&expected)];
    assert!(expected
        .admit(
            &expected.worker_binding,
            &steps,
            &BrainOut::CallTools(vec![native_write(&expected, json!(257))])
        )
        .is_ok());
    assert!(expected
        .admit(
            &expected.worker_binding,
            &steps,
            &BrainOut::CallTools(vec![read])
        )
        .is_err());
    let mut stale = native_write(&expected, json!(257));
    stale.arguments["version"] = json!(2);
    assert!(expected
        .admit(
            &expected.worker_binding,
            &steps,
            &BrainOut::CallTools(vec![stale])
        )
        .is_err());
}
fn verified_read(expected: &Expected) -> ToolStep {
    ToolStep {
        call: expected.read(),
        result: json!({"engineRevision":zoen_agentd::deckcraft::ENGINE_REVISION,
                "scope":{"artifactId":expected.item},
                "head":{"version":expected.version.to_string(),"sha256":expected.head},
                "slides":[{"id":257}]})
        .to_string(),
        is_error: false,
    }
}
fn native_write(expected: &Expected, slide: Value) -> ToolCall {
    ToolCall {
        id: "independent-write-proposal".into(),
        name: crate::deckcraft::model_name("deckcraft_slide_notes", &expected.item).unwrap(),
        arguments: json!({"artifactId":expected.item,"version":expected.version,"sha256":expected.head,"slideId":slide,"text":NOTES}),
    }
}
#[test]
fn native_id_guard_accepts_observed_257_and_rejects_positional_unknown_missing_or_unread_ids() {
    let expected = expected();
    let read = verified_read(&expected);
    let valid = native_write(&expected, json!(257));
    assert!(expected
        .admit(
            &expected.worker_binding,
            std::slice::from_ref(&read),
            &BrainOut::CallTools(vec![valid.clone()])
        )
        .is_ok());
    for id in [json!(1), json!(258), Value::Null, json!("257"), json!(-1)] {
        let proposal = native_write(&expected, id);
        assert!(expected
            .admit(
                &expected.worker_binding,
                std::slice::from_ref(&read),
                &BrainOut::CallTools(vec![proposal])
            )
            .is_err());
    }
    let mut missing = valid.clone();
    missing.arguments.as_object_mut().unwrap().remove("slideId");
    assert!(expected
        .admit(
            &expected.worker_binding,
            std::slice::from_ref(&read),
            &BrainOut::CallTools(vec![missing])
        )
        .is_err());
    assert!(expected
        .admit(
            &expected.worker_binding,
            &[],
            &BrainOut::CallTools(vec![valid])
        )
        .is_err());
}
#[test]
fn native_id_guard_rejects_unverified_head_artifact_engine_worker_and_read_shape() {
    let expected = expected();
    let read = verified_read(&expected);
    let valid = native_write(&expected, json!(257));
    for pointer in [
        "/head/sha256",
        "/head/version",
        "/scope/artifactId",
        "/engineRevision",
        "/slides/0/id",
    ] {
        let mut changed: Value = serde_json::from_str(&read.result).unwrap();
        *changed.pointer_mut(pointer).unwrap() = Value::Null;
        let mut step = read.clone();
        step.result = changed.to_string();
        assert!(
            expected
                .admit(
                    &expected.worker_binding,
                    &[step],
                    &BrainOut::CallTools(vec![valid.clone()])
                )
                .is_err(),
            "{pointer}"
        );
    }
    for id in [json!(-1), json!(u64::MAX), json!(1.5), json!("257")] {
        let mut changed: Value = serde_json::from_str(&read.result).unwrap();
        changed["slides"][0]["id"] = id;
        let mut step = read.clone();
        step.result = changed.to_string();
        assert!(expected
            .admit(
                &expected.worker_binding,
                &[step],
                &BrainOut::CallTools(vec![valid.clone()])
            )
            .is_err());
    }
    let mut changed = valid.clone();
    changed.arguments["sha256"] = json!("00".repeat(32));
    assert!(expected
        .admit(
            &expected.worker_binding,
            std::slice::from_ref(&read),
            &BrainOut::CallTools(vec![changed])
        )
        .is_err());
    assert!(expected
        .admit(
            &"99".repeat(32),
            std::slice::from_ref(&read),
            &BrainOut::CallTools(vec![valid])
        )
        .is_err());
}
#[tokio::test]
async fn rejected_proposal_persists_only_reason_shape_flags_and_cumulative_counts() {
    let expected = expected();
    let read = verified_read(&expected);
    let mut candidate = native_write(&expected, json!(1));
    candidate.arguments["untrusted-key-DO_NOT_LOG"] = json!("untrusted-value-DO_NOT_LOG");
    let path = std::env::temp_dir().join(types::new_id("redacted-provider-rejection"));
    std::fs::create_dir(&path).unwrap();
    let budget = Budget::with_evidence(&path);
    let inner = zoen_agentd::citizen::testing::ControlledToolBrain {
        evaluate: Box::new(move |_| Ok(BrainOut::CallTools(vec![candidate.clone()]))),
    };
    let definition = definition();
    let cx = TurnContext {
        agent: "synthetic",
        device: "synthetic",
        owner: "synthetic",
        definition: &definition,
    };
    let inbound = Turn {
        space: "synthetic".into(),
        from: "synthetic".into(),
        text: "synthetic".into(),
    };
    let result = guarded(Box::new(inner), &budget, expected, "88".repeat(32))
        .think_with_tools(cx, &[], &inbound, &[], &[read])
        .await;
    assert_eq!(result, Err("acceptance guard: write-arguments".into()));
    let saved = std::fs::read_to_string(path.join("provider-trace.json")).unwrap();
    assert!(!saved.contains("DO_NOT_LOG"));
    let value: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(value["codexTurnsStarted"], 1);
    assert_eq!(value["clefDispatches"], 0);
    assert_eq!(value["trace"][1]["state"], "rejected-synthetic-shape");
    assert_eq!(value["trace"][1]["shape"]["calls"][0]["nativeSlideId"], 1);
    assert_eq!(value["trace"][1]["shape"]["readFirstNativeSlideId"], 257);
    std::fs::remove_dir_all(path).unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "offline only; requires the actual operator-signed pinned WASI worker"]
async fn native_slide_id_from_actual_signed_worker_is_admitted() {
    let worker = crate::deckcraft::configured()
        .unwrap()
        .expect("explicit signed worker required");
    let fixture = crate::deckcraft::tests::fixture();
    let result = crate::deckcraft::tests::inspect(&fixture, &worker).await;
    assert_eq!(result["slides"][0]["id"], json!(257));
    let expected = Expected {
        item: fixture.base.item_id.clone(),
        version: fixture.base.version,
        head: fixture.base.sha256.clone(),
        worker_binding: worker.binding_sha256().into(),
    };
    let read = ToolStep {
        call: expected.read(),
        result: result.to_string(),
        is_error: false,
    };
    let write = native_write(&expected, result["slides"][0]["id"].clone());
    let admitted = expected.admit(
        worker.binding_sha256(),
        &[read],
        &BrainOut::CallTools(vec![write]),
    );
    println!("actual signed worker read nativeSlideId=257; guarded single-write admission={admitted:?}; no provider dispatch");
    let path = fixture.path.clone();
    drop(fixture);
    std::fs::remove_dir_all(path).unwrap();
    assert!(
        admitted.is_ok(),
        "the verified native first-slide ID must be admitted: {admitted:?}"
    );
}
