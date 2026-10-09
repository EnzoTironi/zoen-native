//! Journeys for ADR 0028 P0: the cheapest tier by default, heavier tiers only on a signed
//! manifest plus the owner's approval, sandboxes that start late, sleep when idle and go away
//! at the end, a daily budget, and (on Linux) real code running in gVisor.

use bytes::Bytes;
use ed25519_dalek::SigningKey;
use roda_types::ActionClass;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use zoen_agentd::budget::{BudgetError, DailyBudget};
use zoen_agentd::router::RouteError;
use zoen_agentd::sandbox::fake::FakeProvider;
use zoen_agentd::sandbox::gvisor::GvisorProvider;
use zoen_agentd::session::{SessionError, ToolSession};
use zoen_agentd::{
    EgressRule, ExecRequest, Needs, Route, Router, SandboxProvider, Tier, ToolManifest,
};

const NOW: i64 = 1_791_000_000_000;

fn publisher() -> SigningKey {
    SigningKey::from_bytes(&[42; 32])
}

fn summarize() -> ToolManifest {
    ToolManifest {
        id: "summarize".into(),
        version: "1".into(),
        summary: "Resumir a conversa".into(),
        needs: Needs::None,
        egress: vec![],
        secrets: vec![],
    }
}

fn python() -> ToolManifest {
    ToolManifest {
        id: "python".into(),
        version: "1".into(),
        summary: "Rodar uma planilha em Python".into(),
        needs: Needs::MicroVm {
            vcpu: 2,
            mem_mib: 512,
            disk_mib: 1024,
            max_secs: 120,
        },
        egress: vec![EgressRule::host("*.pypi.org")],
        secrets: vec![],
    }
}

