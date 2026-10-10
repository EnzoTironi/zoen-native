use super::*;

pub(super) async fn policies_and_historical_owner() {
    let w = World::new().await;
    let mut tampered = w.signed.clone();
    tampered.policy.limit_units += 1;
    assert_eq!(
        w.runtime.install_policy(tampered).await.unwrap_err(),
        RuntimeError::InvalidPolicy
    );
    let mut agent_policy = w.signed.policy.clone();
    agent_policy.owner = w.agents[0].identity.clone();
    let signed_by_agent = roda_log::owner_budget::sign(&w.agents[0], agent_policy).unwrap();
    assert!(roda_log::owner_budget::verify(&signed_by_agent));
    assert_eq!(
        w.runtime.install_policy(signed_by_agent).await.unwrap_err(),
        RuntimeError::Denied
    );
    let mut tx = w.runtime.finance.pool.begin().await.unwrap();
    assert!(zoen_relay::db::revoke_device(
        &mut tx,
        &w.owner.identity,
        w.owner.device.as_deref().unwrap()
    )
    .await
    .unwrap());
    tx.commit().await.unwrap();
    let mut replacement = w.signed.policy.clone();
    replacement.version = 2;
    replacement.previous_digest = Some(w.policy_digest.clone());
    let signed = roda_log::owner_budget::sign(&w.owner, replacement).unwrap();
    assert_eq!(
        w.runtime.install_policy(signed).await.unwrap_err(),
        RuntimeError::Denied
    );
    // Historical sponsorship survives; this agent's own active device gates work.
    let step = w.step(0).await;
    let (_, state) = w.runtime.complete_verified(&step).await.unwrap();
    assert_eq!(state, FinancialState::Settled { units: 6 });
    assert_eq!(w.http.sends.load(Ordering::SeqCst), 1);
    w.finish().await;
}

pub(super) async fn configured_gateway_must_match_price_profile() {
    let w = World::new().await;
    let step = w.step(0).await;
    for mismatch in 0..3 {
        let mut config = config(&w.db_url, &w.config_namespace, &w.http.base);
        match mismatch {
            0 => config.gateway.model = "unapproved-model".into(),
            1 => config.gateway.credential_ref = "unapproved-credential".into(),
            _ => config.price.endpoint = format!("{}/unapproved/v1/chat/completions", w.http.base),
        }
        let runtime = RuntimeAuthority::open(config).await.unwrap();
        runtime.continuity.store(true, Ordering::SeqCst);
        assert_eq!(
            runtime.complete_verified(&step).await.unwrap_err(),
            RuntimeError::Denied
        );
        runtime.finance.pool.close().await;
    }
    assert_eq!(w.http.sends.load(Ordering::SeqCst), 0);
    assert_eq!(w.count("SELECT count(*) FROM runtime_attempts").await, 0);
    assert_eq!(w.balance().await.held_units, 0);
    w.finish().await;
}

pub(super) async fn owner_wide_concurrency() {
    let w = World::new().await;
    let a = w.binding(&w.step(0).await);
    let b = w.binding(&w.step(1).await);
    let mut tasks = Vec::new();
    for i in 0..100 {
        let runtime = w.runtime.clone();
        let mut binding = if i % 2 == 0 { a.clone() } else { b.clone() };
        binding.context.attempt_id = roda_types::new_id("attempt");
        binding.context.run_id = roda_types::new_id("child");
        tasks.push(tokio::spawn(async move {
            runtime.finance.reserve(&binding).await
        }));
    }
    let mut accepted = 0;
    for task in tasks {
        match task.await.unwrap() {
            Ok(()) => accepted += 1,
            Err(RuntimeError::OverBudget) => {}
            other => panic!("unexpected reservation result: {other:?}"),
        }
    }
    assert_eq!(accepted, 76);
    assert_eq!(
        w.balance().await,
        PeriodBalance {
            held_units: 988,
            spent_units: 0
        }
    );
    assert_eq!(w.count("SELECT count(*) FROM runtime_attempts").await, 76);
    assert_eq!(w.count("SELECT count(*) FROM runtime_postings").await, 152);
    assert_eq!(w.http.sends.load(Ordering::SeqCst), 0);
    w.finish().await;
}

pub(super) async fn immutable_balanced_postings() {
    let w = World::new().await;
    let step = w.step(0).await;
    let binding = w.binding(&step);
    w.runtime.finance.reserve(&binding).await.unwrap();
    w.runtime.finance.reserve(&binding).await.unwrap();
    assert_eq!(w.count("SELECT count(*) FROM runtime_journals").await, 1);
    let mut changed = binding.clone();
    changed.request_digest = hash(b"changed");
    assert_eq!(
        w.runtime.finance.reserve(&changed).await.unwrap_err(),
        RuntimeError::InvalidBinding
    );
    for statement in [
        "UPDATE runtime_postings SET units=units+1",
        "DELETE FROM runtime_postings",
        "DELETE FROM runtime_attempts",
        "UPDATE runtime_periods SET held_units=0",
        "INSERT INTO runtime_journals(attempt,kind) SELECT attempt,'release' FROM runtime_attempts LIMIT 1",
        "INSERT INTO runtime_financial_outbox(attempt,kind,digest) SELECT attempt,'settle',repeat('0',64) FROM runtime_attempts LIMIT 1",
    ] {
        let error=sqlx::query(statement).execute(&w.runtime.finance.pool).await.unwrap_err();
        assert_eq!(error.as_database_error().and_then(|e| e.code()).as_deref(),Some("23514"));
    }
    assert_eq!(w.balance().await.held_units, 13);
    w.finish().await;
}

