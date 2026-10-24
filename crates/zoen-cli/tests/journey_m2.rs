//! Milestone 2 journey: an end-to-end group on the real relay, Postgres and FoundationDB,
//! where the relay holds only ciphertext (ADR 0026). Every `zoen` call is a fresh process,
//! so every step is also a kill-and-relaunch of that device.
//!
//!   ZOEN_TEST_PG=postgres://zoen@127.0.0.1:55432/postgres \
//!   eval "$(scripts/fdb.sh env)" && cargo test -p zoen-cli --test journey_m2

mod common;
use common::watch::Watch;
use common::*;
use sqlx::Connection;
use types::EventBody;

/// Every row of every table in the relay's database, as text, with bytea as hex.
async fn postgres_text(w: &World) -> String {
    let tables: Vec<String> = {
        let mut c = sqlx::postgres::PgConnection::connect(&w.db_url)
            .await
            .unwrap();
        sqlx::query_scalar(
            "SELECT table_name::text FROM information_schema.tables WHERE table_schema = 'public'",
        )
        .fetch_all(&mut c)
        .await
        .unwrap()
    };
    let mut all = String::new();
    for t in tables {
        all += &w
            .scalar(&format!(
                "SELECT coalesce(string_agg(row_to_json(t)::text, '\n'), '') FROM \"{t}\" t"
            ))
            .await;
    }
    all
}

fn keys(out: &str) -> (u64, String) {
    let mut epoch = None;
    let mut digest = None;
    for field in out.trim().split('\t') {
        match field.split_once('=') {
            Some(("epoch", v)) => epoch = v.parse().ok(),
            Some(("digest", v)) => digest = Some(v.to_string()),
            _ => {}
        }
    }
    (epoch.expect("epoch"), digest.expect("digest"))
}

/// The seqs of the relay's stored entries for `space` that are pruned stubs, checking each
/// one is a sealed entry with no MLS bytes left.
async fn pruned_seqs(w: &World, space: &str) -> Vec<u64> {
    w.events_in(space)
        .await
        .iter()
        .filter(|ev| ev.env.is_pruned())
        .map(|ev| {
            let (_, data) = ev
                .env
                .sealed_data()
                .expect("only sealed entries are pruned");
            assert!(data.is_empty(), "a stub keeps no ciphertext");
            // The author's signature still holds: it covers the header and the hash of the
            // MLS bytes, which the stub keeps.
            ev.env.verify().expect("a stub verifies like the original");
            ev.seq
        })
        .collect()
}

#[path = "journey_m2/linked_history.rs"]
mod linked_history;
#[path = "journey_m2/membership.rs"]
mod membership;
#[path = "journey_m2/privacy.rs"]
mod privacy;
#[path = "journey_m2/recovery.rs"]
mod recovery;
