use super::super::profile::{AgentKind, Definition, ModelTier};
use super::*;

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/codex_app_server.py"
);

fn fixture(mode: &str) -> (tempfile::TempDir, CodexBrain, PathBuf) {
    fixture_with_expected(mode, Value::Null)
}
fn fixture_with_expected(mode: &str, expected: Value) -> (tempfile::TempDir, CodexBrain, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fake-codex");
    let marker = dir.path().join("turns");
    // Only per-case data is written. The shared executable is checked in and
    // never opened for writing/chmod by concurrent fixture users.
    std::fs::write(
        path.with_extension("json"),
        serde_json::to_vec(&json!({"mode":mode,"marker":marker,"expected":expected})).unwrap(),
    )
    .unwrap();
    std::os::unix::fs::symlink(FIXTURE, &path).unwrap();
    (
        dir,
        CodexBrain {
            binary: path,
            model: None,
            timeout: Duration::from_secs(5),
        },
        marker,
    )
}

async fn fixture_version(brain: &CodexBrain) -> String {
    let mut c = command(&brain.binary);
    c.arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let output = tokio::time::timeout(Duration::from_secs(5), c.output())
        .await
        .expect("fixture version exceeded its setup budget")
        .unwrap_or_else(|error| {
            panic!(
                "fixture spawn failed: kind={:?}, errno={:?}",
                error.kind(),
                error.raw_os_error()
            )
        });
    assert!(
        output.status.success(),
        "fixture version exited unsuccessfully"
    );
    String::from_utf8(output.stdout).unwrap()
}

#[tokio::test]
async fn immutable_fixture_executes_with_open_case_data_and_keeps_concurrent_cases_isolated() {
    let (_a, a, marker_a) = fixture("version");
    let (_b, b, marker_b) = fixture("success");
    let original = std::fs::read(FIXTURE).unwrap();
    assert_eq!(
        original.as_slice(),
        &include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/codex_app_server.py"
        ))[..]
    );
    for brain in [&a, &b] {
        assert!(std::fs::symlink_metadata(&brain.binary)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(
            std::fs::canonicalize(&brain.binary).unwrap(),
            std::fs::canonicalize(FIXTURE).unwrap()
        );
    }
    // Open-for-write case data is distinct from the immutable executable inode.
    // The handles stay live while both real children execute; no data is changed.
    let _open_a = std::fs::OpenOptions::new()
        .write(true)
        .open(a.binary.with_extension("json"))
        .unwrap();
    let _open_b = std::fs::OpenOptions::new()
        .write(true)
        .open(b.binary.with_extension("json"))
        .unwrap();
    let (version_a, version_b) = tokio::join!(fixture_version(&a), fixture_version(&b));
    assert_eq!(version_a, "codex-cli 0.162.0\n");
    assert_eq!(version_b, "codex-cli 0.163.0\n");
    assert_eq!(std::fs::read(FIXTURE).unwrap(), original);
    for marker in [marker_a, marker_b] {
        assert!(!marker.exists());
        assert!(!marker.with_extension("pid").exists());
    }
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn linux_write_open_executable_control_is_busy_until_its_writer_is_closed() {
    async fn isolated_control(directory: &Path) -> Value {
        // The test pool never owns an executable writer: unrelated parallel
        // process spawns cannot inherit one between fork and exec.
        let mut child = tokio::process::Command::new("python3");
        child
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/executable_write_control.py"
            ))
            .arg(FIXTURE)
            .arg(directory)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let output = tokio::time::timeout(Duration::from_secs(5), child.output())
            .await
            .expect("isolated Linux control exceeded its setup budget")
            .expect("isolated Linux control failed to spawn");
        assert!(output.status.success(), "isolated Linux control failed");
        serde_json::from_slice(&output.stdout).expect("invalid Linux control result")
    }
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let original = std::fs::read(FIXTURE).unwrap();
    let (a_result, b_result) = tokio::join!(isolated_control(a.path()), isolated_control(b.path()));
    let expected = json!({
        "busy_errno": 26,
        "inherited_errno": 26,
        "version": "codex-cli 0.162.0\n"
    });
    assert_eq!(a_result, expected);
    assert_eq!(b_result, expected);
    assert_eq!(std::fs::read(FIXTURE).unwrap(), original);
}