pub(super) async fn changed_and_lowered_policy() {
    let w = World::new().await;
    let step = w.step(0).await;
    w.runtime.fault.store(4, Ordering::SeqCst);
    assert!(w.runtime.complete_verified(&step).await.is_err());
    let mut policy = w.signed.policy.clone();
    policy.version = 2;
    policy.previous_digest = Some(w.policy_digest.clone());
    policy.limit_units = 5;
    policy.max_attempt_units = 5;
    let signed = roda_log::owner_budget::sign(&w.owner, policy).unwrap();
    w.runtime.install_policy(signed).await.unwrap();
    w.runtime.fault.store(0, Ordering::SeqCst);
    assert!(w.runtime.complete_verified(&step).await.is_err());
    assert_eq!(w.http.sends.load(Ordering::SeqCst), 0);
    assert_eq!(w.balance().await.held_units, 13);
    let binding = w
        .runtime
        .execution
        .prepared_binding(&step.request.context.attempt_id, &w.runtime.custody)
        .await
        .unwrap();
    let proof = w
        .runtime
        .execution
        .close_before_dispatch(&step, &binding, &w.runtime.custody, &w.runtime)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        w.runtime.finance.release(proof, &w.runtime).await.unwrap(),
        FinancialState::Released
    );
    assert_eq!(w.balance().await.held_units, 0);
    let mut price = w.runtime.price.clone();
    price.currency = "BRL".into();
    assert_eq!(price.digest().unwrap_err(), RuntimeError::InvalidProfile);
    price = w.runtime.price.clone();
    price.input_units_per_million = u64::MAX;
    price.max_billable_input_tokens = u64::MAX;
    assert_eq!(price.digest().unwrap_err(), RuntimeError::InvalidProfile);
    w.finish().await;
}

pub(super) async fn overrun_and_missing_usage() {
    for (usage, expected, held) in [
        (serde_json::Value::Null, FinancialState::Claimed, 13),
        (
            serde_json::json!({"prompt_tokens":4,"total_tokens":4}),
            FinancialState::Claimed,
            13,
        ),
        (
            serde_json::json!({"prompt_tokens":-1,"completion_tokens":2}),
            FinancialState::Claimed,
            13,
        ),
        (
            serde_json::json!({"prompt_tokens":0,"completion_tokens":0,"total_tokens":0}),
            FinancialState::Settled { units: 0 },
            0,
        ),
    ] {
        let w = World::new().await;
        w.http.body.lock().unwrap()["usage"] = usage;
        let (_, state) = w.runtime.complete_verified(&w.step(0).await).await.unwrap();
        assert_eq!(state, expected);
        assert_eq!(w.balance().await.held_units, held);
        assert_eq!(w.http.sends.load(Ordering::SeqCst), 1);
        w.finish().await;
    }
    let w = World::new().await;
    w.http.body.lock().unwrap()["usage"] =
        serde_json::json!({"prompt_tokens":1001,"completion_tokens":2,"total_tokens":1003});
    let (_, state) = w.runtime.complete_verified(&w.step(0).await).await.unwrap();
    assert_eq!(state, FinancialState::Settled { units: 1003 });
    assert_eq!(
        w.balance().await,
        PeriodBalance {
            held_units: 0,
            spent_units: 1003
        }
    );
    assert!(w.runtime.complete_verified(&w.step(1).await).await.is_err());
    assert_eq!(w.http.sends.load(Ordering::SeqCst), 1);
    w.finish().await;
}

pub(super) async fn malformed_output_still_settles() {
    let w = World::new().await;
    {
        let mut body = w.http.body.lock().unwrap();
        body["choices"][0]["message"] = serde_json::json!({"role":"assistant","content":null,"tool_calls":[{"id":"call-fixture","type":"function","function":{"name":"not-offered","arguments":"{}"}}]});
        body["choices"][0]["finish_reason"] = serde_json::json!("tool_calls");
    }
    let (result, state) = w.runtime.complete_verified(&w.step(0).await).await.unwrap();
    assert!(result.output.is_err());
    assert_eq!(state, FinancialState::Settled { units: 6 });
    assert_eq!(w.balance().await.spent_units, 6);
    let step = w.step(1).await;
    w.http.body.lock().unwrap()["choices"][0]["message"]["content"] =
        serde_json::json!(BODY.repeat(2000));
    let (_, state) = w.runtime.complete_verified(&step).await.unwrap();
    assert_eq!(state, FinancialState::Claimed);
    assert_eq!(w.balance().await.held_units, 13);
    w.finish().await;
}
