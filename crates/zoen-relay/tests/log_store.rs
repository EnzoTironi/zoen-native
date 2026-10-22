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

async fn concurrent_writers_on_one_space_build_one_gapless_chain() {
    let log = store();
    let a = Arc::new(Author::root(Signer::generate()));
    let (space, genesis) = create(&log, &a).await;
    let mut tasks = Vec::new();
    for w in 0..8 {
        let (log, a, space, genesis) = (log.clone(), a.clone(), space.clone(), genesis.clone());
        tasks.push(tokio::spawn(async move {
            for i in 0..40 {
                let env = sign(&a, &space, Some(genesis.clone()), message(w * 100 + i));
                assert!(matches!(
                    log.append(&env, true).await,
                    Ok(Sequencing::New { .. })
                ));
            }
        }));
    }
    for t in tasks {
        t.await.unwrap();
    }
    let events = read_all(&log, &space, 1000).await;
    assert_eq!(events.len(), 321);
    assert_chain(&space, &events);
    log.drop_cell().await.unwrap();
}

async fn catch_up_returns_every_event_past_partial_batches() {
    let log = store();
    let a = Author::root(Signer::generate());
    let (space, genesis) = create(&log, &a).await;
    for i in 0..1200 {
        log.append(&sign(&a, &space, Some(genesis.clone()), message(i)), true)
            .await
            .unwrap();
    }
    let events = read_all(&log, &space, 500).await;
    assert_eq!(
        events.len(),
        1201,
        "pages of 500 come back full until the end"
    );
    assert_chain(&space, &events);
    assert_eq!(log.read(&space, 1100, 500).await.unwrap().len(), 101);
    log.drop_cell().await.unwrap();
}

async fn a_retried_envelope_is_answered_with_the_stored_copy() {
    let log = store();
    let a = Author::root(Signer::generate());
    let (space, genesis) = create(&log, &a).await;
    let env = sign(&a, &space, Some(genesis), message(0));
    let Ok(Sequencing::New { ev: first, .. }) = log.append(&env, true).await else {
        panic!()
    };
    let Ok(Sequencing::Duplicate { ev, .. }) = log.append(&env, true).await else {
        panic!("not deduplicated")
    };
    assert_eq!((ev.seq, ev.hash), (first.seq, first.hash));
    assert_eq!(read_all(&log, &space, 500).await.len(), 2);
    log.drop_cell().await.unwrap();
}

async fn membership_changes_commit_with_the_event_that_makes_them() {
    let log = store();
    let ana = Author::root(Signer::generate());
    let bruno = Author::root(Signer::generate());
    let (space, genesis) = create(&log, &ana).await;
    let r = log
        .append(
            &sign(&bruno, &space, Some(genesis.clone()), message(0)),
            true,
        )
        .await;
    assert!(r.is_err_and(|r| r.reason.contains("not a member")));

    let add = EventBody::MemberAdded {
        identity: bruno.identity.clone(),
        role: Role::Member,
    };
    let Ok(Sequencing::New {
        audience, joined, ..
    }) = log
        .append(&sign(&ana, &space, Some(genesis.clone()), add), true)
        .await
    else {
        panic!("add refused")
    };
    assert_eq!(joined.as_deref(), Some(bruno.identity.as_str()));
    assert!(audience.contains(&bruno.identity) && audience.contains(&ana.identity));
    assert_eq!(
        log.role(&space, &bruno.identity).await.unwrap(),
        Some(Role::Member)
    );
    assert_eq!(
        log.spaces_of(&bruno.identity).await.unwrap(),
        vec![space.clone()]
    );
    assert!(log
        .append(
            &sign(&bruno, &space, Some(genesis.clone()), message(1)),
            true
        )
        .await
        .is_ok());

    let remove = EventBody::MemberRemoved {
        identity: bruno.identity.clone(),
    };
    let Ok(Sequencing::New { audience, .. }) = log
        .append(&sign(&ana, &space, Some(genesis.clone()), remove), true)
        .await
    else {
        panic!("remove refused")
    };
    assert!(
        audience.contains(&bruno.identity),
        "the removed member learns it"
    );
    assert!(log.spaces_of(&bruno.identity).await.unwrap().is_empty());
    assert!(log
        .append(&sign(&bruno, &space, Some(genesis), message(2)), true)
        .await
        .is_err());
    log.drop_cell().await.unwrap();
}

async fn invites_are_bounded_by_uses() {
    let log = store();
    let ana = Author::root(Signer::generate());
    let (space, _) = create(&log, &ana).await;
    let inv = log
        .create_invite(&ana.identity, &space, Role::Member, 1, 3600)
        .await
        .unwrap();
    let preview = log.preview_invite(&inv.code).await.unwrap();
    assert_eq!(
        (preview.space.as_str(), preview.members),
        (space.as_str(), 1)
    );

    let join = |who: &Author| {
        let mut env = sign(
            who,
            &space,
            None,
            EventBody::MemberAdded {
                identity: who.identity.clone(),
                role: Role::Member,
            },
        );
        env.invite = Some(inv.code.to_lowercase());
        env
    };
    let carla = Author::root(Signer::generate());
    assert!(matches!(
        log.append(&join(&carla), true).await,
        Ok(Sequencing::New { .. })
    ));
    let dani = Author::root(Signer::generate());
    assert!(
        log.append(&join(&dani), true).await.is_err(),
        "one use only"
    );
    assert!(log.preview_invite(&inv.code).await.is_err());
    let stored = read_all(&log, &space, 500).await;
    assert!(
        stored.iter().all(|e| e.env.invite.is_none()),
        "codes are never stored"
    );
    log.drop_cell().await.unwrap();
}

