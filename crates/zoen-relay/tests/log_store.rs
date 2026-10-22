//! The FoundationDB log store's contract, against a real cluster
//! (`eval "$(scripts/fdb.sh env)"`): gapless chains under concurrent writers, complete
//! catch-up past FoundationDB's partial batches, idempotent retries and atomic membership,
//! also when two relays write one Space and when one batch carries duplicates (ADR 0023).

use std::{future::Future, sync::Arc};

use event_log::{chain_hash, Author, Signer};
use protocol::{Envelope, Sequenced};
use types::{EventBody, Privacy, Role, Seen, SpaceKind, GENESIS_PREV};
use zoen_relay::log::{fdb::FdbLog, LogStore, Sequencing};

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
async fn end_to_end_spaces_hold_ciphertext_on_every_relay() {
    let cell = types::new_id("t");
    let (warm, cold) = (store_on(&cell), store_on(&cell));
    let ana = Author::device(&Signer::generate(), Signer::generate());
    let space = types::new_id("sp");
    let body = EventBody::SpaceCreated {
        title: "t".into(),
        kind: SpaceKind::Group,
        privacy: Privacy::EndToEnd,
    };
    let Ok(Sequencing::New { ev, .. }) = warm.append(&sign(&ana, &space, None, body), true).await
    else {
        panic!("genesis refused");
    };
    let genesis = Seen {
        seq: ev.seq,
        hash: ev.hash,
    };
    let seal = |seen: &Seen| {
        let data = protocol::Sealed::new(protocol::SealedKind::Application, 3, vec![9; 48]);
        Envelope::sealed(
            &ana,
            &space,
            &types::new_ulid(now_ms()),
            now_ms(),
            Some(seen),
            data,
        )
    };
    for _ in 0..3 {
        assert!(matches!(
            warm.append(&seal(&genesis), true).await,
            Ok(Sequencing::New { .. })
        ));
    }
    let r = cold
        .append(&sign(&ana, &space, Some(genesis.clone()), message(0)), true)
        .await;
    assert!(
        r.is_err_and(|r| r.reason.contains("end-to-end")),
        "a cold relay refuses plaintext"
    );
    assert!(matches!(
        cold.append(&seal(&genesis), true).await,
        Ok(Sequencing::New { .. })
    ));

    let checkpoint = |upto: Seen| EventBody::Checkpoint {
        upto,
        epoch: 0,
        digest: "d".into(),
    };
    let fresh = store_on(&cell);
    let ok = fresh
        .append(
            &sign(
                &ana,
                &space,
                Some(genesis.clone()),
                checkpoint(genesis.clone()),
            ),
            true,
        )
        .await;
    assert!(
        matches!(ok, Ok(Sequencing::New { .. })),
        "an old upto is read from the chain"
    );
    let forged = Seen {
        seq: 0,
        hash: "f".repeat(64),
    };
    let r = fresh
        .append(
            &sign(&ana, &space, Some(genesis.clone()), checkpoint(forged)),
            true,
        )
        .await;
    assert!(
        r.is_err_and(|r| r.reason.contains("checkpoint")),
        "and must match it"
    );

    let events = read_all(&warm, &space, 100).await;
    assert_eq!(events.len(), 6);
    assert_chain(&space, &events);
    assert_eq!(events.iter().filter(|e| e.env.is_sealed()).count(), 4);
    warm.drop_cell().await.unwrap();
}

