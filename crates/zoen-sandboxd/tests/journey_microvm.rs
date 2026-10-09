//! User journeys for the microVM tier on real Firecracker (ADR 0028 P1).

mod common;

use bytes::Bytes;
use common::*;
use std::time::{Duration, Instant};
use zoen_agentd::{ExecRequest, SandboxError, SandboxProvider};
use zoen_sandboxd::{FirecrackerProvider, Shape};

/// An agent runs a code tool: the first use builds the template (the only cold boot), every
/// lease after that comes from the warm pool in well under a second, runs real commands in
/// its own kernel with no network, keeps its files across suspend and resume, and is gone
/// without a trace at the end.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_code_tool_gets_a_warm_microvm_and_leaves_nothing_behind() {
    let Some(cfg) = config("journey-lifecycle") else {
        return;
    };
    let work = cfg.work.clone();
    let p = FirecrackerProvider::new(cfg, None);
    let spec = spec("tool.python-run", 2, 256);
    let shape = Shape::of(&spec);

    // First use of this shape: cold boot + snapshot, once.
    let built = p.warm(shape).await.expect("template").expect("built now");
    eprintln!("template (cold boot + full snapshot): {}", ms(built));
    assert_eq!(p.pooled(shape), 2, "pool filled");

    // Acquire from the pool.
    let a = p.acquire(&spec, "id_owner").await.expect("acquire");
    let t = p.last_acquire();
    eprintln!("acquire from warm pool: {}", ms(t.total));
    assert!(t.from_pool);
    assert!(
        t.total < Duration::from_secs(1),
        "acquire took {:?}",
        t.total
    );

    // Real commands, its own kernel, the vCPUs it asked for.
    let out = p
        .exec(
            &a,
            ExecRequest::sh("uname -r; grep -c ^processor /proc/cpuinfo; hostname"),
        )
        .await
        .unwrap();
    let text = out.stdout_str();
    eprintln!("guest: {}", text.replace('\n', " | "));
    assert_eq!(out.exit_code, Some(0));
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines[0].starts_with("6.1."), "guest kernel, not the host's");
    assert_eq!(lines[1], "2");
    assert_eq!(lines[2], "sandbox");

    // No network at all: only loopback, and the proxy path has no egress behind it here.
    let out = p
        .exec(
            &a,
            ExecRequest::sh(
                "ls /sys/class/net; wget -q -T 3 -O - http://169.254.169.254/ 2>&1; echo rc=$?",
            ),
        )
        .await
        .unwrap();
    let text = out.stdout_str();
    assert!(text.starts_with("lo\n"), "only loopback: {text}");
    assert!(
        !text.contains("rc=0"),
        "metadata address unreachable: {text}"
    );

    // Exec latency in a warm VM.
    let mut lat = vec![];
    for _ in 0..10 {
        let t0 = Instant::now();
        let o = p.exec(&a, ExecRequest::sh("true")).await.unwrap();
        assert_eq!(o.exit_code, Some(0));
        lat.push(t0.elapsed());
    }
    lat.sort();
    eprintln!("exec round trip: p50 {}, max {}", ms(lat[5]), ms(lat[9]));

    // Files in and out.
    p.put_file(
        &a,
        "/work/in.txt",
        Bytes::from_static(b"hello from the owner\n"),
    )
    .await
    .unwrap();
    let o = p
        .exec(&a, ExecRequest::sh("wc -c < /work/in.txt > /work/out.txt"))
        .await
        .unwrap();
    assert_eq!(o.exit_code, Some(0));
    assert_eq!(&p.get_file(&a, "/work/out.txt").await.unwrap()[..], b"21\n");

    // A runaway command is cut off at its timeout, and the VM stays usable.
    let t0 = Instant::now();
    let o = p
        .exec(
            &a,
            ExecRequest {
                timeout: Duration::from_secs(1),
                ..ExecRequest::sh("sleep 30")
            },
        )
        .await
        .unwrap();
    assert!(o.timed_out && o.exit_code.is_none());
    assert!(t0.elapsed() < Duration::from_secs(5));

    // Two leases from the same template don't share randomness.
    let b = p.acquire(&spec, "id_owner").await.expect("second acquire");
    let rand = |l| {
        let p = p.clone();
        async move {
            p.exec(&l, ExecRequest::sh("head -c 16 /dev/urandom | od -An -tx1"))
                .await
                .unwrap()
                .stdout_str()
        }
    };
    assert_ne!(rand(a.clone()).await, rand(b.clone()).await);
    p.release(b).await.unwrap();

    // Idle: suspend frees the VM (its cgroup goes away), resume brings the files back.
    let cg = p.cgroup_of(&a).unwrap();
    let t0 = Instant::now();
    let snap = p.suspend(&a).await.expect("suspend");
    let t_suspend = t0.elapsed();
    assert!(
        !cg.exists(),
        "suspended VM no longer runs: procs {:?}",
        read(&cg.join("cgroup.procs"))
    );
    let t0 = Instant::now();
    let a = p.resume(&snap).await.expect("resume");
    let t_resume = t0.elapsed();
    eprintln!(
        "suspend (full snapshot): {}; resume: {}",
        ms(t_suspend),
        ms(t_resume)
    );
    let o = p
        .exec(&a, ExecRequest::sh("cat /work/in.txt"))
        .await
        .unwrap();
    assert_eq!(o.stdout_str(), "hello from the owner\n");
    assert!(
        matches!(p.resume(&snap).await, Err(SandboxError::NoLease)),
        "a snapshot restores once"
    );

    // End of task: the VM, its cgroup and its chroot are gone.
    let cg = p.cgroup_of(&a).unwrap();
    p.release(a.clone()).await.unwrap();
    assert!(!cg.exists());
    assert!(matches!(
        p.exec(&a, ExecRequest::sh("true")).await,
        Err(SandboxError::NoLease)
    ));
    drop(p);
    let left = std::fs::read_dir(work.join("jail/firecracker"))
        .map(|d| d.count())
        .unwrap_or(0);
    assert_eq!(left, 0, "no VM left behind, pooled ones included");
}