fn definition() -> Definition {
    Definition {
        version: 1,
        persona: "Propose presentation edits only".into(),
        rules: vec![],
        state: json!({}),
        kind: AgentKind::User,
        model_tier: ModelTier::Cheap,
    }
}
fn tool() -> ToolDef {
    ToolDef {
        name: "edit_deck".into(),
        description: "Edit one offered deck".into(),
        parameters: json!({"type":"object","properties":{"shapeId":{"type":"integer"},"text":{"type":"string"}},"required":["shapeId","text"]}),
    }
}
async fn think(brain: &CodexBrain) -> Result<BrainOut, String> {
    let def = definition();
    let cx = TurnContext {
        agent: "agent",
        device: "device",
        owner: "owner",
        definition: &def,
    };
    brain
        .think_with_tools(
            cx,
            &[],
            &Turn {
                space: "space".into(),
                from: "owner".into(),
                text: "Edit offered shape".into(),
            },
            &[tool()],
            &[],
        )
        .await
}
#[tokio::test]
async fn isolated_metadata_admission_returns_only_exact_offered_proposal() {
    let (_dir, brain, marker) = fixture("success");
    let BrainOut::CallTools(calls) = think(&brain).await.unwrap() else {
        panic!("expected proposal")
    };
    assert_eq!(
        calls,
        vec![ToolCall {
            requires_review: false,
            id: "call1".into(),
            name: "edit_deck".into(),
            arguments: json!({"shapeId":42,"text":"Real proposal"})
        }]
    );
    assert_eq!(std::fs::read_to_string(marker).unwrap(), "turn\n");
}
#[tokio::test]
async fn api_key_incompatible_version_and_still_enabled_mcp_refuse_before_inference() {
    for mode in ["api-key", "version", "mcp", "notify"] {
        let (_dir, brain, marker) = fixture(mode);
        assert!(think(&brain).await.is_err(), "{mode}");
        assert!(!marker.exists(), "no inference for {mode}");
    }
}
#[tokio::test]
async fn unknown_tools_server_requests_and_incomplete_turns_never_escape_as_proposals() {
    for mode in ["unknown-tool", "request", "failed", "interrupted"] {
        let (_dir, brain, marker) = fixture(mode);
        assert!(think(&brain).await.is_err(), "{mode}");
        assert!(marker.exists());
    }
}
#[tokio::test]
async fn output_bound_is_enforced_before_a_missing_newline_allocates_unbounded_memory() {
    let (_dir, brain, _) = fixture("oversize");
    assert_eq!(
        think(&brain).await.unwrap_err(),
        "Codex output exceeds limit"
    );
}
#[tokio::test]
async fn timeout_retires_an_actual_stalled_child_process() {
    let (_dir, brain, marker) = fixture("hang");
    let work = Work::new().unwrap();
    // Establish the real child independently of the inference timeout. A slow
    // interpreter/admission must not masquerade as a started, stalled session.
    let mut session =
        tokio::time::timeout(brain.timeout, Session::start(&brain.binary, &work.0, None))
            .await
            .expect("fixture session startup exceeded its harness budget")
            .unwrap();
    let pid = session.child.id().expect("fixture child has an owned PID");
    assert_eq!(
        std::fs::read_to_string(marker.with_extension("pid"))
            .unwrap()
            .parse::<u32>()
            .unwrap(),
        pid
    );
    assert!(
        process_alive(pid),
        "fixture child must be alive before timeout"
    );
    let result = tokio::time::timeout(Duration::from_millis(500), async move {
        session
            .rpc(
                "turn/start",
                json!({"threadId":"thread-fixture","environments":[],
                    "outputSchema":output_schema(&[tool()])}),
            )
            .await
    })
    .await;
    assert!(
        result.is_err(),
        "the stalled RPC must expire, not fail startup"
    );
    assert_eq!(std::fs::read_to_string(marker).unwrap(), "turn\n");
    assert_retired(pid).await;
}

#[tokio::test]
async fn full_brain_timeout_bounds_admission_without_assuming_a_started_session() {
    let (_dir, mut brain, marker) = fixture("hang");
    brain.timeout = Duration::from_millis(500);
    assert_eq!(
        think(&brain).await.unwrap_err(),
        "Codex local inference timed out"
    );
    // This timeout includes version and admission subprocesses. A PID marker
    // may not exist yet; only the prestarted-session test proves a stalled child.
    match std::fs::read_to_string(marker.with_extension("pid")) {
        Ok(pid) => assert_retired(pid.parse().unwrap()).await,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => panic!("cannot read fixture PID marker: {error}"),
    }
}

