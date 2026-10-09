//! `zoen-load`: simulated people on the real wire protocol.
//!
//! Each person is a device with its own WebSocket: it logs in (Hello, Challenge, Auth),
//! registers, joins one group of `--group` people, then publishes signed messages when the
//! open-loop scheduler says so (`--rate` messages per second across everyone, picked at
//! random). Delivery latency runs from the moment a message was due to the moment each other
//! member receives it, so queueing anywhere shows up. CPU and memory of the processes named
//! with `--pid` are read from /proc over the measured window.
//!
//!   zoen-load --relay http://127.0.0.1:8787 [--relay …] --users 2000 --group 8 \
//!             --rate 1000 --seconds 30 [--warmup 5] [--connect-concurrency 256] \
//!             [--pid relay=1234 --pid fdb=5678] [--label name] [--json out.json]
//!
//! The relay must allow many handshakes from one address (`ZOEN_LIMITS=connect_ip=…`),
//! since every simulated person shares the load generator's.

mod client;
mod procfs;

use std::time::Duration;

use anyhow::{bail, Context};
use client::{histogram, Conn, Plan, Publish, Tally};
use futures_util::{stream, StreamExt, TryStreamExt};
use hdrhistogram::Histogram;
use serde_json::{json, Value};
use tokio::{sync::mpsc, time::Instant};

struct Args {
    relays: Vec<String>,
    users: usize,
    group: usize,
    rate: f64,
    seconds: f64,
    warmup: f64,
    connect_concurrency: usize,
    pids: Vec<(String, u32)>,
    label: String,
    json: Option<String>,
}

fn args() -> anyhow::Result<Args> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let all = |name: &str| -> Vec<String> {
        raw.windows(2)
            .filter(|w| w[0] == name)
            .map(|w| w[1].clone())
            .collect()
    };
    let one = |name: &str| all(name).pop();
    let num = |name: &str, default: f64| -> anyhow::Result<f64> {
        one(name).map_or(Ok(default), |v| {
            v.parse().with_context(|| format!("{name} {v}"))
        })
    };
    let relays = all("--relay");
    if relays.is_empty() {
        bail!(
            "usage: zoen-load --relay URL [--relay URL…] --users N --group G --rate R --seconds S"
        );
    }
    let pids = all("--pid")
        .iter()
        .map(|p| {
            let (n, v) = p.split_once('=').context("--pid name=pid")?;
            Ok((n.to_string(), v.parse()?))
        })
        .collect::<anyhow::Result<_>>()?;
    let a = Args {
        relays,
        users: num("--users", 100.0)? as usize,
        group: num("--group", 8.0)? as usize,
        rate: num("--rate", 100.0)?,
        seconds: num("--seconds", 20.0)?,
        warmup: num("--warmup", 5.0)?,
        connect_concurrency: num("--connect-concurrency", 256.0)? as usize,
        pids,
        label: one("--label").unwrap_or_else(|| "run".into()),
        json: one("--json"),
    };
    if a.group < 2 || a.users < a.group {
        bail!("--group must be at least 2 and no more than --users");
    }
    Ok(a)
}

fn ms(h: &Histogram<u32>, q: f64) -> f64 {
    (h.value_at_quantile(q) as f64 / 1000.0 * 100.0).round() / 100.0
}

fn latency(h: &Histogram<u32>) -> Value {
    json!({
        "count": h.len(),
        "p50_ms": ms(h, 0.5), "p90_ms": ms(h, 0.9), "p99_ms": ms(h, 0.99),
        "p999_ms": ms(h, 0.999), "max_ms": (h.max() as f64 / 10.0).round() / 100.0,
    })
}

