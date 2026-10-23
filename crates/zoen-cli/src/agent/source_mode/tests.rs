use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use zoen_agentd::citizen::{testing::ControlledToolBrain, BrainOut};
mod fixture;
mod relay;
mod turn_budget;
use fixture::{message, Fixture};
use relay::{online, relay, Mode};

fn source(id: &str) -> Source {
    Source { id: id.into() }
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(3)
}
fn definition() -> Definition {
    serde_json::from_value(json!({"version":1,"kind":"user","persona":"Synthetic source fixture","rules":[],"state":{}})).unwrap()
}
fn context(f: &Fixture, calls: &Arc<AtomicUsize>, fail: bool) -> Ctx {
    let account = f.client.account().unwrap();
    let calls = calls.clone();
    Ctx {
        me: account.identity_id,
        device: account.device_id,
        owner: f.owner.clone(),
        runtime: Runtime::new(
            Gate::default(),
            Box::new(ControlledToolBrain {
                evaluate: Box::new(move |_| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    if fail {
                        Err("synthetic unknown outcome".into())
                    } else {
                        Ok(BrainOut::Silence)
                    }
                }),
            }),
        ),
        deckcraft: None,
        journal: f.path.join("source-steps.json"),
    }
}

#[test]
fn selector_requires_exact_id_and_explicit_positive_bound_without_changing_normal_args() {
    let parse =
        |args: &[&str]| Source::parse(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>());
    assert!(parse(&["agent-host", "--for", "legacy-invalid"])
        .unwrap()
        .is_none());
    assert_eq!(
        parse(&[
            "--home",
            "/owned",
            "agent-host",
            "--source",
            "01SIGNED",
            "--for",
            "240"
        ])
        .unwrap()
        .unwrap()
        .id,
        "01SIGNED"
    );
    for args in [
        vec!["agent-host", "--source=id", "--for", "1"],
        vec!["agent-host", "--source", "id"],
        vec!["agent-host", "--source", "id", "--for", "0"],
        vec!["agent-host", "--source", "id", "--for", "bad"],
        vec![
            "agent-host",
            "--source",
            "id",
            "--for",
            "18446744073709551615",
        ],
        vec![
            "agent-host",
            "--source",
            "id",
            "--for",
            "1",
            "--source",
            "other",
        ],
        vec!["agent-host", "--source", "prefix*", "--for", "1"],
        vec!["agent-host", "--source", "id", "--for", "1", "--offline"],
        vec![
            "agent-host",
            "--source",
            "id",
            "--for",
            "1",
            "--timeout",
            "invalid",
        ],
    ] {
        assert!(parse(&args).is_err(), "{args:?}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unsettled_source_admission_never_reads_state_or_seeds_a_cursor() {
    let server = relay(Mode::Unsynced).await;
    let f = Fixture::new(&server.url);
    online(&f.client, false).await;
    assert!(startup::settled_cursors(&f.client, 100).await.is_err());
    assert!(!server.calls.lock().unwrap().contains(&"agent_state"));
    assert!(!server
        .calls
        .lock()
        .unwrap()
        .contains(&"forbidden_agent_operation"));
    assert!(server.cursors.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn missing_target_refuses_before_provider_construction_and_never_seeds_or_publishes_ready() {
    let server = relay(Mode::Normal).await;
    let f = Fixture::new(&server.url);
    assert_eq!(
        Source::preflight(&f.client),
        Err("agent_host_source_account_unavailable".into())
    );
    online(&f.client, true).await;
    let lock = crate::agent_host_lock::HostLock::acquire(&f.path).unwrap();
    let ready = startup::Readiness::reset(&f.path, &lock).unwrap();
    let error = host(&f.client, &f.path, &ready, Some(1), Some(source("missing")))
        .await
        .unwrap_err();
    assert_eq!(error, "agent_host_source_unavailable_in_projection");
    assert!(!f.path.join("agent-ready").exists());
    assert!(!f.path.join("agent-tool-steps.json").exists());
    assert!(server.cursors.lock().unwrap().is_empty());
    assert!(!server
        .calls
        .lock()
        .unwrap()
        .contains(&"forbidden_agent_operation"));
    assert!(!server.calls.lock().unwrap().contains(&"read_receipt"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unknown_state_and_definition_metadata_mismatch_refuse_before_providers_or_readiness() {
    for (mode, expected) in [
        (Mode::InvalidStatus, "agent_host_source_invalid_state"),
        (Mode::InvalidKind, "agent_host_source_invalid_state"),
        (
            Mode::DefinitionKindMismatch,
            "agent_host_source_definition_mismatch",
        ),
        (
            Mode::DefinitionVersionMismatch,
            "agent_host_source_definition_mismatch",
        ),
    ] {
        let server = relay(mode).await;
        let f = Fixture::new(&server.url);
        online(&f.client, true).await;
        let lock = crate::agent_host_lock::HostLock::acquire(&f.path).unwrap();
        let ready = startup::Readiness::reset(&f.path, &lock).unwrap();
        assert_eq!(
            host(&f.client, &f.path, &ready, Some(1), Some(source(&f.source))).await,
            Err(expected.into())
        );
        assert!(!f.path.join("agent-ready").exists());
        assert!(!f.path.join("agent-tool-steps.json").exists());
        assert!(server.cursors.lock().unwrap().is_empty());
        assert!(!server
            .calls
            .lock()
            .unwrap()
            .contains(&"forbidden_agent_operation"));
        assert!(!server.calls.lock().unwrap().contains(&"read_receipt"));
    }
    // The same production validator is applied to every status poll, before turn.
    let bytes = definition().encode().unwrap();
    assert!(super::definition("future-status", "User", 1, &bytes).is_err());
    assert!(super::definition("active", "future-kind", 1, &bytes).is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_signed_projection_selects_one_source_and_preserves_all_cursors_and_read_state() {
    let server = relay(Mode::Normal).await;
    let mut f = Fixture::new(&server.url);
    f.append(message("Newer source also must not execute.", None), true);
    online(&f.client, true).await;
    server.cursors.lock().unwrap().push((f.space.clone(), 1));
    let before = startup::settled_cursors(&f.client, 1000).await.unwrap();
    server.calls.lock().unwrap().clear();
    let mut run = Run::prepare(&f.client, source(&f.source)).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let cx = context(&f, &calls, false);
    for _ in 0..3 {
        assert!(!run
            .tick(&f.client, &cx, &definition(), deadline())
            .await
            .unwrap());
    }
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "handled silence must not become repeated inference"
    );
    assert_eq!(before, *server.cursors.lock().unwrap());
    assert!(!server
        .calls
        .lock()
        .unwrap()
        .contains(&"forbidden_agent_operation"));
    assert!(!server.calls.lock().unwrap().contains(&"read_receipt"));
    assert!(!f.path.join("agent-ready").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn selected_turn_refuses_another_valid_signed_message_before_runtime() {
    let server = relay(Mode::Normal).await;
    let f = Fixture::new(&server.url);
    online(&f.client, true).await;
    let run = Run::prepare(&f.client, source(&f.source)).unwrap();
    let timeline = f.client.timeline(f.space.clone()).unwrap();
    let entry = timeline.iter().find(|entry| entry.id == f.other).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    assert!(
        super::super::turn::selected(
            &f.client,
            &context(&f, &calls, false),
            &definition(),
            &f.space,
            &[],
            entry,
            &run.bound
        )
        .await
        .outcome
            == super::super::turn::Outcome::Retry
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(!f.path.join("source-steps.json").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unknown_outcome_is_not_automatically_retried_on_a_wake() {
    let server = relay(Mode::Normal).await;
    let f = Fixture::new(&server.url);
    online(&f.client, true).await;
    let mut run = Run::prepare(&f.client, source(&f.source)).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let cx = context(&f, &calls, true);
    assert_eq!(
        run.tick(&f.client, &cx, &definition(), deadline()).await,
        Err("agent_host_source_outcome_unknown".into())
    );
    // The host exits on Err. Even an erroneous extra wake cannot replay this token.
    assert!(!run
        .tick(&f.client, &cx, &definition(), deadline())
        .await
        .unwrap());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(server.cursors.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn account_retirement_during_inference_prevents_result_delivery() {
    let server = relay(Mode::Normal).await;
    let f = Fixture::new(&server.url);
    online(&f.client, true).await;
    let mut run = Run::prepare(&f.client, source(&f.source)).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut cx = context(&f, &calls, false);
    let client = f.client.clone();
    let observed = calls.clone();
    cx.runtime = Runtime::new(
        Gate::default(),
        Box::new(ControlledToolBrain {
            evaluate: Box::new(move |_| {
                observed.fetch_add(1, Ordering::SeqCst);
                client.reset_demo().unwrap(); // own synthetic account only
                Ok(BrainOut::Say(
                    "Must never be sent after account retirement.".into(),
                ))
            }),
        }),
    );
    assert_eq!(
        run.tick(&f.client, &cx, &definition(), deadline()).await,
        Err("agent_host_source_account_unavailable".into())
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(!f.path.join("source-steps.json").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_existing_accepted_own_reply_completes_without_model_or_cursor_effects() {
    let server = relay(Mode::Normal).await;
    let mut f = Fixture::new(&server.url);
    online(&f.client, true).await;
    let mut body = message("Existing accepted reply.", None);
    let types::EventBody::MessagePosted { reply, .. } = &mut body else {
        unreachable!()
    };
    *reply = Some(types::ReplyRef::inline(f.source.clone()));
    let reply_id = f.append(body, false);
    let reply = f
        .client
        .timeline(f.space.clone())
        .unwrap()
        .into_iter()
        .find(|entry| entry.id == reply_id)
        .unwrap();
    assert_eq!(reply.delivery, client_core::Delivery::Sent);
    *server.delivered.lock().unwrap() = Some((f.space.clone(), f.owner.clone(), reply.seq));
    online(&f.client, true).await;
    assert_eq!(
        f.client
            .timeline(f.space.clone())
            .unwrap()
            .into_iter()
            .find(|entry| entry.id == reply_id)
            .unwrap()
            .delivery,
        client_core::Delivery::Delivered
    );
    let mut run = Run::prepare(&f.client, source(&f.source)).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    assert!(run
        .tick(
            &f.client,
            &context(&f, &calls, true),
            &definition(),
            deadline()
        )
        .await
        .unwrap());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(server.cursors.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn deleted_empty_self_and_nonmessage_sources_are_refused() {
    let server = relay(Mode::Normal).await;
    let mut f = Fixture::new(&server.url);
    online(&f.client, true).await;
    let empty = f.append(message(" \n ", None), true);
    assert_eq!(
        Run::prepare(&f.client, source(&empty)).err().unwrap(),
        "agent_host_source_not_text_message"
    );
    let own = f.append(message("Own message cannot become a task.", None), false);
    assert_eq!(
        Run::prepare(&f.client, source(&own)).err().unwrap(),
        "agent_host_source_member_unavailable"
    );
    f.append(
        types::EventBody::MessageAnnotated {
            target: f.source.clone(),
            note: types::Annotation::Delete,
        },
        true,
    );
    assert_eq!(
        Run::prepare(&f.client, source(&f.source)).err().unwrap(),
        "agent_host_source_not_text_message"
    );
    let system = f.append(types::EventBody::SpaceEncrypted, false);
    let event = f
        .client
        .timeline(f.space.clone())
        .unwrap()
        .into_iter()
        .find(|e| e.id == system)
        .unwrap();
    assert!(matches!(event.kind, EntryKind::System { .. }));
    assert_eq!(
        Run::prepare(&f.client, source(&event.id)).err().unwrap(),
        "agent_host_source_not_text_message"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn live_binding_refuses_text_edit_member_removal_and_foreign_context() {
    let server = relay(Mode::Normal).await;
    let mut f = Fixture::new(&server.url);
    online(&f.client, true).await;
    let run = Run::prepare(&f.client, source(&f.source)).unwrap();
    let account = f.client.account().unwrap();
    assert!(run.actor("foreign", &account.device_id, &f.owner).is_err());
    f.append(
        types::EventBody::MessageAnnotated {
            target: f.source.clone(),
            note: types::Annotation::Edit {
                text: "Changed after selection.".into(),
            },
        },
        true,
    );
    assert_eq!(
        run.check(&f.client),
        Err("agent_host_source_changed".into())
    );
    let run = Run::prepare(&f.client, source(&f.other)).unwrap();
    f.append(
        types::EventBody::MemberRemoved {
            identity: f.owner.clone(),
        },
        false,
    );
    assert_eq!(
        run.check(&f.client),
        Err("agent_host_source_member_unavailable".into())
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shared_app_card_is_not_an_explicit_text_task() {
    let server = relay(Mode::Normal).await;
    let mut f = Fixture::new(&server.url);
    online(&f.client, true).await;
    f.client.stop_sync();
    let base = deck(&f);
    let card = f.append(
        message("Generated share caption.", Some(base.item_id)),
        true,
    );
    assert!(f
        .client
        .timeline(f.space.clone())
        .unwrap()
        .iter()
        .any(|e| e.id == card && matches!(e.kind, EntryKind::Message { card: Some(_), .. })));
    assert_eq!(
        Run::prepare(&f.client, source(&card)).err().unwrap(),
        "agent_host_source_not_text_message"
    );
}

fn deck(f: &Fixture) -> client_core::AppBinaryDocumentDto {
    use base64::Engine;
    f.client
        .app_binary_create(
            f.space.clone(),
            "Synthetic deck".into(),
            base64::engine::general_purpose::STANDARD.encode(include_bytes!(
                "../../../../client-core/src/app_binary_documents/fixtures/original.deckcraft"
            )),
            "source_fixture_create".into(),
        )
        .unwrap()
}
fn stage(f: &Fixture, item: &str, source: &str) -> client_core::AgentRequestDto {
    stage_review(f, item, source, false)
}
fn stage_review(
    f: &Fixture,
    item: &str,
    source: &str,
    review: bool,
) -> client_core::AgentRequestDto {
    let turn = f
        .client
        .agent_binary_turn(item, source, &f.owner, None, None)
        .unwrap();
    let account = f.client.account().unwrap();
    let entry = f
        .client
        .timeline(f.space.clone())
        .unwrap()
        .into_iter()
        .find(|e| e.id == source)
        .unwrap();
    let mut definition = crate::deckcraft::tools()
        .into_iter()
        .find(|t| t.name == "read_deckcraft")
        .unwrap();
    definition.name = crate::deckcraft::model_name("read_deckcraft", item).unwrap();
    let arguments = json!({"artifactId":item});
    let rule = if review {
        zoen_agentd::NATIVE_REVIEW_REQUIRED
    } else {
        "model:low-confidence"
    };
    let mut ask = json!({"version":1,"policy_sha256":"77".repeat(32),
        "envelope":{"agent":account.identity_id,"device":account.device_id,"owner":f.owner,"space":f.space,"from":f.owner,
            "inbound_sha256":hex::encode(Sha256::digest(text_of(&entry).unwrap().as_bytes()))},
        "call":{"id":"source-held-read","name":definition.name,"arguments":arguments},"definition":definition,
        "steps":[],"remaining":[],"round":0,"rule":rule});
    if review {
        ask["call"]["requires_review"] = true.into();
    }
    f.client
        .stage_tool_approval(
            &turn,
            client_core::AgentToolApprovalInput {
                canonical_tool: "read_deckcraft".into(),
                arguments,
                worker_binding: "88".repeat(32),
                continuation_json: serde_json::to_string(
                    &serde_json::from_value::<ToolAsk>(ask).unwrap(),
                )
                .unwrap(),
                gate_rule: rule.into(),
            },
        )
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn approved_recovery_enters_only_selected_request_and_retains_executor_admission() {
    let server = relay(Mode::Normal).await;
    let mut f = Fixture::new(&server.url);
    online(&f.client, true).await;
    f.client.stop_sync();
    let base = deck(&f);
    let other = stage(&f, &base.item_id, &f.other);
    let selected = stage(&f, &base.item_id, &f.source);
    let mut run = Run::prepare(&f.client, source(&f.source)).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let cx = context(&f, &calls, false);
    let a = trace::tests::capture(&f.space, &f.source);
    let b = trace::tests::capture(&f.space, &f.other);
    assert!(!run
        .tick(&f.client, &cx, &definition(), deadline())
        .await
        .unwrap());
    assert_eq!(a.values().last().unwrap()["outcome"], "pending");
    let observed = a.values().len();
    f.approve(&other).await;
    assert!(!run
        .tick(&f.client, &cx, &definition(), deadline())
        .await
        .unwrap());
    assert_eq!(
        a.values().len(),
        observed,
        "other approval must not re-enter the selected source"
    );
    f.approve(&selected).await;
    assert_eq!(
        run.head(&f.client).unwrap(),
        Some((selected.id.clone(), RequestStatus::Approved, None))
    );
    assert_eq!(f.client.approved_tool_sources(&f.space).len(), 2);
    assert!(!run
        .tick(
            &f.client,
            &context(&f, &calls, false),
            &definition(),
            deadline()
        )
        .await
        .unwrap());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        a.values().last().unwrap()["outcome"],
        "executor_unavailable"
    );
    assert!(
        b.values().is_empty(),
        "unrelated approved source must never enter turn/recovery"
    );
    assert_eq!(
        f.client
            .requests()
            .iter()
            .filter(|r| r.status == RequestStatus::Approved)
            .count(),
        2
    );
    assert!(server.cursors.lock().unwrap().is_empty());
    assert!(!f.path.join("source-steps.json").exists());
}

mod review;

mod renewal;