/// ADR 0026: two admins add Carla at once. Both commits were made at the same epoch; the
/// relay keeps the first, refuses the second and the Welcome that rode on it, and a cold
/// relay knows the epoch too.
async fn one_commit_per_epoch_and_each_welcome_follows_its_commit() {
    use mls::{sealed::state_key, Device};
    use protocol::{Sealed, SealedKind, STALE_COMMIT};
    let cell = types::new_id("t");
    let (warm, cold) = (store_on(&cell), store_on(&cell));
    let people: Vec<Author> = (0..3)
        .map(|_| Author::device(&Signer::generate(), Signer::generate()))
        .collect();
    let (ana, bruno, carla) = (&people[0], &people[1], &people[2]);
    let dbs: Vec<rusqlite::Connection> = (0..3)
        .map(|_| {
            let mut c = rusqlite::Connection::open_in_memory().unwrap();
            mls::migrate(&mut c).unwrap();
            c
        })
        .collect();
    let device = |i: usize| {
        let (a, secret) = (&people[i], people[i].key.secret());
        let cert = a.cert.as_deref().unwrap();
        Device::new(&dbs[i], state_key(&secret), &a.identity, secret, cert).unwrap()
    };
    let (d_ana, d_bruno, d_carla) = (device(0), device(1), device(2));
    let space = types::new_id("sp");
    let handshake = |a: &Author, kind, data: Vec<u8>, seen: &Seen| {
        let data = Sealed::new(kind, mls::SUITE_ID, data);
        let cid = types::new_ulid(now_ms());
        Envelope::sealed(a, &space, &cid, now_ms(), Some(seen), data)
    };
    let at = |ev: &Sequenced| Seen {
        seq: ev.seq,
        hash: ev.hash.clone(),
    };

    let created = EventBody::SpaceCreated {
        title: "t".into(),
        kind: SpaceKind::Group,
        privacy: Privacy::EndToEnd,
    };
    let mut head = at(&landed(
        warm.append(&sign(ana, &space, None, created), true).await,
    ));
    for (who, role) in [
        (&bruno.identity, Role::Admin),
        (&carla.identity, Role::Member),
    ] {
        let body = EventBody::MemberAdded {
            identity: who.clone(),
            role,
        };
        let env = sign(ana, &space, Some(head.clone()), body);
        head = at(&landed(warm.append(&env, true).await));
    }
    let listed: std::collections::BTreeSet<String> =
        people.iter().map(|p| p.identity.clone()).collect();

    // Ana's device starts the group and adds Bruno: the commit takes epoch 0.
    d_ana.create_group(&space).unwrap();
    let none = Default::default();
    let first = d_ana
        .commit(&space, &d_bruno.key_packages(1, false).unwrap(), &none)
        .unwrap();
    let with_context = |kind, data, epoch, head: &types::Seen| {
        let mut sealed = protocol::Sealed::new(kind, mls::SUITE_ID, data);
        sealed.recovery = event_log::recovery::RecoveryRef {
            version: 1,
            epoch,
            blob: "ab".repeat(32),
        }
        .encode();
        Envelope::sealed(ana, &space, &types::new_id("c"), 1, Some(head), sealed)
    };
    assert_eq!(
        refused(
            warm.append(
                &with_context(SealedKind::Commit, first.commit.clone(), 99, &head),
                true
            )
            .await
        ),
        "recovery context must follow its commit's epoch"
    );
    assert_eq!(
        refused(
            warm.append(
                &with_context(SealedKind::Application, vec![1], 1, &head),
                true
            )
            .await
        ),
        "recovery context must follow its commit's epoch"
    );
    let env = with_context(SealedKind::Commit, first.commit.clone(), 1, &head);
    head = at(&landed(warm.append(&env, true).await));
    let env = handshake(ana, SealedKind::Welcome, first.welcome.unwrap(), &head);
    let welcome = landed(warm.append(&env, true).await);
    head = at(&welcome);
    d_ana
        .open(&space, &first.commit, &listed, d_ana.leaf())
        .unwrap();
    let data = welcome.env.sealed_data().unwrap().1.to_vec();
    assert!(d_bruno.join(&space, &data, &listed).unwrap());

    // Both admins add Carla, both from epoch 1. Bruno's commit lands first.
    let by_bruno = d_bruno
        .commit(&space, &d_carla.key_packages(1, false).unwrap(), &none)
        .unwrap();
    let by_ana = d_ana
        .commit(&space, &d_carla.key_packages(1, false).unwrap(), &none)
        .unwrap();
    let env = handshake(bruno, SealedKind::Commit, by_bruno.commit, &head);
    head = at(&landed(warm.append(&env, true).await));
    let env = handshake(ana, SealedKind::Commit, by_ana.commit.clone(), &head);
    assert_eq!(refused(warm.append(&env, true).await), STALE_COMMIT);
    let env = handshake(ana, SealedKind::Welcome, by_ana.welcome.unwrap(), &head);
    assert_eq!(
        refused(warm.append(&env, true).await),
        "a welcome follows its own commit"
    );
    let env = handshake(bruno, SealedKind::Welcome, by_bruno.welcome.unwrap(), &head);
    head = at(&landed(warm.append(&env, true).await));
    // A relay that saw none of it reads the epoch from FoundationDB.
    let env = handshake(ana, SealedKind::Commit, by_ana.commit, &head);
    assert_eq!(refused(cold.append(&env, true).await), STALE_COMMIT);
    warm.drop_cell().await.unwrap();
}