#[tokio::test]
async fn only_the_tool_that_needs_a_computer_gets_one_and_only_after_ana_says_yes() {
    let router = Router::new(vec![publisher().verifying_key()]);
    let provider = Arc::new(FakeProvider::new());
    let (ana, agent) = ("ana".to_string(), "ana-agent".to_string());

    // A tool that declares nothing runs in the agent runtime: no sandbox, no card.
    let s = summarize().sign(&publisher());
    assert_eq!(
        router.route(&ana, &agent, &s, &[], NOW),
        Route::Run {
            tier: Tier::None,
            spec: None
        }
    );

    // The model can't push it to a heavier tier at call time.
    assert_eq!(
        router.check_call(&s, Tier::MicroVm),
        Err(RouteError::NotDeclared {
            declared: Tier::None,
            asked: Tier::MicroVm
        })
    );

    // A tampered or unknown-publisher manifest is refused outright.
    let mut forged = python().sign(&publisher());
    forged.manifest.needs = Needs::MicroVm {
        vcpu: 8,
        mem_mib: 8192,
        disk_mib: 1024,
        max_secs: 3600,
    };
    assert!(matches!(
        router.route(&ana, &agent, &forged, &[], NOW),
        Route::Refused(_)
    ));
    let stranger = python().sign(&SigningKey::from_bytes(&[1; 32]));
    assert!(matches!(
        router.route(&ana, &agent, &stranger, &[], NOW),
        Route::Refused(_)
    ));

    // The Python tool declares a microVM: without a Grant, Ana gets a card in plain words.
    let p = python().sign(&publisher());
    let Route::NeedsApproval(card) = router.route(&ana, &agent, &p, &[], NOW) else {
        panic!("expected a card");
    };
    assert_eq!(
        card.title,
        "Usar um computador isolado para “Rodar uma planilha em Python”"
    );
    assert_eq!(
        card.detail,
        "Até 2 min por vez, acesso só a *.pypi.org. Ele é apagado quando a tarefa acaba."
    );
    assert_eq!(card.action, ActionClass::External);
    assert_eq!(
        provider.acquired.load(Ordering::SeqCst),
        0,
        "nothing started before approval"
    );

    // She approves: the Grant routes it to a microVM spec. Still nothing started (lazy).
    let grant = Router::grant_for(&ana, &agent, &p.manifest, Some(NOW + 3_600_000));
    let Route::Run {
        tier: Tier::MicroVm,
        spec: Some(spec),
    } = router.route(&ana, &agent, &p, std::slice::from_ref(&grant), NOW)
    else {
        panic!("expected a microVM route");
    };
    assert_eq!((spec.vcpu, spec.mem_mib, spec.max_secs), (2, 512, 120));
    // A Grant for someone else's agent, or an expired one, doesn't count.
    let mut other = grant.clone();
    other.grantee = Some("bob-agent".into());
    assert!(matches!(
        router.route(&ana, &agent, &p, &[other], NOW),
        Route::NeedsApproval(_)
    ));
    assert!(matches!(
        router.route(
            &ana,
            &agent,
            &p,
            std::slice::from_ref(&grant),
            NOW + 3_600_001
        ),
        Route::NeedsApproval(_)
    ));

    // Lazy start, idle suspend, resume, teardown.
    let budget = Arc::new(Mutex::new(DailyBudget::new(600, 0, NOW)));
    let session = ToolSession::new(
        provider.clone(),
        spec,
        ana.clone(),
        Duration::from_millis(150),
        budget.clone(),
    );
    assert_eq!(session.state().await, "cold");
    assert_eq!(provider.acquired.load(Ordering::SeqCst), 0);

    let out = session
        .exec(ExecRequest::sh("print(1)"), NOW)
        .await
        .unwrap();
    assert_eq!(out.exit_code, Some(0));
    assert_eq!(session.state().await, "running");
    assert_eq!(provider.acquired.load(Ordering::SeqCst), 1);

    assert!(!session.tick(NOW).await.unwrap(), "not idle yet");
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(session.tick(NOW).await.unwrap());
    assert_eq!(session.state().await, "suspended");
    assert_eq!(
        provider.live(),
        0,
        "a suspended sandbox holds no CPU or RAM"
    );
    let charged: u64 = session.usage().iter().map(|u| u.secs).sum();
    assert_eq!(
        charged, 1,
        "running time is charged, rounded up to the second"
    );

    session
        .exec(ExecRequest::sh("print(2)"), NOW)
        .await
        .unwrap();
    assert_eq!(
        provider.acquired.load(Ordering::SeqCst),
        1,
        "resumed, not started again"
    );

    session.finish(NOW).await.unwrap();
    assert_eq!(session.state().await, "done");
    assert_eq!(provider.released.load(Ordering::SeqCst), 1);
    assert!(matches!(
        session.exec(ExecRequest::sh("x"), NOW).await,
        Err(SessionError::Finished)
    ));
    assert_eq!(budget.lock().unwrap().microvm_secs_used, 2);
}

#[tokio::test]
async fn the_daily_budget_stops_heavy_tiers_and_resets_the_next_day() {
    let provider = Arc::new(FakeProvider::new());
    let spec = zoen_agentd::router::spec_for(&python());
    let budget = Arc::new(Mutex::new(DailyBudget::new(1, 0, NOW)));
    let session = ToolSession::new(
        provider.clone(),
        spec.clone(),
        "ana".into(),
        Duration::ZERO,
        budget.clone(),
    );

    session.exec(ExecRequest::sh("a"), NOW).await.unwrap();
    session.tick(NOW).await.unwrap(); // idle at once: charges 1 s
    let err = session.exec(ExecRequest::sh("b"), NOW).await.unwrap_err();
    assert!(matches!(
        err,
        SessionError::Budget(BudgetError::Exhausted {
            tier: Tier::MicroVm
        })
    ));

    // Tomorrow it works again.
    session
        .exec(ExecRequest::sh("c"), NOW + 86_400_000)
        .await
        .unwrap();

    // A tier with no allowance at all (browser = 0 s) never starts.
    let mut b = DailyBudget::new(600, 0, NOW);
    assert!(b.admit(Tier::Browser, NOW).is_err());
    assert!(b.admit(Tier::None, NOW).is_ok());
    session.finish(NOW + 86_400_000).await.unwrap();
}

