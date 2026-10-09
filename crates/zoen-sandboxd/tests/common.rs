//! Shared setup for the Firecracker journeys. They need `/dev/kvm` and the exports of
//! `scripts/firecracker.sh env`; without them they say so and pass, unless
//! `ZOEN_REQUIRE_FIRECRACKER=1` (set in CI), where a missing VM is a failure.

#![allow(dead_code)]

use std::path::PathBuf;
use std::time::Duration;
use zoen_agentd::{SandboxSpec, Tier};
use zoen_sandboxd::FirecrackerConfig;

pub fn config(journey: &str) -> Option<FirecrackerConfig> {
    // Fixed per journey, so a run that was killed leaves VMs the next run sweeps up.
    let base = std::env::var_os("ZOEN_FC_WORK")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("ZOEN_FIRECRACKER")
                .map(|f| PathBuf::from(f).parent().unwrap().join("work"))
        });
    let cfg = base.and_then(|b| FirecrackerConfig::from_env(b.join(journey)));
    if cfg.is_none() {
        assert!(
            std::env::var("ZOEN_REQUIRE_FIRECRACKER").as_deref() != Ok("1"),
            "ZOEN_REQUIRE_FIRECRACKER=1 but Firecracker isn't set up (scripts/firecracker.sh)"
        );
        eprintln!("skipping {journey}: no /dev/kvm or scripts/firecracker.sh env not loaded");
    }
    cfg
}

pub fn spec(tool: &str, vcpu: u8, mem_mib: u32) -> SandboxSpec {
    SandboxSpec {
        tier: Tier::MicroVm,
        tool: tool.into(),
        template: "base".into(),
        vcpu,
        mem_mib,
        disk_mib: 64,
        max_secs: 120,
        egress: vec![],
        secrets: vec![],
        agent: None,
    }
}

pub fn ms(d: Duration) -> String {
    format!("{:.1} ms", d.as_secs_f64() * 1000.0)
}

pub fn read(p: &std::path::Path) -> String {
    std::fs::read_to_string(p)
        .unwrap_or_default()
        .trim()
        .to_string()
}