fn per_process(pids: &[(String, u32)], before: &[f64], after: &[f64], seconds: f64) -> Value {
    let mut out = serde_json::Map::new();
    for (i, (name, pid)) in pids.iter().enumerate() {
        let cpu = after[i] - before[i];
        out.insert(
            name.clone(),
            json!({
                "cpu_seconds": (cpu * 100.0).round() / 100.0,
                "cores_busy": (cpu / seconds * 100.0).round() / 100.0,
                "rss_mib": procfs::rss_bytes(*pid).map(|b| b / (1024 * 1024)),
            }),
        );
    }
    Value::Object(out)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let a = args()?;

    // ── connect and register everyone ──
    let rss_before: Vec<Option<u64>> = a.pids.iter().map(|(_, p)| procfs::rss_bytes(*p)).collect();
    let cpu_before = procfs::snapshot(&a.pids);
    let t0 = Instant::now();
    let opened: Vec<(Conn, u64)> = stream::iter(0..a.users)
        .map(|i| {
            let relay = a.relays[i % a.relays.len()].clone();
            async move {
                let t = Instant::now();
                let c = Conn::open(&relay)
                    .await
                    .with_context(|| format!("person {i}"))?;
                anyhow::Ok((c, t.elapsed().as_micros() as u64))
            }
        })
        .buffer_unordered(a.connect_concurrency)
        .try_collect()
        .await?;
    let connect_secs = t0.elapsed().as_secs_f64();
    let cpu_connected = procfs::snapshot(&a.pids);
    let mut handshake = histogram();
    let mut conns = Vec::with_capacity(opened.len());
    for (c, us) in opened {
        handshake.saturating_record(us.max(1));
        conns.push(c);
    }
    let rss_per_conn: Value = a
        .pids
        .iter()
        .zip(&rss_before)
        .map(|((name, pid), before)| {
            let per = match (before, procfs::rss_bytes(*pid)) {
                (Some(b), Some(n)) => Some(n.saturating_sub(*b) as f64 / a.users as f64 / 1024.0),
                _ => None,
            };
            (name.clone(), json!(per.map(|k| (k * 10.0).round() / 10.0)))
        })
        .collect::<serde_json::Map<_, _>>()
        .into();
    eprintln!(
        "connected {} people in {connect_secs:.1} s ({})",
        a.users,
        own_rss()
    );

    // ── groups ──
    // Exact-size groups: keeping `split_off`'s head would keep the whole remaining list's
    // capacity in every group, quadratic in people (3.7 GB at 10,000).
    let mut groups: Vec<Vec<Conn>> = Vec::new();
    let mut people = conns.into_iter().peekable();
    while people.peek().is_some() {
        groups.push(people.by_ref().take(a.group).collect());
    }
    if groups.len() > 1 && groups.last().is_some_and(|g| g.len() < 2) {
        let lone = groups.pop().and_then(|mut g| g.pop());
        groups.last_mut().unwrap().extend(lone);
    }
    let t1 = Instant::now();
    let formed: Vec<(String, Vec<Conn>)> = stream::iter(groups)
        .map(|mut g| async move {
            let members: Vec<String> = g[1..].iter().map(|c| c.identity().to_string()).collect();
            let space = g[0].create_group(&members).await?;
            anyhow::Ok((space, g))
        })
        .buffer_unordered(a.connect_concurrency)
        .try_collect()
        .await?;
    eprintln!(
        "formed {} groups in {:.1} s ({})",
        formed.len(),
        t1.elapsed().as_secs_f64(),
        own_rss()
    );

    // ── catch up, then run ──
    let start = Instant::now() + Duration::from_millis(500);
    let measure_from_us = (a.warmup * 1e6) as u64;
    let mut senders: Vec<(mpsc::Sender<Publish>, usize)> = Vec::new();
    let mut tasks = Vec::new();
    let ready: Vec<(Conn, String, roda_types::Seen, usize)> =
        stream::iter(formed.into_iter().flat_map(|(space, g)| {
            let n = g.len();
            g.into_iter().map(move |c| (c, space.clone(), n))
        }))
        .map(|(mut c, space, n)| async move {
            let head = c.catch_up(&space).await?;
            anyhow::Ok((c, space, head, n))
        })
        .buffer_unordered(a.connect_concurrency)
        .try_collect()
        .await?;
    eprintln!("caught up ({})", own_rss());
    for (c, space, head, n) in ready {
        let (tx, rx) = mpsc::channel(1024);
        senders.push((tx, n));
        tasks.push(tokio::spawn(client::run(
            c,
            Plan {
                space,
                head,
                start,
                measure_from_us,
                drain: Duration::from_secs(5),
            },
            rx,
        )));
    }

    tokio::time::sleep_until(start).await;
    eprintln!("running ({})", own_rss());
    let total_us = ((a.warmup + a.seconds) * 1e6) as u64;
    let mut dispatched: u64 = 0;
    let mut expected_deliveries: u64 = 0;
    let mut backlogged: u64 = 0;
    let mut cpu_measure_start = None;
    loop {
        let now_us = start.elapsed().as_micros() as u64;
        if now_us >= total_us {
            break;
        }
        if cpu_measure_start.is_none() && now_us >= measure_from_us {
            cpu_measure_start = Some((procfs::snapshot(&a.pids), Instant::now()));
        }
        // Message k is due at k/rate, so every k ≤ now·rate is due now.
        let due_total = (now_us as f64 / 1e6 * a.rate) as u64 + 1;
        while dispatched < due_total {
            let due_us = (dispatched as f64 / a.rate * 1e6) as u64;
            let (tx, n) = &senders[fastrand::usize(..senders.len())];
            match tx.try_send(Publish { due_us }) {
                Ok(()) if due_us >= measure_from_us => expected_deliveries += *n as u64 - 1,
                Ok(()) => {}
                Err(_) => backlogged += 1,
            }
            dispatched += 1;
        }
        // Wake exactly when the next message is due, or the measured window opens.
        let mut wake_us = (dispatched as f64 / a.rate * 1e6) as u64;
        if cpu_measure_start.is_none() {
            wake_us = wake_us.min(measure_from_us);
        }
        tokio::time::sleep_until(start + Duration::from_micros(wake_us.min(total_us))).await;
    }
    let (cpu_start, measured_at) = cpu_measure_start.context("run shorter than the warm-up")?;
    let measured_secs = measured_at.elapsed().as_secs_f64();
    let cpu_end = procfs::snapshot(&a.pids);
    drop(senders);

    let mut tally = Tally::new();
    let mut failed = 0;
    for t in tasks {
        match t.await? {
            Ok(t) => tally.merge(t),
            Err(e) => {
                failed += 1;
                eprintln!("person failed: {e:#}");
            }
        }
    }

    let relay_cpu: f64 = a
        .pids
        .iter()
        .enumerate()
        .filter(|(_, (n, _))| n.starts_with("relay"))
        .map(|(i, _)| cpu_end[i] - cpu_start[i])
        .sum();
    let per_core = |n: u64| (relay_cpu > 0.0).then(|| (n as f64 / relay_cpu).round());
    let report = json!({
        "label": a.label,
        "config": {
            "relays": a.relays.len(), "users": a.users, "group": a.group, "rate": a.rate,
            "seconds": a.seconds, "warmup": a.warmup,
        },
        "connect": {
            "seconds": (connect_secs * 100.0).round() / 100.0,
            "handshakes_per_s": (a.users as f64 / connect_secs).round(),
            "handshake": latency(&handshake),
            "rss_kib_per_connection": rss_per_conn,
            "processes": per_process(&a.pids, &cpu_before, &cpu_connected, connect_secs),
        },
        "run": {
            "measured_seconds": (measured_secs * 100.0).round() / 100.0,
            "sent": tally.sent,
            "sent_per_s": (tally.sent as f64 / measured_secs).round(),
            "accepted": tally.accepted,
            "accepted_per_s": (tally.accepted as f64 / measured_secs).round(),
            "rejected": tally.rejected,
            "scheduler_backlogged": backlogged,
            "people_failed": failed,
            "deliveries": tally.delivered,
            "deliveries_expected": expected_deliveries,
            "deliveries_per_s": (tally.delivered as f64 / measured_secs).round(),
            "ack": latency(&tally.ack),
            "delivery": latency(&tally.delivery),
            "processes": per_process(&a.pids, &cpu_start, &cpu_end, measured_secs),
            "relay_cpu_seconds": (relay_cpu * 100.0).round() / 100.0,
            "messages_per_relay_core_second": per_core(tally.accepted),
            "deliveries_per_relay_core_second": per_core(tally.delivered),
        },
    });
    let text = serde_json::to_string_pretty(&report)?;
    println!("{text}");
    if let Some(path) = a.json {
        std::fs::write(&path, &text).with_context(|| path.clone())?;
    }
    if failed > 0 || tally.accepted < tally.sent * 99 / 100 {
        bail!(
            "{failed} people failed, {} of {} accepted",
            tally.accepted,
            tally.sent
        );
    }
    Ok(())
}

/// This load generator's own resident memory, so a run shows what it cost the machine.
fn own_rss() -> String {
    procfs::rss_bytes(std::process::id()).map_or_else(
        || "rss unknown".into(),
        |b| format!("{} MiB resident", b >> 20),
    )
}
