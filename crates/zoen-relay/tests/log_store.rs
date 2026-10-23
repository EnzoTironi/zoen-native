//! The FoundationDB log store's contract, against a real cluster
//! (`eval "$(scripts/fdb.sh env)"`): gapless chains under concurrent writers, complete
//! catch-up past FoundationDB's partial batches, idempotent retries and atomic membership,
//! also when two relays write one Space and when one batch carries duplicates (ADR 0023).

use std::{future::Future, sync::Arc};

use event_log::{chain_hash, Author, Signer};
use protocol::{Envelope, Sequenced};
use types::{EventBody, Privacy, Role, Seen, SpaceKind, GENESIS_PREV};
use zoen_relay::log::{fdb::FdbLog, LogStore, Sequencing};

#[path = "log_store/mls_contracts.rs"]
mod mls_contracts;

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

fn store() -> Arc<FdbLog> {
    store_on(&types::new_id("t"))
}

/// Another storage client of the same owner: separate sequencer/cache, shared keys.
/// Distinct owners and failover are exercised by the ownership journey.
fn store_on(cell: &str) -> Arc<FdbLog> {
    Arc::new(
        FdbLog::open_as(
            std::env::var("FDB_CLUSTER_FILE").ok().as_deref(),
            cell,
            "log-contract",
            std::time::Duration::from_secs(15),
        )
        .expect("FoundationDB"),
    )
}

fn sign(a: &Author, space: &str, seen: Option<Seen>, body: EventBody) -> Envelope {
    Envelope::plain(&a.sign_event(space, &types::new_ulid(now_ms()), now_ms(), seen, body))
}

fn message(i: usize) -> EventBody {
    EventBody::MessagePosted {
        message: format!("m{i}"),
        text: format!("mensagem {i}"),
        attaches: None,
        reply: None,
        media: None,
        forwarded: false,
    }
}

async fn create(log: &FdbLog, a: &Author) -> (String, Seen) {
    log.owner.maintain().await.expect("initialize ownership");
    let space = types::new_id("sp");
    let body = EventBody::SpaceCreated {
        title: "t".into(),
        kind: SpaceKind::Group,
        privacy: Privacy::Closed,
    };
    match log.append(&sign(a, &space, None, body), true).await {
        Ok(Sequencing::New { ev, .. }) => (
            space,
            Seen {
                seq: ev.seq,
                hash: ev.hash,
            },
        ),
        Err(r) => panic!("genesis refused: {}", r.reason),
        _ => panic!("genesis answered as a duplicate"),
    }
}

fn assert_chain(space: &str, events: &[Sequenced]) {
    let mut prev = GENESIS_PREV.to_string();
    for (i, ev) in events.iter().enumerate() {
        assert_eq!(ev.seq, i as u64, "gapless");
        assert_eq!(ev.prev, prev, "linked");
        assert_eq!(
            ev.hash,
            chain_hash(space, ev.seq, &ev.prev, &ev.env.wire_hash()),
            "hashed"
        );
        prev = ev.hash.clone();
    }
}

async fn read_all(log: &FdbLog, space: &str, page: usize) -> Vec<Sequenced> {
    let mut out: Vec<Sequenced> = Vec::new();
    loop {
        let got = log.read(space, out.len() as u64, page).await.unwrap();
        let n = got.len();
        out.extend(got);
        if n < page {
            return out;
        }
    }
}

#[path = "log_store/ordering.rs"]
mod ordering;
use ordering::*;

/// ADR 0026 at the store: the privacy is part of the Space state, so a relay that never
/// saw the creation still refuses plaintext, and a checkpoint naming an entry long out of
/// the cache is checked against the stored chain.
async fn run<F: Future<Output = ()>>(name: &str, f: F) {
    f.await;
    println!("ok  {name}");
}

async fn co_members_deduplicates_across_read_batches() {
    let log = store();
    let ana = Author::root(Signer::generate());
    let bruno = Author::root(Signer::generate());
    // Cross the 16-read batch boundary and exercise repeated members in every space.
    for _ in 0..17 {
        let (space, genesis) = create(&log, &ana).await;
        let add = EventBody::MemberAdded {
            identity: bruno.identity.clone(),
            role: Role::Member,
        };
        log.append(&sign(&ana, &space, Some(genesis), add), true)
            .await
            .unwrap();
    }
    assert_eq!(
        log.co_members(&ana.identity).await.unwrap(),
        vec![bruno.identity.clone()]
    );
    assert_eq!(
        log.co_members(&bruno.identity).await.unwrap(),
        vec![ana.identity]
    );
    assert!(log.co_members("unknown").await.unwrap().is_empty());
    log.drop_cell().await.unwrap();
}

fn main() {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    assert!(
        arguments.is_empty()
            || arguments == ["--two-client"]
            || arguments == ["--two-client-renewals"],
        "usage: log_store [--two-client|--two-client-renewals]"
    );
    // The error at the FDB boundary is otherwise lost behind the retryable public
    // rejection. Test-owned stderr observes it without weakening the contract.
    tracing_subscriber::fmt()
        .with_env_filter("zoen_relay=error")
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();
    let network = unsafe { foundationdb::boot() };
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        if arguments == ["--two-client-renewals"] {
            run(
                "two storage clients keep the chain during explicit lease renewals",
                two_storage_clients_keep_the_chain_during_explicit_lease_renewals(),
            )
            .await;
            return;
        }
        if !arguments.is_empty() {
            run(
                "two storage clients of one owner share one chain and one membership",
                two_storage_clients_share_one_chain_and_one_membership(),
            )
            .await;
            return;
        }
        run(
            "concurrent writers on one space build one gapless chain",
            concurrent_writers_on_one_space_build_one_gapless_chain(),
        )
        .await;
        run(
            "catch-up returns every event past partial batches",
            catch_up_returns_every_event_past_partial_batches(),
        )
        .await;
        run(
            "a retried envelope is answered with the stored copy",
            a_retried_envelope_is_answered_with_the_stored_copy(),
        )
        .await;
        run(
            "membership changes commit with the event that makes them",
            membership_changes_commit_with_the_event_that_makes_them(),
        )
        .await;
        run(
            "co-members deduplicate across read batches",
            co_members_deduplicates_across_read_batches(),
        )
        .await;
        run("invites are bounded by uses", invites_are_bounded_by_uses()).await;
        run(
            "end-to-end spaces hold ciphertext on every relay",
            mls_contracts::end_to_end_spaces_hold_ciphertext_on_every_relay(),
        )
        .await;
        run(
            "one commit per epoch, and each welcome follows its commit",
            mls_contracts::one_commit_per_epoch_and_each_welcome_follows_its_commit(),
        )
        .await;
        run(
            "two storage clients of one owner share one chain and one membership",
            two_storage_clients_share_one_chain_and_one_membership(),
        )
        .await;
        run(
            "duplicates in one batch get the one stored copy",
            duplicates_in_one_batch_get_the_one_stored_copy(),
        )
        .await;
        run(
            "legacy checkpoint holds upgrade without expiring history",
            mls_contracts::legacy_checkpoint_holds_upgrade_without_expiring_history(),
        )
        .await;
    });
    drop(network);
}
