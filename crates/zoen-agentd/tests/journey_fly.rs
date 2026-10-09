//! ADR 0028 P3: the microVM tier on staging, where each lease is a Fly Machine (a Firecracker
//! microVM) on the sandbox app's own private network. Same journey as the box's: Ana approves
//! a code tool, it gets a computer only on the first call, files survive a suspend, the
//! computer can't see Zoen's private network, and it's gone at the end.
//!
//! Runs against real Fly when `ZOEN_FLY_SANDBOX_APP` and `ZOEN_FLY_SANDBOX_TOKEN` (or `FLY_API_TOKEN`) are set (see
//! `infra/fly/deploy.sh sandbox`); otherwise it says so and passes, unless
//! `ZOEN_REQUIRE_FLY=1`.

use bytes::Bytes;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use zoen_agentd::budget::DailyBudget;
use zoen_agentd::sandbox::fly::{FlyConfig, FlyMachinesProvider};
use zoen_agentd::session::ToolSession;
use zoen_agentd::{ExecRequest, SandboxProvider, SandboxSpec, Tier};

const NOW: i64 = 1_791_000_000_000;

fn provider() -> Option<Arc<FlyMachinesProvider>> {
    match FlyConfig::from_env() {
        Some(cfg) => Some(Arc::new(FlyMachinesProvider::new(cfg).unwrap())),
        None => {
            assert!(
                std::env::var("ZOEN_REQUIRE_FLY").as_deref() != Ok("1"),
                "ZOEN_REQUIRE_FLY=1 but ZOEN_FLY_SANDBOX_APP / ZOEN_FLY_SANDBOX_TOKEN aren't set"
            );
            eprintln!("skipping journey_fly: no Fly sandbox app configured");
            None
        }
    }
}

fn spec() -> SandboxSpec {
    SandboxSpec {
        tier: Tier::MicroVm,
        tool: "python".into(),
        template: "base".into(),
        vcpu: 1,
        mem_mib: 256,
        disk_mib: 1024,
        max_secs: 120,
        egress: vec![],
        secrets: vec![],
        agent: None,
    }
}

#[tokio::test]
async fn an_approved_tool_gets_a_fly_microvm_only_when_it_runs_and_loses_it_at_the_end() {
    let Some(fly) = provider() else { return };
    let budget = Arc::new(Mutex::new(DailyBudget::new(3600, 3600, NOW)));
    let session = ToolSession::new(
        fly.clone(),
        spec(),
        "ana".into(),
        Duration::from_secs(60),
        budget,
    );
    assert_eq!(
        session.state().await,
        "cold",
        "nothing runs (or costs) before the first call"
    );

    // First call: a Machine boots and runs real code.
    let out = session
        .exec(
            ExecRequest::sh("uname -s; cat /proc/cpuinfo | grep -c ^processor"),
            NOW,
        )
        .await
        .unwrap();
    assert_eq!(
        out.exit_code,
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.stdout_str().starts_with("Linux"),
        "{}",
        out.stdout_str()
    );
    assert_eq!(session.state().await, "running");

    // The computer can't see Zoen's private network: the relay's internal name doesn't
    // resolve from the sandbox app's network.
    let out = session
        .exec(
            ExecRequest::sh(
                "getent hosts zoen-staging-relay.internal >/dev/null && echo seen || echo hidden",
            ),
            NOW,
        )
        .await
        .unwrap();
    assert_eq!(out.stdout_str().trim(), "hidden");
    session.finish(NOW).await.unwrap();
}

#[tokio::test]
async fn files_survive_a_suspend_and_the_machine_is_destroyed_on_release() {
    let Some(fly) = provider() else { return };
    let lease = fly.acquire(&spec(), "ana").await.unwrap();
    let machine = fly.machine_of(&lease).unwrap();
    fly.put_file(
        &lease,
        "/work/planilha.csv",
        Bytes::from("mês,gasto\nout,42\n"),
    )
    .await
    .unwrap();
    let out = fly
        .exec(&lease, ExecRequest::sh("wc -l < /work/planilha.csv"))
        .await
        .unwrap();
    assert_eq!(out.stdout_str().trim(), "2");

    let snap = fly.suspend(&lease).await.unwrap();
    assert_eq!(fly.state_of(&machine).await.unwrap(), "suspended");
    let lease = fly.resume(&snap).await.unwrap();
    assert_eq!(
        fly.get_file(&lease, "/work/planilha.csv").await.unwrap(),
        Bytes::from("mês,gasto\nout,42\n")
    );

    fly.release(lease).await.unwrap();
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let state = fly.state_of(&machine).await.expect("verify Fly teardown");
            if state == "destroyed" {
                return;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    })
    .await
    .expect("Fly did not confirm machine destruction within 30 seconds");
}