fn process_alive(pid: u32) -> bool {
    std::process::Command::new("/bin/kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap()
        .success()
}

async fn assert_retired(pid: u32) {
    let until = tokio::time::Instant::now() + Duration::from_secs(2);
    while process_alive(pid) {
        assert!(
            tokio::time::Instant::now() < until,
            "stalled child remained alive"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
#[test]
fn malformed_ambiguous_unoffered_and_duplicate_calls_are_refused() {
    for output in [
        json!({"outcome":{"kind":"call","text":"speak and call","calls":[]}}),
        json!({"outcome":{"kind":"call","calls":[{"id":"same","name":"edit_deck","arguments_json":"{}"},{"id":"same","name":"edit_deck","arguments_json":"{}"}]}}),
        json!({"outcome":{"kind":"call","calls":[{"id":"one","name":"edit_deck","arguments_json":"[]"}]}}),
        json!({"outcome":{"kind":"say","text":"ok"},"extra":true}),
    ] {
        assert!(parse_output(&output.to_string(), &[tool()]).is_err());
    }
    assert_eq!(
        parse_output(r#"{"outcome":{"kind":"quiet"}}"#, &[]).unwrap(),
        BrainOut::Silence
    );
}

#[tokio::test]
async fn notifications_overtaking_turn_admission_are_not_dropped_or_misclassified() {
    let (_dir, brain, _) = fixture("overtake");
    assert!(matches!(think(&brain).await.unwrap(),BrainOut::CallTools(calls)if calls.len()==1));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore = "explicit existing operator-signed WASI pins; fake Codex only, no provider login"]
async fn actual_wasi_read_metadata_reaches_fake_codex_completed_tool_steps_exactly() {
    use crate::deckcraft::{DeckcraftWorker, ENGINE_REVISION};
    use crate::SignedManifest;
    use ed25519_dalek::VerifyingKey;
    use sha2::{Digest, Sha256};

    // Public deployment pins only. No fixture signing key or fallback component.
    let component = std::env::var("ZOEN_DECKCRAFT_WORKER_COMPONENT").unwrap();
    let manifest = std::env::var("ZOEN_DECKCRAFT_WORKER_MANIFEST").unwrap();
    let publisher = std::env::var("ZOEN_DECKCRAFT_WORKER_PUBLISHER").unwrap();
    let key: [u8; 32] = hex::decode(publisher).unwrap().try_into().unwrap();
    let signed: SignedManifest = serde_json::from_slice(&std::fs::read(manifest).unwrap()).unwrap();
    let worker = DeckcraftWorker::load(
        vec![VerifyingKey::from_bytes(&key).unwrap()],
        &signed,
        &std::fs::read(component).unwrap(),
    )
    .unwrap();
    let bytes = include_bytes!("../../../../miniapps/resources/deckcraft.empty.deckcraft");
    let scope = json!({"artifactId":"it_deck_codex_read_fixture","actorId":"synthetic_agent","actorKind":"agent"});
    let head = json!({"version":"1","sha256":hex::encode(Sha256::digest(bytes))});
    let reply = worker
        .run(
            &json!({"schema":1,"open":{"scope":scope,"head":head},"command":null}),
            bytes,
        )
        .await
        .unwrap();
    assert!(reply.bytes.is_empty());
    assert_eq!(reply.metadata["scope"], scope);
    assert_eq!(reply.metadata["head"], head);
    assert_eq!(reply.metadata["engineRevision"], ENGINE_REVISION);
    assert_eq!(reply.metadata["slides"].as_array().unwrap().len(), 1);
    let native_shape = reply.metadata["slides"][0]["shapes"][0]["id"]
        .as_u64()
        .unwrap();
    let step = ToolStep {
        call: ToolCall {
            requires_review: false,
            id: "accepted-synthetic-read".into(),
            name: "read_deck".into(),
            arguments: json!({"artifactId":"it_deck_codex_read_fixture"}),
        },
        result: reply.metadata.to_string(),
        is_error: false,
    };
    let expected = json!({"steps":[step],"read":reply.metadata,"scope":scope,"head":head});
    let (_dir, brain, marker) = fixture_with_expected("after-tools", expected.clone());
    let def = definition();
    let cx = TurnContext {
        agent: "synthetic_agent",
        device: "synthetic_device",
        owner: "synthetic_owner",
        definition: &def,
    };
    let inbound = Turn {
        space: "synthetic_space".into(),
        from: "synthetic_owner".into(),
        text: "Use the existing read context for the next proposal.".into(),
    };
    let out = brain
        .think_with_tools(cx, &[], &inbound, &[tool()], std::slice::from_ref(&step))
        .await
        .unwrap();
    assert_eq!(
        out,
        BrainOut::CallTools(vec![ToolCall {
            requires_review: false,
            id: "after-read".into(),
            name: "edit_deck".into(),
            arguments: json!({"shapeId":native_shape,"text":"Synthetic next step"}),
        }])
    );
    assert_eq!(std::fs::read_to_string(marker).unwrap(), "turn\n");

    // The same executable assertion must reject a receipt-only replacement;
    // otherwise a green test would not detect the payload-loss hypothesis.
    let (_dir, brain, _) = fixture_with_expected("after-tools", expected);
    let mut receipt_only = step;
    receipt_only.result =
        json!({"scope":scope,"head":head,"engineRevision":ENGINE_REVISION}).to_string();
    assert!(brain
        .think_with_tools(cx, &[], &inbound, &[tool()], &[receipt_only])
        .await
        .is_err());
}

#[path = "tests/explicit_review.rs"]
mod explicit_review;

#[path = "tests/output_contract.rs"]
mod output_contract;