fn landed(r: Result<Sequencing, zoen_relay::log::Reject>) -> Sequenced {
    match r {
        Ok(Sequencing::New { ev, .. }) => ev,
        Ok(_) => panic!("answered as a duplicate"),
        Err(e) => panic!("refused: {}", e.reason),
    }
}

fn refused(r: Result<Sequencing, zoen_relay::log::Reject>) -> String {
    match r {
        Err(e) => e.reason,
        Ok(_) => panic!("admitted"),
    }
}

async fn legacy_checkpoint_holds_upgrade_without_expiring_history() {
    use foundationdb::{
        tuple::{pack, unpack, Subspace},
        Database,
    };

    let cell = types::new_id("t");
    let warm = store_on(&cell);
    let ana = Author::root(Signer::generate());
    let (space, genesis) = create(&warm, &ana).await;
    let encrypted = landed(
        warm.append(
            &sign(&ana, &space, Some(genesis), EventBody::SpaceEncrypted),
            true,
        )
        .await,
    );
    let key =
        Subspace::all()
            .subspace(&("zoen", &cell))
            .pack(&("s", &space, "ck", &ana.identity, "*"));
    let db = Database::new(std::env::var("FDB_CLUSTER_FILE").ok().as_deref()).unwrap();
    db.run(|trx, _| {
        let key = key.clone();
        async move {
            trx.set(&key, &pack(&(encrypted.seq as i64)));
            Ok(())
        }
    })
    .await
    .unwrap();
    let before = now_ms();
    let cold = store_on(&cell);
    let joining = sign(
        &ana,
        &space,
        Some(Seen {
            seq: encrypted.seq,
            hash: encrypted.hash.clone(),
        }),
        EventBody::DeviceJoining {
            device: "a".repeat(64),
        },
    );
    assert!(matches!(
        cold.append(&joining, true).await,
        Ok(Sequencing::New { .. })
    ));
    let migrated = db
        .run(|trx, _| {
            let key = key.clone();
            async move { Ok(trx.get(&key, false).await?) }
        })
        .await
        .unwrap()
        .unwrap();
    let (seq, since): (i64, i64) = unpack(&migrated).unwrap();
    assert_eq!(seq, encrypted.seq as i64);
    assert!(
        since >= before && since <= now_ms(),
        "legacy hold starts its ceiling on upgrade"
    );
    let events = read_all(&cold, &space, 100).await;
    assert_chain(&space, &events);
    assert_eq!(events[1].env, encrypted.env);
    cold.drop_cell().await.unwrap();
}

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
    let network = unsafe { foundationdb::boot() };
    tokio::runtime::Runtime::new().unwrap().block_on(async {
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
            end_to_end_spaces_hold_ciphertext_on_every_relay(),
        )
        .await;
        run(
            "one commit per epoch, and each welcome follows its commit",
            one_commit_per_epoch_and_each_welcome_follows_its_commit(),
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
            legacy_checkpoint_holds_upgrade_without_expiring_history(),
        )
        .await;
    });
    drop(network);
}
