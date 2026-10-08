//! Append and catch-up throughput of the FoundationDB log store, end to end through
//! admission: every append is one real transaction with the real rules.
//!
//!   eval "$(scripts/fdb.sh env)"
//!   cargo run --release -p zoen-relay --example log_bench -- [spaces] [writers] [events]
//!
//! `spaces` = 1 measures one hot Space (every writer conflicts on its head); the default
//! spreads writers over 256 Spaces, the shape of real traffic. Events are signed up front
//! so the numbers are the store's, and the throwaway cell is dropped at the end.

use std::{sync::Arc, time::Instant};

use roda_log::{Author, Signer};
use roda_proto::Envelope;
use roda_types::{EventBody, Privacy, Seen, SpaceKind};
use zoen_relay::log::{fdb::FdbLog, LogStore, Sequencing};

fn arg(i: usize, default: usize) -> usize {
    std::env::args()
        .nth(i)
        .and_then(|a| a.parse().ok())
        .unwrap_or(default)
}

fn pct(sorted: &[u128], p: f64) -> f64 {
    sorted[((sorted.len() as f64 * p) as usize).min(sorted.len() - 1)] as f64 / 1000.0
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let (spaces, writers, total) = (arg(1, 256), arg(2, 64), arg(3, 20_000));
    let _network = unsafe { foundationdb::boot() };
    let cell = roda_types::new_id("bench");
    let log = Arc::new(FdbLog::open(
        std::env::var("FDB_CLUSTER_FILE").ok().as_deref(),
        &cell,
    )?);
    let author = Author::root(Signer::generate());

    let mut genesis = Vec::new();
    for _ in 0..spaces {
        let space = roda_types::new_id("sp");
        let created = author.sign_event(
            &space,
            &roda_types::new_ulid(now_ms()),
            now_ms(),
            None,
            EventBody::SpaceCreated {
                title: "bench".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::Closed,
            },
        );
        let Ok(Sequencing::New { ev, .. }) = log.append(&Envelope::plain(&created), true).await
        else {
            anyhow::bail!("genesis refused");
        };
        genesis.push((
            space,
            Seen {
                seq: ev.seq,
                hash: ev.hash,
            },
        ));
    }

    let mut queues: Vec<Vec<Envelope>> = vec![Vec::new(); writers];
    for i in 0..total {
        let (space, seen) = &genesis[i % spaces];
        let e = author.sign_event(
            space,
            &roda_types::new_ulid(now_ms()),
            now_ms(),
            Some(seen.clone()),
            EventBody::MessagePosted {
                message: format!("m{i}"),
                text: "x".repeat(200),
                attaches: None,
            },
        );
        queues[i % writers].push(Envelope::plain(&e));
    }

    let started = Instant::now();
    let mut tasks = Vec::new();
    for q in queues {
        let log = log.clone();
        tasks.push(tokio::spawn(async move {
            let mut lat = Vec::with_capacity(q.len());
            for env in q {
                let t = Instant::now();
                match log.append(&env, true).await {
                    Ok(Sequencing::New { .. }) => lat.push(t.elapsed().as_micros()),
                    Ok(Sequencing::Duplicate { .. }) => anyhow::bail!("unexpected duplicate"),
                    Err(r) => anyhow::bail!("refused: {}", r.reason),
                }
            }
            Ok(lat)
        }));
    }
    let mut lat = Vec::new();
    for t in tasks {
        lat.extend(t.await??);
    }
    let elapsed = started.elapsed().as_secs_f64();
    lat.sort_unstable();
    println!(
        "append  spaces={spaces} writers={writers} events={}  {:.0}/s  p50={:.2}ms p95={:.2}ms p99={:.2}ms max={:.2}ms",
        lat.len(),
        lat.len() as f64 / elapsed,
        pct(&lat, 0.50),
        pct(&lat, 0.95),
        pct(&lat, 0.99),
        pct(&lat, 1.0)
    );

    let started = Instant::now();
    let mut read = 0;
    for (space, _) in &genesis {
        let mut from = 0;
        loop {
            let page = log.read(space, from, 500).await?;
            read += page.len();
            match page.last() {
                Some(last) if page.len() == 500 => from = last.seq + 1,
                _ => break,
            }
        }
    }
    let elapsed = started.elapsed().as_secs_f64();
    println!(
        "catchup read={read}  {:.0} events/s (pages of 500, one Space at a time)",
        read as f64 / elapsed
    );

    log.drop_cell().await?;
    Ok(())
}
