//! Real signed Core, source journal and process fault; no services or provider.
use super::*;
use std::process::Command;
const CHILD: &str = "ZOEN_KNOWN_TOOLS_CHILD";
const EXACT: &str = "The exact accepted booking is confirmed once.";

fn runtime(f: &Fixture, final_calls: &Arc<AtomicUsize>, lose_response: bool) -> Ctx {
    let mut cx = context(f, final_calls, false);
    let calls = final_calls.clone();
    cx.runtime = Runtime::new(
        Gate {
            on_device: Some(Box::new(crate::agent::page_tool_tests::Allow)),
            ..Gate::default()
        },
        Box::new(ControlledToolBrain {
            evaluate: Box::new(move |steps| {
                calls.fetch_add(1, Ordering::SeqCst);
                if steps.len() != 1 || steps[0].is_error {
                    return Err(
                        "A completed exact receipt is required; do not repeat the first inference"
                            .into(),
                    );
                }
                if lose_response {
                    return Err("synthetic lost response from the next inference".into());
                }
                Ok(BrainOut::Say(EXACT.into()))
            }),
        }),
    );
    cx
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn known_tools_fault_child() -> Result<(), Box<dyn std::error::Error>> {
    let Some(config) = std::env::var_os(CHILD) else {
        return Ok(());
    };
    let input: serde_json::Value = serde_json::from_slice(&std::fs::read(config)?)?;
    let path = Path::new(input["path"].as_str().ok_or("missing path")?);
    let vault = Arc::new(relay::Vault::default());
    *vault.0.lock().map_err(|_| "vault poisoned")? =
        serde_json::from_value(input["vault"].clone())?;
    let e = Client::open(
        path.join("agent.sqlite").to_string_lossy().into_owned(),
        "en".into(),
    )?;
    assert!(e.unlock(vault)?);
    let account = e.account().ok_or("missing account")?;
    let call: ToolCall = serde_json::from_value(input["call"].clone())?;
    let count = path.join("brain-plans");
    let cx = Ctx {
        me: account.identity_id,
        device: account.device_id,
        owner: input["owner"].as_str().ok_or("missing owner")?.into(),
        journal: path.join("source-steps.json"),
        deckcraft: None,
        runtime: Runtime::new(
            Gate {
                on_device: Some(Box::new(crate::agent::page_tool_tests::Allow)),
                ..Gate::default()
            },
            Box::new(ControlledToolBrain {
                evaluate: Box::new(move |steps| {
                    if !steps.is_empty() {
                        return Err("Child must stop before the second inference".into());
                    }
                    use std::io::Write;
                    let mut file = std::fs::OpenOptions::new()
                        .append(true)
                        .create(true)
                        .open(&count)
                        .map_err(|error| {
                            zoen_agentd::BrainError::OutcomeUnknown(error.to_string())
                        })?;
                    file.write_all(b"1").map_err(|error| {
                        zoen_agentd::BrainError::OutcomeUnknown(error.to_string())
                    })?;
                    Ok(BrainOut::CallTools(vec![call.clone()]))
                }),
            }),
        ),
    };
    let space = input["space"].as_str().ok_or("missing space")?;
    let source = input["source"].as_str().ok_or("missing source")?;
    let timeline = e.timeline(space.into())?;
    let index = timeline
        .iter()
        .position(|entry| entry.id == source)
        .ok_or("missing source")?;
    super::super::super::turn::turn(
        &e,
        &cx,
        &definition(),
        space,
        &timeline[..index],
        &timeline[index],
    )
    .await;
    Err("the before-journal fault did not fire".into())
}

async fn exercise(lose_response: bool) -> Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::new("http://127.0.0.1:1")?;
    let agent = f.client.account().ok_or("missing account")?.identity_id;
    f.append(
        types::EventBody::GrantIssued {
            grant: types::Grant {
                id: types::new_id("source-act")?,
                grantor: f.owner.clone(),
                grantee: Some(agent),
                scope: types::GrantScope::Space(f.space.clone()),
                capability: types::Capability::Trust(types::TrustLevel::Act),
                expires_at_ms: None,
            },
        },
        true,
    )?;
    let hours: serde_json::Map<String, serde_json::Value> = (0..7)
        .map(|day| (day.to_string(), json!([["08:00", "20:00"]])))
        .collect();
    let item = f.client.install_app(f.space.clone(), "schedule".into(), json!({
        "business":"Offline test", "timezone":"UTC", "services":[{"id":"corte","name":"Corte","minutes":30,"priceCents":0}], "weeklyHours":hours
    }).to_string())?;
    f.confirm_publications()?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as i64;
    let start = (now / 86_400_000 + 2) * 86_400_000 + 10 * 3_600_000;
    let call = ToolCall {
        id: "source-book-once".into(),
        name: "schedule_book".into(),
        arguments: json!({"serviceId":"corte", "startMs":start, "note":"exact source recovery"}),
        requires_review: false,
    };
    let config = f.path.join("child.json");
    std::fs::write(
        &config,
        serde_json::to_vec(
            &json!({"path":f.path,"owner":f.owner,"space":f.space,"source":f.source,"call":call,
        "vault":*f.vault.0.lock().map_err(|_| "vault poisoned")?}),
        )?,
    )?;
    f.close();
    let status = Command::new(std::env::current_exe()?)
        .args([
            "--exact",
            "agent::source_mode::tests::known_tools::known_tools_fault_child",
            "--nocapture",
        ])
        .env(CHILD, &config)
        .env("ZOEN_AGENT_FAULT", "before-journal")
        .env("ZOEN_AGENT_FAULT_MARKER", f.path.join("fault-fired"))
        .status()?;
    assert_eq!(status.code(), Some(86));
    std::fs::remove_file(config)?;
    f.reopen();
    let final_calls = Arc::new(AtomicUsize::new(0));
    {
        let cx = runtime(&f, &final_calls, lose_response);
        let timeline = f.client.timeline(f.space.clone())?;
        let original = timeline
            .iter()
            .find(|entry| entry.id == f.source)
            .ok_or("missing source")?;
        let tools = SpaceTools::new(&f.client, &cx, &f.space, &f.owner, &f.source);
        let def = definition();
        let source_host =
            super::super::super::source_recovery::SourceTools::new(&tools, &cx, &def, original);
        let before = f.client.connection().pending;
        assert!(
            source_host.recover().is_err(),
            "an unacknowledged optimistic booking is not a canonical receipt"
        );
        assert_eq!(
            f.client.connection().pending,
            before,
            "recover-only cannot execute or enqueue"
        );
    }
    // The actual signed pending events receive a synthetic relay sequence, without executing anything again.
    f.confirm_publications()?;
    let cx = runtime(&f, &final_calls, lose_response);
    let timeline = f.client.timeline(f.space.clone())?;
    let index = timeline
        .iter()
        .position(|entry| entry.id == f.source)
        .ok_or("missing source")?;
    assert_eq!(
        super::super::super::turn::turn(
            &f.client,
            &cx,
            &definition(),
            &f.space,
            &timeline[..index],
            &timeline[index]
        )
        .await,
        if lose_response {
            super::super::super::turn::Outcome::Retry
        } else {
            super::super::super::turn::Outcome::Handled
        }
    );
    assert_eq!(std::fs::read(f.path.join("brain-plans"))?, b"1");
    assert_eq!(
        final_calls.load(Ordering::SeqCst),
        1,
        "only the next, previously undispatched inference may run"
    );
    if lose_response {
        drop(cx);
        f.reopen();
        let cx = runtime(&f, &final_calls, false);
        let timeline = f.client.timeline(f.space.clone())?;
        let index = timeline
            .iter()
            .position(|entry| entry.id == f.source)
            .ok_or("missing source")?;
        assert_eq!(
            super::super::super::turn::turn(
                &f.client,
                &cx,
                &definition(),
                &f.space,
                &timeline[..index],
                &timeline[index]
            )
            .await,
            super::super::super::turn::Outcome::Retry
        );
        assert_eq!(
            final_calls.load(Ordering::SeqCst),
            1,
            "an opaque next-inference failure remains Unknown across cold restart"
        );
        assert!(timeline.iter().all(|entry| entry
            .reply_to
            .as_ref()
            .is_none_or(|reply| reply.id != f.source)));
        return Ok(());
    }
    let replies: Vec<_> = f
        .client
        .timeline(f.space.clone())?
        .into_iter()
        .filter(|entry| {
            entry
                .reply_to
                .as_ref()
                .is_some_and(|reply| reply.id == f.source)
        })
        .collect();
    assert_eq!(replies.len(), 1);
    assert_eq!(text_of(&replies[0]), Some(EXACT));
    let view: serde_json::Value = serde_json::from_str(
        &f.client
            .app_call_tool(item.id, "schedule_get_state".into(), "{}".into(), false)?
            .result_json,
    )?;
    let encoded = view.to_string();
    assert_eq!(encoded.matches("exact source recovery").count(), 1);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cold_source_reconciles_accepted_booking_without_repeating_plan_or_executor(
) -> Result<(), Box<dyn std::error::Error>> {
    exercise(false).await
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lost_next_inference_after_known_tools_is_not_repeated_after_cold_restart(
) -> Result<(), Box<dyn std::error::Error>> {
    exercise(true).await
}