async fn two_storage_clients_share_one_chain_and_one_membership() {
    two_clients(false).await;
}

async fn two_storage_clients_keep_the_chain_during_explicit_lease_renewals() {
    tokio::time::timeout(std::time::Duration::from_secs(60), two_clients(true))
        .await
        .expect("bounded explicit-renewal contention contract");
}

async fn two_clients(renew_during_writes: bool) {
    let cell = types::new_id("t");
    let relays = [store_on(&cell), store_on(&cell)];
    let ana = Arc::new(Author::root(Signer::generate()));
    let (space, genesis) = create(&relays[0], &ana).await;
    let renewal_barrier = renew_during_writes.then(|| Arc::new(tokio::sync::Barrier::new(10)));
    let mut renewers = Vec::new();
    if let Some(barrier) = &renewal_barrier {
        // A fast baseline finishes before the normal five-second maintenance interval.
        // Explicitly overlap two independent owner clients with the second half of
        // the SAME eight writers; keep the production TTL/retry/deadline untouched.
        relays[1].owner.maintain().await.unwrap();
        for log in &relays {
            let (log, barrier) = (log.clone(), barrier.clone());
            renewers.push(tokio::spawn(async move {
                barrier.wait().await;
                for _ in 0..4 {
                    log.owner.maintain().await.expect("explicit owner renewal");
                }
            }));
        }
    }
    let mut tasks = Vec::new();
    for w in 0..8 {
        let log = relays[w % 2].clone();
        let (a, space, genesis) = (ana.clone(), space.clone(), genesis.clone());
        let renewal_barrier = renewal_barrier.clone();
        tasks.push(tokio::spawn(async move {
            for i in 0..40 {
                if i == 20 {
                    if let Some(barrier) = &renewal_barrier {
                        barrier.wait().await;
                    }
                }
                let env = sign(&a, &space, Some(genesis.clone()), message(w * 100 + i));
                let result = log.append(&env, true).await;
                assert!(
                    matches!(&result, Ok(Sequencing::New { .. })),
                    "concurrent append rejection: {:?}",
                    result.err()
                );
            }
        }));
    }
    for t in tasks {
        t.await.unwrap();
    }
    for renewal in renewers {
        renewal.await.unwrap();
    }
    let events = read_all(&relays[1], &space, 1000).await;
    assert_eq!(events.len(), 321);
    assert_chain(&space, &events);

    // Both relays now hold a warm cache of the Space. A change through one must bind the
    // other.
    let bruno = Author::root(Signer::generate());
    let add = EventBody::MemberAdded {
        identity: bruno.identity.clone(),
        role: Role::Member,
    };
    relays[0]
        .append(&sign(&ana, &space, Some(genesis.clone()), add), true)
        .await
        .unwrap();
    assert!(
        relays[1]
            .append(
                &sign(&bruno, &space, Some(genesis.clone()), message(1)),
                true
            )
            .await
            .is_ok(),
        "the other relay sees the new member"
    );
    let remove = EventBody::MemberRemoved {
        identity: bruno.identity.clone(),
    };
    relays[1]
        .append(&sign(&ana, &space, Some(genesis.clone()), remove), true)
        .await
        .unwrap();
    let r = relays[0]
        .append(&sign(&bruno, &space, Some(genesis), message(2)), true)
        .await;
    assert!(
        r.is_err_and(|r| r.reason.contains("not a member")),
        "and the removal"
    );
    assert_chain(&space, &read_all(&relays[0], &space, 1000).await);
    relays[0].drop_cell().await.unwrap();
}

async fn duplicates_in_one_batch_get_the_one_stored_copy() {
    // On a single-threaded runtime the Space's worker can't run until every copy below is
    // queued, so they meet in one batch every time.
    tokio::task::spawn_blocking(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(duplicates_in_one_batch())
    })
    .await
    .unwrap();
}

async fn duplicates_in_one_batch() {
    let log = store();
    let a = Arc::new(Author::root(Signer::generate()));
    let (space, genesis) = create(&log, &a).await;
    // Each copy is queued on its first poll, and `join_all` polls them all before yielding.
    let twin = sign(&a, &space, Some(genesis.clone()), message(0));
    let copies = futures_util::future::join_all((0..16).map(|_| log.append(&twin, true))).await;
    let mut new = 0;
    let mut stored = Vec::new();
    for c in copies {
        match c {
            Ok(Sequencing::New { ev, .. }) => {
                new += 1;
                stored.push((ev.seq, ev.hash));
            }
            Ok(Sequencing::Duplicate { ev, .. }) => stored.push((ev.seq, ev.hash)),
            Err(r) => panic!("refused: {}", r.reason),
        }
    }
    assert_eq!(new, 1, "one copy is sequenced");
    assert!(
        stored.windows(2).all(|w| w[0] == w[1]),
        "every copy answers with it"
    );
    let events = read_all(&log, &space, 1000).await;
    assert_eq!(events.len(), 2);
    assert_chain(&space, &events);
    log.drop_cell().await.unwrap();
}

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