/// Real code in gVisor: what the approved Python tool gets in P0.
#[tokio::test]
async fn approved_code_runs_isolated_in_gvisor() {
    if !cfg!(target_os = "linux") {
        eprintln!("gVisor runs on Linux only; skipped on this OS");
        return;
    }
    let work = tempfile::tempdir().unwrap();
    let provider = GvisorProvider::from_env(work.path().to_path_buf())
        .expect("run `eval \"$(scripts/gvisor.sh env)\"` after `scripts/gvisor.sh up`");
    let provider: Arc<dyn SandboxProvider> = Arc::new(provider);
    let spec = zoen_agentd::router::spec_for(&python());

    // A host file the tool must never see.
    let host_secret = work.path().join("host-secret.txt");
    std::fs::write(&host_secret, "do not leak").unwrap();

    let t = Instant::now();
    let lease = provider.acquire(&spec, "ana").await.unwrap();
    let acquire_ms = t.elapsed().as_millis();

    let sh = |s: &str| ExecRequest::sh(s);
    let out = provider
        .exec(&lease, sh("uname -r; hostname; id -u"))
        .await
        .unwrap();
    let text = out.stdout_str();
    assert!(
        text.contains("gvisor"),
        "a user-space kernel answers: {text} / {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains("sandbox"));

    // Files persist between calls; go in and out through the sandbox.
    provider
        .put_file(
            &lease,
            "/root/work/in.csv",
            Bytes::from_static(b"a,b\n1,2\n"),
        )
        .await
        .unwrap();
    let out = provider
        .exec(&lease, sh("wc -l < /root/work/in.csv > /root/work/out.txt"))
        .await
        .unwrap();
    assert_eq!(out.exit_code, Some(0));
    assert_eq!(
        provider
            .get_file(&lease, "/root/work/out.txt")
            .await
            .unwrap(),
        Bytes::from_static(b"2\n")
    );

    // No network at all in P0 (egress goes through zoen-egress from P1).
    let out = provider
        .exec(&lease, sh("wget -q -T 2 -O- http://1.1.1.1 >/dev/null 2>&1 && echo online || echo offline; ls /sys/class/net"))
        .await
        .unwrap();
    assert!(
        out.stdout_str().starts_with("offline"),
        "{}",
        out.stdout_str()
    );

    // The host's files aren't there.
    let path = host_secret.to_string_lossy().to_string();
    let out = provider
        .exec(&lease, sh(&format!("cat '{path}' 2>&1; ls /home")))
        .await
        .unwrap();
    assert!(
        !out.stdout_str().contains("do not leak"),
        "{}",
        out.stdout_str()
    );

    // Memory is capped by the manifest (512 MiB address space per process).
    let out = provider.exec(&lease, sh("ulimit -v")).await.unwrap();
    assert_eq!(out.stdout_str().trim(), "524288");

    // A runaway call is killed at its timeout.
    let mut slow = sh("sleep 30");
    slow.timeout = Duration::from_secs(1);
    let out = provider.exec(&lease, slow).await.unwrap();
    assert!(out.timed_out);
    assert!(out.elapsed < Duration::from_secs(5));

    // Latency of a call (fresh gVisor sandbox per exec in P0).
    let mut ms = vec![];
    for _ in 0..10 {
        let t = Instant::now();
        provider.exec(&lease, sh("true")).await.unwrap();
        ms.push(t.elapsed().as_millis());
    }
    ms.sort();
    println!("gvisor stand-in: acquire {acquire_ms} ms (copy of the base rootfs), exec p50 {} ms, max {} ms", ms[5], ms[9]);

    // Teardown deletes the disk.
    let id = lease.id.clone();
    provider.release(lease).await.unwrap();
    assert!(!work.path().join(&id).exists());
}