/// The host enforces each VM's limits, whatever the guest does: memory, CPU and process
/// caps sit in the VMM's cgroup, and a guest burning every vCPU is throttled to its quota.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_host_enforces_a_microvms_cpu_and_memory_limits() {
    let Some(mut cfg) = config("journey-limits") else {
        return;
    };
    cfg.pool_size = 0;
    cfg.cpu_quota_pct = 50; // two vCPUs, one core's worth of CPU time
    let p = FirecrackerProvider::new(cfg, None);
    let spec = spec("tool.heavy", 2, 256);
    let l = p.acquire(&spec, "id_owner").await.expect("acquire");
    let cg = p.cgroup_of(&l).unwrap();
    let lim = p.limits_of(&l).unwrap();

    // The kernel has the limits (read back from the cgroup, not from our config).
    assert_eq!(read(&cg.join("memory.max")), lim.memory_bytes.to_string());
    assert_eq!(lim.memory_bytes, (256 + 64) << 20);
    assert_eq!(read(&cg.join("cpu.max")), "100000 100000");
    assert_eq!(read(&cg.join("pids.max")), "32");

    // Burn both vCPUs for 3 s; the VMM gets at most one core.
    let usage = |s: String| -> (u64, u64) {
        let get = |k: &str| {
            s.lines()
                .find_map(|l| l.strip_prefix(k)?.trim().parse().ok())
                .unwrap_or(0)
        };
        (get("usage_usec"), get("nr_throttled"))
    };
    let (u0, t0) = usage(read(&cg.join("cpu.stat")));
    let wall = Instant::now();
    let o = p
        .exec(
            &l,
            ExecRequest::sh(
                "for i in 1 2; do (while :; do :; done) & done; sleep 3; kill $(jobs -p)",
            ),
        )
        .await
        .unwrap();
    assert_eq!(
        o.exit_code,
        Some(0),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    let secs = wall.elapsed().as_secs_f64();
    let (u1, t1) = usage(read(&cg.join("cpu.stat")));
    let cores = (u1 - u0) as f64 / 1e6 / secs;
    eprintln!(
        "guest burned 2 vCPUs for {secs:.1} s: VMM used {cores:.2} cores, throttled {} times",
        t1 - t0
    );
    assert!(cores < 1.25, "quota of one core held: {cores:.2}");
    assert!(cores > 0.4, "the burn really ran: {cores:.2}");
    assert!(t1 > t0, "the kernel throttled it");

    let peak: u64 = read(&cg.join("memory.peak")).parse().unwrap_or(0);
    eprintln!(
        "VMM memory peak: {} MiB of {} MiB",
        peak >> 20,
        lim.memory_bytes >> 20
    );
    assert!(peak > 0 && peak <= lim.memory_bytes);

    p.release(l).await.unwrap();
    assert!(!cg.exists());
}
