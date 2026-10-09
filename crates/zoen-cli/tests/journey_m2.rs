//! Milestone 2 journey: an end-to-end group on the real relay, Postgres and FoundationDB,
//! where the relay holds only ciphertext (ADR 0026). Every `zoen` call is a fresh process,
//! so every step is also a kill-and-relaunch of that device.
//!
//!   ZOEN_TEST_PG=postgres://zoen@127.0.0.1:55432/postgres \
//!   eval "$(scripts/fdb.sh env)" && cargo test -p zoen-cli --test journey_m2

mod common;
use common::*;
use roda_types::EventBody;
use sqlx::Connection;

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

#[tokio::test]
async fn an_end_to_end_group_leaves_the_relay_only_ciphertext() {
    let w = World::new("m2").await;
    for (h, n) in [("ana", "Ana"), ("bruno", "Bruno"), ("carla", "Carla")] {
        w.init(h, n);
    }
    // Each device published its key packages when it signed up: 32 plus a last-resort one.
    assert_eq!(w.count("SELECT count(*) FROM key_packages").await, 3 * 33);

    let space = w.zoen("ana", &["group", "Cofre", "@bruno"]);
    let space = space.trim();
    // Ana's device claimed one of Bruno's packages and committed him into the group.
    assert_eq!(
        w.count("SELECT count(*) FROM key_packages WHERE NOT last_resort")
            .await,
        3 * 32 - 1
    );

    let secret = "a senha do cofre é tucano-4512";
    let reply = "anotado, só nós dois lemos isso";
    w.zoen("ana", &["send", "Cofre", secret]);
    assert_eq!(
        w.zoen("bruno", &["read", "Cofre"]).trim(),
        format!("Ana: {secret}")
    );
    w.zoen("bruno", &["send", "Cofre", reply]);
    assert_eq!(
        w.zoen("ana", &["read", "Cofre"]).trim(),
        format!("Ana: {secret}\nBruno: {reply}")
    );
    // Carla isn't in it.
    assert!(!w.zoen("carla", &["chats"]).contains("Cofre"));

    // ── what the relay holds ──
    let stored = w.events_in(space).await;
    let mut sealed = 0;
    for ev in &stored {
        match ev.env.body() {
            None => sealed += 1,
            Some(
                EventBody::SpaceCreated { .. }
                | EventBody::MemberAdded { .. }
                | EventBody::ProfileKeyShared { .. }
                | EventBody::Checkpoint { .. },
            ) => {}
            Some(other) => panic!("the relay stored plaintext: {other:?}"),
        }
    }
    assert_eq!(sealed, 4, "a commit, a welcome and two messages");
    let needles = [secret, reply, "tucano-4512", "anotado"];
    for ev in &stored {
        let bytes = String::from_utf8_lossy(ev.env.content()).to_string();
        for n in needles {
            assert!(!bytes.contains(n), "FoundationDB holds {n:?} in the clear");
        }
    }
    let pg = postgres_text(&w).await;
    for n in needles {
        assert!(!pg.contains(n), "Postgres holds {n:?} in the clear");
        assert!(
            !pg.contains(&hex::encode(n)),
            "Postgres holds {n:?} as bytes"
        );
    }

    // A stranger can't write into the group, and learns nothing about it.
    let mut mallory = RawClient::connect(&w.relay_url(), "mallory").await;
    let refused = mallory.publish_message(space, "texto aberto").await;
    assert!(refused.contains("not a member"), "{refused}");

    // ── members agree on the group ──
    let (ae, ad) = keys(&w.zoen("ana", &["keys", "Cofre"]));
    let (be, bd) = keys(&w.zoen("bruno", &["keys", "Cofre"]));
    assert_eq!((ae, &ad), (be, &bd), "one group");
    assert_eq!(ae, 1, "one commit so far");
    let checkpoints: Vec<(String, u64, String)> = stored
        .iter()
        .filter_map(|ev| match ev.env.body() {
            Some(EventBody::Checkpoint { epoch, digest, .. }) => {
                Some((ev.env.author().to_string(), epoch, digest))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        checkpoints.len(),
        2,
        "Ana after her commit, Bruno after joining"
    );
    assert!(checkpoints.iter().all(|(_, e, d)| *e == 1 && *d == ad));

    // Both devices re-verify every signature and the chain over ciphertext from disk.
    for who in ["ana", "bruno"] {
        let v = w.zoen(who, &["verify"]);
        assert!(!v.contains("BROKEN"), "{who}: {v}");
    }

    // ── a newcomer reads from their welcome on ──
    let code = w.zoen("ana", &["invite", "Cofre"]);
    let code = code.split('\t').next().unwrap().trim().to_string();
    w.zoen("carla", &["join", &format!("zoen://join/{code}")]);
    w.zoen("ana", &["sync"]); // Ana's device commits Carla in
    w.zoen("carla", &["sync"]);
    let later = "Carla chegou; daqui pra frente ela lê";
    w.zoen("bruno", &["send", "Cofre", later]);
    let carla = w.zoen("carla", &["read", "Cofre"]);
    assert_eq!(
        carla.trim(),
        format!("Bruno: {later}"),
        "nothing from before she joined"
    );
    let ana = w.zoen("ana", &["read", "Cofre"]);
    assert!(ana.contains(secret) && ana.contains(later), "{ana}");
    let (ce, cd) = keys(&w.zoen("carla", &["keys", "Cofre"]));
    let (be, bd) = keys(&w.zoen("bruno", &["keys", "Cofre"]));
    assert_eq!((ce, &cd), (be, &bd));
    assert_eq!(ce, 2);
    assert!(!w.zoen("carla", &["verify"]).contains("BROKEN"));
    let pg = postgres_text(&w).await;
    assert!(!pg.contains(later));
    for ev in w.events_in(space).await {
        assert!(!String::from_utf8_lossy(ev.env.content()).contains("daqui pra frente"));
    }

    // ── a member without MLS: listed, can't read, can't write in the clear ──
    let code = w.zoen("ana", &["invite", "Cofre"]);
    let code = code.split('\t').next().unwrap().trim().to_string();
    let joined = mallory.join(space, &code).await.expect("joins by invite");
    let seen = Some(roda_types::Seen {
        seq: joined.seq,
        hash: joined.hash.clone(),
    });
    let clear = EventBody::MessagePosted {
        message: "m".into(),
        text: "texto aberto".into(),
        attaches: None,
        reply: None,
    };
    let refused = mallory.publish_body(space, seen, clear).await.unwrap_err();
    assert_eq!(refused, roda_proto::SEAL_REQUIRED);
    // She has no key packages, so no commit can add her, and nobody's messages wait for her.
    let after = "seguimos sem esperar quem não tem chaves";
    w.zoen("ana", &["send", "Cofre", after]);
    assert!(w.zoen("bruno", &["read", "Cofre"]).contains(after));
    let (ae, _) = keys(&w.zoen("ana", &["keys", "Cofre"]));
    assert_eq!(ae, 2, "no commit for someone without packages");
}

/// ADR 0027: a relay-readable group from M1 keeps working, becomes end-to-end on request,
/// and can't be turned back.
#[tokio::test]
async fn a_readable_group_becomes_end_to_end_and_never_goes_back() {
    let w = World::new("m2up").await;
    w.init("ana", "Ana");
    w.init("bruno", "Bruno");
    let space = w.zoen("ana", &["group", "Trilha", "@bruno", "--readable"]);
    let space = space.trim();
    let before = "combinado antes: tucano-antes";
    w.zoen("ana", &["send", "Trilha", before]);
    assert!(w.zoen("bruno", &["read", "Trilha"]).contains(before));
    assert!(w.zoen("bruno", &["chats"]).contains("\treadable\t"));

    // Bruno writes offline, before he hears about the upgrade.
    let queued = "escrito antes de saber: tucano-fila";
    let q = w.zoen("bruno", &["send", "Trilha", queued, "--offline"]);
    assert!(q.starts_with("queued"), "{q}");

    w.zoen("ana", &["encrypt", "Trilha"]);
    assert!(w.zoen("ana", &["chats"]).contains("\te2e\t"));
    let after = "depois da troca: tucano-depois";
    w.zoen("ana", &["send", "Trilha", after]);

    // Bruno's clear message is refused, waits for his Welcome, and goes out sealed.
    w.sync_until("bruno", |s| s.contains("pending=0"));
    let bruno = w.zoen("bruno", &["read", "Trilha"]);
    for text in [before, after, queued, "End-to-end encryption is on"] {
        assert!(bruno.contains(text), "{text:?} in\n{bruno}");
    }
    assert!(w.zoen("bruno", &["chats"]).contains("\te2e\t"));
    assert!(w.zoen("ana", &["read", "Trilha"]).contains(queued));
    let (ae, ad) = keys(&w.zoen("ana", &["keys", "Trilha"]));
    let (be, bd) = keys(&w.zoen("bruno", &["keys", "Trilha"]));
    assert_eq!((ae, &ad), (be, &bd));
    assert_eq!(ae, 1);

    // What was said in the clear stays as it was; nothing after the upgrade is readable.
    let stored = w.events_in(space).await;
    let text = |ev: &roda_proto::Sequenced| String::from_utf8_lossy(ev.env.content()).to_string();
    assert!(stored.iter().any(|ev| text(ev).contains("tucano-antes")));
    let pg = postgres_text(&w).await;
    for n in ["tucano-fila", "tucano-depois"] {
        assert!(
            stored.iter().all(|ev| !text(ev).contains(n)),
            "FoundationDB holds {n}"
        );
        assert!(!pg.contains(n), "Postgres holds {n}");
    }

    // No way back: not by recreating the Space, not by "upgrading" again.
    let code = w.zoen("ana", &["invite", "Trilha"]);
    let code = code.split('\t').next().unwrap().trim().to_string();
    let mut mallory = RawClient::connect(&w.relay_url(), "mallory").await;
    let joined = mallory.join(space, &code).await.expect("joins by invite");
    let seen = Some(roda_types::Seen {
        seq: joined.seq,
        hash: joined.hash.clone(),
    });
    let recreate = EventBody::SpaceCreated {
        title: "Trilha".into(),
        kind: roda_types::SpaceKind::Group,
        privacy: roda_types::Privacy::Closed,
    };
    let refused = mallory.publish_body(space, seen.clone(), recreate).await;
    assert_eq!(refused.unwrap_err(), "space already exists");
    let again = mallory
        .publish_body(space, seen, EventBody::SpaceEncrypted)
        .await;
    assert_eq!(again.unwrap_err(), "already end-to-end");
    for who in ["ana", "bruno"] {
        let v = w.zoen(who, &["verify"]);
        assert!(!v.contains("BROKEN"), "{who}: {v}");
    }
}

/// A removed member reads nothing written after their removal, their device forgets the
/// group, and nobody seals a message before the commit that takes them out.
#[tokio::test]
async fn a_removed_member_reads_nothing_after_removal() {
    let w = World::new("m2rm").await;
    for (h, n) in [("ana", "Ana"), ("bruno", "Bruno"), ("carla", "Carla")] {
        w.init(h, n);
    }
    let space = w.zoen("ana", &["group", "Cofre", "@bruno", "@carla"]);
    let space = space.trim();
    let before = "todos leem isto";
    w.zoen("ana", &["send", "Cofre", before]);
    for who in ["bruno", "carla"] {
        assert!(w.zoen(who, &["read", "Cofre"]).contains(before), "{who}");
    }

    w.zoen("ana", &["remove", "Cofre", "@bruno"]);
    let after = "sem o Bruno: tucano-depois";
    w.zoen("ana", &["send", "Cofre", after]);
    let reply = "combinado, só nós: tucano-resposta";
    w.zoen("carla", &["send", "Cofre", reply]);

    let carla = w.zoen("carla", &["read", "Cofre"]);
    assert!(carla.contains(after) && carla.contains(reply), "{carla}");
    let bruno = w.zoen("bruno", &["read", "Cofre"]);
    assert!(bruno.contains(before), "{bruno}");
    assert!(!bruno.contains("tucano"), "{bruno}");
    // Bruno's device forgot the group's secrets.
    assert!(w.try_zoen("bruno", &["keys", "Cofre"]).is_err());
    let (ae, ad) = keys(&w.zoen("ana", &["keys", "Cofre"]));
    let (ce, cd) = keys(&w.zoen("carla", &["keys", "Cofre"]));
    assert_eq!((ae, &ad), (ce, &cd));
    assert_eq!(ae, 2, "the add, then the removal");

    // In the relay's order: the removal, the commit, and only then messages sealed for the
    // smaller group.
    let stored = w.events_in(space).await;
    let removed = stored
        .iter()
        .position(|ev| matches!(ev.env.body(), Some(EventBody::MemberRemoved { .. })))
        .expect("the removal");
    let commit = stored
        .iter()
        .skip(removed)
        .position(|ev| ev.env.sealed_kind() == Some(roda_proto::SealedKind::Commit))
        .map(|i| i + removed)
        .expect("the removal's commit");
    let messages_after: Vec<usize> = stored
        .iter()
        .enumerate()
        .skip(removed)
        .filter(|(_, ev)| ev.env.sealed_kind() == Some(roda_proto::SealedKind::Application))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(messages_after.len(), 2, "Ana's and Carla's");
    assert!(messages_after.iter().all(|i| *i > commit));
    for who in ["ana", "bruno", "carla"] {
        let v = w.zoen(who, &["verify"]);
        assert!(!v.contains("BROKEN"), "{who}: {v}");
    }
}

/// Two admins are online when someone joins by invite: both owe the commit that adds her.
/// They take turns (identity order), and the relay grants each epoch to one commit only, so
/// the group gets exactly one commit for her and everyone ends up in the same epoch.
#[tokio::test(flavor = "multi_thread")]
async fn two_admins_online_make_one_commit_for_a_newcomer() {
    let w = World::new("m2race").await;
    for (h, n) in [
        ("ana", "Ana"),
        ("bruno", "Bruno"),
        ("carla", "Carla"),
        ("dora", "Dora"),
    ] {
        w.init(h, n);
    }
    let space = w.zoen("ana", &["group", "Ninho", "@dora"]);
    let space = space.trim();
    w.zoen("ana", &["add", "Ninho", "@bruno", "--admin"]);
    assert!(w.zoen("bruno", &["chats"]).contains("Ninho"));
    let code = w.zoen("ana", &["invite", "Ninho"]);
    let code = code.trim();

    let watchers = [
        w.spawn_zoen("ana", &["watch", "--for", "8"]),
        w.spawn_zoen("bruno", &["watch", "--for", "8"]),
    ];
    w.wait_online(2);
    w.zoen("carla", &["join", &format!("zoen://join/{code}")]);
    for watcher in watchers {
        let out = watcher.wait_with_output().expect("watch");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "{stderr}");
    }

    let hello = "cheguei no ninho: tucano-oi";
    w.zoen("carla", &["send", "Ninho", hello]);
    for who in ["ana", "bruno", "dora"] {
        assert!(w.zoen(who, &["read", "Ninho"]).contains(hello), "{who}");
    }
    let (epoch, digest) = keys(&w.zoen("ana", &["keys", "Ninho"]));
    assert_eq!(epoch, 3, "Dora, Bruno, then Carla");
    for who in ["bruno", "carla", "dora"] {
        assert_eq!(
            keys(&w.zoen(who, &["keys", "Ninho"])),
            (epoch, digest.clone()),
            "{who}"
        );
    }

    let stored = w.events_in(space).await;
    let joined = stored
        .iter()
        .position(|ev| match ev.env.body() {
            // Joining by invite: the newcomer adds herself.
            Some(EventBody::MemberAdded { identity, .. }) => identity == ev.env.author(),
            _ => false,
        })
        .expect("Carla's join");
    let commits = stored
        .iter()
        .skip(joined)
        .filter(|ev| ev.env.sealed_kind() == Some(roda_proto::SealedKind::Commit))
        .count();
    assert_eq!(commits, 1, "one commit for Carla");
    for who in ["ana", "bruno", "carla", "dora"] {
        let v = w.zoen(who, &["verify"]);
        assert!(!v.contains("BROKEN"), "{who}: {v}");
    }
}

/// Two admins add people at once under a tight publish limit, so commits and Welcomes are
/// held back and race: a Welcome can land behind the other admin's commit (refused: the
/// newcomer's leaf is then taken out and she's added again), or a commit can lose its epoch
/// (dropped with its Welcome, then made again). Whatever happened, everyone ends in one
/// group and reads everyone.
#[tokio::test(flavor = "multi_thread")]
async fn admins_adding_at_once_under_a_publish_limit_converge() {
    // One publish every 6 s per device, two at once.
    let w = World::with_env("m2limit", &[("ZOEN_LIMITS", "publish_device=10/m:2")]).await;
    for (h, n) in [
        ("ana", "Ana"),
        ("bruno", "Bruno"),
        ("carla", "Carla"),
        ("dora", "Dora"),
        ("eva", "Eva"),
    ] {
        w.init(h, n);
    }
    let t = ["--timeout", "60000"];
    w.zoen("ana", &["group", "Ninho", "@dora", t[0], t[1]]);
    w.zoen("ana", &["add", "Ninho", "@bruno", "--admin", t[0], t[1]]);
    w.zoen("ana", &["sync", t[0], t[1]]);
    assert!(w.zoen("bruno", &["chats"]).contains("Ninho"));
    // Both buckets refill, then both admins add someone at the same moment.
    std::thread::sleep(std::time::Duration::from_secs(13));
    let ana = w.spawn_zoen("ana", &["add", "Ninho", "@carla", t[0], t[1]]);
    w.zoen("bruno", &["add", "Ninho", "@eva", t[0], t[1]]);
    let out = ana.wait_with_output().expect("ana");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    for who in ["ana", "bruno", "ana", "bruno"] {
        w.zoen(who, &["sync", t[0], t[1]]);
    }

    for (who, text) in [
        ("carla", "cheguei: tucano-carla"),
        ("eva", "cheguei: tucano-eva"),
    ] {
        w.zoen(who, &["send", "Ninho", text]);
        w.zoen(who, &["sync", t[0], t[1]]);
        for reader in ["ana", "bruno", "carla", "dora", "eva"] {
            assert!(
                w.zoen(reader, &["read", "Ninho"]).contains(text),
                "{reader} reads {who}"
            );
        }
    }
    let (epoch, digest) = keys(&w.zoen("ana", &["keys", "Ninho"]));
    for who in ["bruno", "carla", "dora", "eva"] {
        assert_eq!(
            keys(&w.zoen(who, &["keys", "Ninho"])),
            (epoch, digest.clone()),
            "{who}"
        );
    }
    for who in ["ana", "bruno", "carla", "dora", "eva"] {
        let v = w.zoen(who, &["verify"]);
        assert!(!v.contains("BROKEN"), "{who}: {v}");
    }
}

/// Bruno's device keeps 32 single-use key packages on the relay; every group that adds him
/// uses one. Away while 25 groups take him in, his device refills as it comes back; online
/// when the stock dips under 8, it refills right away. Every group still opens for him.
#[tokio::test(flavor = "multi_thread")]
async fn key_packages_refill_when_they_run_low() {
    // Fifty groups from one terminal in a minute or two: lift the per-IP connect and
    // lookup limits that would otherwise pace Ana, not what this journey is about.
    let mut w = World::with_env(
        "m2kp",
        &[(
            "ZOEN_LIMITS",
            "connect_ip=1000/m:1000,lookup_account=1000/m:1000",
        )],
    )
    .await;
    w.set_client_env("ZOEN_NET_DEBUG", "1");
    w.init("ana", "Ana");
    w.init("bruno", "Bruno");
    let bruno = w.id_of("bruno").await;
    let sql =
        format!("SELECT count(*) FROM key_packages WHERE identity = '{bruno}' AND NOT last_resort");
    assert_eq!(w.count(&sql).await, 32);

    // Bruno is away: 25 groups claim 25 of his packages.
    for i in 0..25 {
        w.zoen("ana", &["group", &format!("Roda {i}"), "@bruno"]);
    }
    assert_eq!(w.count(&sql).await, 7);
    // He comes back: the relay says he's low, his device publishes 25 more.
    w.zoen("bruno", &["sync"]);
    assert_eq!(w.count(&sql).await, 32);

    // Down to 8 while he isn't looking: nothing to do yet.
    for i in 25..49 {
        w.zoen("ana", &["group", &format!("Roda {i}"), "@bruno"]);
    }
    assert_eq!(w.count(&sql).await, 8);
    w.zoen("bruno", &["sync"]);
    assert_eq!(w.count(&sql).await, 8);

    // Online when one more group takes him under 8: he refills while it happens.
    // Wait for this client's catch-up, rather than a session count that can still include
    // the preceding sync. Keep it alive until the assertion; 120 s is only a failsafe.
    let log_path = w.dir.join("bruno-watch.log");
    let mut watch = w.spawn_zoen_logged("bruno", &["watch", "--for", "120"], "bruno-watch.log");
    let mut ready = false;
    for _ in 0..300 {
        let log = std::fs::read_to_string(&log_path).unwrap();
        ready = log.contains("watching as @bruno")
            && log
                .lines()
                .rev()
                .find(|line| line.starts_with("[zoen-net] connection="))
                .is_some_and(|line| line.contains("connection=online synced=true"));
        if ready || watch.try_wait().expect("watch status").is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    if !ready {
        let _ = watch.kill();
        let _ = watch.wait();
        panic!(
            "Bruno never finished starting the watcher:\n{}",
            std::fs::read_to_string(&log_path).unwrap()
        );
    }
    w.zoen("ana", &["group", "Roda 49", "@bruno"]);
    let mut stock = 0;
    for _ in 0..200 {
        stock = w.count(&sql).await;
        if stock == 32 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let status = watch.try_wait().expect("watch status");
    let _ = watch.kill();
    let _ = watch.wait();
    let log = std::fs::read_to_string(&log_path).unwrap();
    assert!(
        status.is_none(),
        "watcher exited before the refill assertion: {status:?}\n{log}"
    );
    assert_eq!(
        stock,
        32,
        "refilled while watching:\n{log}\n{}\n{}",
        w.metrics(),
        w.relay_log_text()
    );

    // A group made from a refilled package opens like any other, and so do the old ones.
    w.zoen("ana", &["group", "Roda nova", "@bruno"]);
    for chat in ["Roda nova", "Roda 0", "Roda 24", "Roda 49"] {
        let text = format!("oi na {chat}: tucano");
        w.zoen("ana", &["send", chat, &text]);
        assert!(w.zoen("bruno", &["read", chat]).contains(&text), "{chat}");
    }
    let v = w.zoen("bruno", &["verify"]);
    assert!(!v.contains("BROKEN"), "{v}");
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

/// ADR 0026: the relay prunes ciphertext every member already holds. Members keep their
/// history, someone added later holds pruning back until she checkpoints, and she still
/// links the whole chain, joins from her Welcome and reads on.
#[tokio::test]
async fn the_relay_prunes_what_every_member_holds() {
    let mut w = World::new("m2prune").await;
    // Checkpoints every 4 entries instead of 256, so a few messages reach pruning.
    w.set_client_env("ZOEN_CHECKPOINT_EVERY", "4");
    for (h, n) in [("ana", "Ana"), ("bruno", "Bruno"), ("carla", "Carla")] {
        w.init(h, n);
    }
    let space = w.zoen("ana", &["group", "Poda", "@bruno"]);
    let space = space.trim().to_string();
    let early = "antes da poda: tucano-velho";
    w.zoen("ana", &["send", "Poda", early]);
    assert!(w.zoen("bruno", &["read", "Poda"]).contains(early));
    let first = w
        .events_in(&space)
        .await
        .iter()
        .find(|ev| ev.env.sealed_kind() == Some(roda_proto::SealedKind::Application))
        .map(|ev| ev.seq)
        .expect("the first message is in the log");

    // Both talk, read and checkpoint; once both checkpoints pass it, the first message goes.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    let mut i = 0;
    while !pruned_seqs(&w, &space).await.contains(&first) {
        assert!(std::time::Instant::now() < deadline, "nothing was pruned");
        for who in ["ana", "bruno"] {
            w.zoen(who, &["send", "Poda", &format!("{who} {i}")]);
        }
        for who in ["ana", "bruno"] {
            w.sync_until(who, |s| s.contains("pending=0"));
        }
        i += 1;
    }

    // Only below what both members' devices hold: their latest checkpoints.
    let stored = w.events_in(&space).await;
    let mut reached: std::collections::HashMap<String, u64> = Default::default();
    for ev in &stored {
        if let Some(EventBody::Checkpoint { upto, .. }) = ev.env.body() {
            let at = reached.entry(ev.env.author().to_string()).or_default();
            *at = (*at).max(upto.seq);
        }
    }
    assert_eq!(reached.len(), 2, "both checkpointed: {reached:?}");
    let floor = *reached.values().min().unwrap();
    let pruned = pruned_seqs(&w, &space).await;
    assert!(
        pruned.iter().all(|s| *s < floor),
        "{pruned:?} below {floor}"
    );
    // Control events stay as they were, and can't be made into a stub: only sealed
    // content hashes as header plus the hash of its MLS bytes.
    for ev in stored.iter().filter(|ev| ev.env.body().is_some()) {
        assert!(!ev.env.is_pruned());
        assert!(ev.env.pruned().is_none());
    }

    // Members keep their history: it's on their devices.
    for who in ["ana", "bruno"] {
        assert!(w.zoen(who, &["read", "Poda"]).contains(early), "{who}");
        let v = w.zoen(who, &["verify"]);
        assert!(!v.contains("BROKEN"), "{who}: {v}");
    }

    // Carla is added: nothing from her addition on is pruned until she checkpoints.
    w.zoen("ana", &["add", "Poda", "@carla"]);
    let added = w
        .events_in(&space)
        .await
        .iter()
        .rev()
        .find(|ev| matches!(ev.env.body(), Some(EventBody::MemberAdded { .. })))
        .map(|ev| ev.seq)
        .unwrap();
    for i in 0..3 {
        for who in ["ana", "bruno"] {
            w.zoen(who, &["send", "Poda", &format!("{who} sem a Carla {i}")]);
            w.sync_until(who, |s| s.contains("pending=0"));
        }
    }
    let held = pruned_seqs(&w, &space).await;
    assert!(held.iter().all(|s| *s < added), "{held:?} vs {added}");

    // She links the chain over the stubs, joins from her Welcome and reads on.
    w.sync_until("carla", |s| s.contains("pending=0"));
    assert!(w.zoen("carla", &["chats"]).contains("Poda"));
    let late = "depois da poda: tucano-novo";
    w.zoen("ana", &["send", "Poda", late]);
    let carla = w.zoen("carla", &["read", "Poda"]);
    assert!(carla.contains(late), "{carla}");
    assert!(!carla.contains(early), "{carla}");
    let v = w.zoen("carla", &["verify"]);
    assert!(!v.contains("BROKEN"), "carla: {v}");
}

/// ADR 0027: a message written offline is sealed when it goes out, at the epoch the group is
/// at then, so it arrives readable however many commits happened meanwhile (MLS keeps only a
/// few past epochs; sealing at write time would have lost it past 4).
#[tokio::test]
async fn a_message_queued_offline_survives_many_commits() {
    let w = World::new("m2queue").await;
    let people = [
        ("ana", "Ana"),
        ("bruno", "Bruno"),
        ("carla", "Carla"),
        ("dora", "Dora"),
        ("eva", "Eva"),
        ("fabio", "Fabio"),
        ("gabi", "Gabi"),
    ];
    for (h, n) in people {
        w.init(h, n);
    }
    let space = w.zoen("ana", &["group", "Fila", "@bruno"]);
    let space = space.trim().to_string();
    w.sync_until("ana", |s| s.contains("pending=0"));
    w.sync_until("bruno", |s| s.contains("pending=0"));
    let (start, _) = keys(&w.zoen("bruno", &["keys", "Fila"]));

    let queued = "escrito sem rede: tucano-fila-longa";
    let q = w.zoen("bruno", &["send", "Fila", queued, "--offline"]);
    assert!(q.starts_with("queued"), "{q}");

    // Five commits while Bruno is away.
    for (h, _) in &people[2..] {
        w.zoen("ana", &["add", "Fila", &format!("@{h}")]);
        w.sync_until("ana", |s| s.contains("pending=0"));
    }
    let (now, _) = keys(&w.zoen("ana", &["keys", "Fila"]));
    assert!(now >= start + 5, "epoch {start} -> {now}");

    // He comes back: catches up through every commit, then seals and sends.
    w.sync_until("bruno", |s| s.contains("pending=0"));
    let (be, _) = keys(&w.zoen("bruno", &["keys", "Fila"]));
    assert_eq!(be, now);
    for (h, _) in people.iter().filter(|(h, _)| *h != "bruno") {
        w.sync_until(h, |s| s.contains("pending=0"));
        let read = w.zoen(h, &["read", "Fila"]);
        assert!(read.contains(queued), "{h}:\n{read}");
    }
    // And the relay only ever saw it sealed.
    let stored = w.events_in(&space).await;
    assert!(stored
        .iter()
        .all(|ev| !matches!(ev.env.body(), Some(EventBody::MessagePosted { .. }))));
}

/// ADR 0026, the ceiling: a member device that stops coming back stops holding pruning
/// back once its last checkpoint is older than the ceiling (30 days; 30 s here). When it
/// does come back, what it never fetched is gone: it asks for a fresh leaf, an admin takes
/// its old one out and adds it again, and it reads from its new Welcome on.
#[tokio::test]
async fn a_device_away_past_the_ceiling_rejoins_from_a_new_welcome() {
    let mut w = World::with_env("m2ceiling", &[("ZOEN_PRUNE_CEILING_SECS", "30")]).await;
    w.set_client_env("ZOEN_CHECKPOINT_EVERY", "4");
    // Devices in use say they are alive at every sync a second after their last checkpoint.
    w.set_client_env("ZOEN_CHECKPOINT_REFRESH_SECS", "1");
    for (h, n) in [("ana", "Ana"), ("bruno", "Bruno"), ("carla", "Carla")] {
        w.init(h, n);
    }
    let space = w.zoen("ana", &["group", "Teto", "@bruno", "@carla"]);
    let space = space.trim().to_string();
    let early = "antes do teto: sabia-laranjeira";
    w.zoen("ana", &["send", "Teto", early]);
    for who in ["bruno", "carla"] {
        assert!(w.zoen(who, &["read", "Teto"]).contains(early), "{who}");
    }
    let carla_saw = w.events_in(&space).await.last().map(|ev| ev.seq).unwrap();

    // Carla goes quiet past the ceiling. Ana and Bruno keep talking (and checkpointing):
    // her hold stops counting, so what she never fetched gets pruned.
    // Keep the active members checkpointing throughout Carla's absence. Sleeping past
    // the ceiling here expires Bruno too; Ana can then prune a message he still needs,
    // leaving his next send waiting for a rejoin while the admin is offline.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(90);
    let mut i = 0;
    while !pruned_seqs(&w, &space).await.iter().any(|s| *s > carla_saw) {
        assert!(
            std::time::Instant::now() < deadline,
            "nothing past Carla's place was pruned"
        );
        for who in ["ana", "bruno"] {
            w.zoen(who, &["send", "Teto", &format!("sem a Carla {who} {i}")]);
        }
        for who in ["ana", "bruno"] {
            w.sync_until(who, |s| s.contains("pending=0"));
        }
        i += 1;
    }

    // Carla is back: she finds stubs where her history should go on and asks to rejoin.
    w.sync_until("carla", |s| s.contains("pending=0"));
    let carla = w.id_of("carla").await;
    let asked = w.events_in(&space).await.iter().any(|ev| {
        ev.env.author() == carla && matches!(ev.env.body(), Some(EventBody::DeviceJoining { .. }))
    });
    assert!(asked, "Carla's device asked for a fresh leaf");

    // Ana (admin) takes the old leaf out and adds her again; Carla joins from the Welcome.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(90);
    let mut n = 0;
    loop {
        let probe = format!("de volta? {n}");
        w.zoen("ana", &["send", "Teto", &probe]);
        w.sync_until("ana", |s| s.contains("pending=0"));
        w.sync_until("carla", |s| s.contains("pending=0"));
        if w.zoen("carla", &["read", "Teto"]).contains(&probe) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "Carla never read a message again"
        );
        n += 1;
    }
    let after = "depois do teto: bem-te-vi";
    w.zoen("bruno", &["send", "Teto", after]);
    w.sync_until("bruno", |s| s.contains("pending=0"));
    w.sync_until("carla", |s| s.contains("pending=0"));
    let read = w.zoen("carla", &["read", "Teto"]);
    assert!(read.contains(after), "{read}");
    // Her own history stays; what was said while she was gone doesn't come back.
    assert!(read.contains(early), "{read}");
    assert!(!read.contains("sem a Carla ana 0"), "{read}");
    let v = w.zoen("carla", &["verify"]);
    assert!(!v.contains("BROKEN"), "{v}");
}

/// Reads a spawned `zoen`'s stdout line by line on a thread.
fn lines_of(child: &mut std::process::Child) -> std::sync::mpsc::Receiver<String> {
    use std::io::BufRead;
    let out = child.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for l in std::io::BufReader::new(out).lines().map_while(Result::ok) {
            if tx.send(l).is_err() {
                break;
            }
        }
    });
    rx
}

fn wait_line(rx: &std::sync::mpsc::Receiver<String>, prefix: &str) -> String {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        match rx.recv_timeout(left) {
            Ok(l) if l.starts_with(prefix) => return l[prefix.len()..].trim().to_string(),
            Ok(_) => {}
            Err(_) => panic!("no {prefix:?} line"),
        }
    }
}

fn finish(child: std::process::Child) -> (bool, String) {
    let out = child.wait_with_output().unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

/// ADR 0045: a second device of the same account. The new device shows a code; the
/// existing one sends it the account over the relay (sealed to the code), adds it to every
/// end-to-end group, and hands it the recent history in encrypted chunks it downloads (and
/// resumes) and then deletes. Older pages come from the existing device on demand while it
/// is online. The relay sees no plaintext at any point; an unlinked device loses access.
#[tokio::test]
async fn a_linked_device_gets_the_history_and_loses_access_when_unlinked() {
    let mut w = World::new("m2link").await;
    w.set_client_env("ZOEN_CHECKPOINT_EVERY", "4");
    w.set_client_env("ZOEN_HISTORY_RECENT", "3");
    w.set_client_env("ZOEN_HISTORY_PAGE", "3");
    w.set_client_env("ZOEN_TRANSFER_CHUNK", "700");
    for (h, n) in [("ana", "Ana"), ("bruno", "Bruno")] {
        w.init(h, n);
    }
    let space = w
        .zoen("ana", &["group", "Casa", "@bruno"])
        .trim()
        .to_string();
    let before: Vec<String> = (0..9)
        .map(|i| format!("antes do notebook {i} jabuticaba"))
        .collect();
    for (i, m) in before.iter().enumerate() {
        let who = if i % 2 == 0 { "ana" } else { "bruno" };
        w.zoen(who, &["send", "Casa", m]);
        w.sync_until(if who == "ana" { "bruno" } else { "ana" }, |s| {
            s.contains("pending=0")
        });
    }
    for who in ["ana", "bruno"] {
        w.sync_until(who, |s| s.contains("pending=0"));
    }
    // The relay already pruned some of it: the new device can only get it from Ana's phone.
    assert!(
        !pruned_seqs(&w, &space).await.is_empty(),
        "nothing was pruned before the link"
    );

    // ── link: the new device shows a code; Ana's device takes it ──
    let mut b = w.spawn_zoen_env(
        "ana-notebook",
        &["link-request", "--for", "120"],
        &[("ZOEN_TRANSFER_STOP_AFTER", "1")],
    );
    let rx = lines_of(&mut b);
    let code = wait_line(&rx, "code:");
    let check_b = wait_line(&rx, "check:");
    assert!(code.starts_with("zoen-link:1:"), "{code}");
    let linked = w.zoen("ana", &["link", &code]);
    let devices_in = |who: &str| -> String {
        w.zoen(who, &["keys", "Casa"])
            .split('\t')
            .find_map(|f| f.trim().strip_prefix("devices=").map(str::to_string))
            .unwrap()
    };
    assert_eq!(
        devices_in("ana"),
        "3",
        "Ana's phone and notebook, and Bruno"
    );
    assert!(linked.contains(&format!("check: {check_b}")), "{linked}");
    assert!(linked.contains("history sent: "), "{linked}");
    let device_b = wait_line(&rx, "linked:");
    assert!(device_b.starts_with("@ana "), "{device_b}");
    let device_b = device_b.rsplit(' ').next().unwrap().to_string();
    // The transfer stopped after one chunk (as if the network dropped).
    let (ok, err) = finish(b);
    assert!(!ok, "the first download was cut short");
    assert!(err.contains("history stopped at 1/"), "{err}");
    let chunks = w
        .blobs()
        .into_iter()
        .filter(|(p, _)| p.to_string_lossy().contains("transfers"))
        .collect::<Vec<_>>();
    assert!(chunks.len() > 1, "the history went up in several chunks");

    // ── it resumes, and only the recent window arrives at first ──
    let h = w.zoen("ana-notebook", &["history"]);
    assert!(h.contains("(1 resumed)"), "{h}");
    assert!(
        !w.blobs()
            .iter()
            .any(|(p, _)| p.to_string_lossy().contains("transfers")),
        "the chunks are gone from the relay once downloaded"
    );
    let read = w.zoen("ana-notebook", &["read", "Casa"]);
    for m in &before[6..] {
        assert!(read.contains(m.as_str()), "recent {m:?} missing:\n{read}");
    }
    assert!(!read.contains(&before[0]), "{read}");

    // ── older pages, from Ana's phone while it is online ──
    let mut watch = w.spawn_zoen("ana", &["watch", "--for", "90"]);
    w.wait_online(1);
    let read = w.zoen("ana-notebook", &["read", "Casa", "--older"]);
    assert!(read.contains("(older: 3 more, still more)"), "{read}");
    for m in &before[3..] {
        assert!(read.contains(m.as_str()), "{m:?} missing:\n{read}");
    }
    assert!(!read.contains(&before[0]), "{read}");
    let _ = watch.kill();
    let _ = watch.wait();
    // With the phone away, a gentle state instead of a page.
    let offline = w.zoen("ana-notebook", &["read", "Casa", "--older"]);
    assert!(
        offline.contains("Abra o Zoen no celular") || offline.contains("Open Zoen on your phone"),
        "{offline}"
    );
    // The phone is back: the rest arrives, and then there is nothing older to ask for.
    let mut watch = w.spawn_zoen("ana", &["watch", "--for", "90"]);
    w.wait_online(1);
    let read = w.zoen("ana-notebook", &["read", "Casa", "--older"]);
    assert!(read.contains("(older: 3 more)"), "{read}");
    for m in &before {
        assert!(read.contains(m.as_str()), "{m:?} missing:\n{read}");
    }
    let _ = watch.kill();
    let _ = watch.wait();
    let done = w.zoen("ana-notebook", &["read", "Casa", "--older"]);
    assert!(done.contains("(older: 0 more)"), "{done}");

    // ── both devices get what's new ──
    let after = "depois do link: maracujá";
    w.zoen("bruno", &["send", "Casa", after]);
    w.sync_until("bruno", |s| s.contains("pending=0"));
    for who in ["ana", "ana-notebook"] {
        w.sync_until(who, |s| s.contains("pending=0"));
        assert!(w.zoen(who, &["read", "Casa"]).contains(after), "{who}");
    }
    let from_b = "escrito no notebook: goiaba";
    w.zoen("ana-notebook", &["send", "Casa", from_b]);
    for who in ["bruno", "ana"] {
        w.sync_until(who, |s| s.contains("pending=0"));
        assert!(w.zoen(who, &["read", "Casa"]).contains(from_b), "{who}");
    }

    // ── the relay never held plaintext: log, database, object store ──
    let mut needles: Vec<String> = before.clone();
    needles.extend([after.to_string(), from_b.to_string(), "jabuticaba".into()]);
    for ev in w.events_in(&space).await {
        let bytes = String::from_utf8_lossy(ev.env.content()).to_string();
        for n in &needles {
            assert!(!bytes.contains(n.as_str()), "FoundationDB holds {n:?}");
        }
    }
    let pg = postgres_text(&w).await;
    for (_, blob) in chunks.iter().chain(w.blobs().iter()) {
        let text = String::from_utf8_lossy(blob);
        for n in &needles {
            assert!(!text.contains(n.as_str()), "a blob holds {n:?}");
        }
    }
    for n in &needles {
        assert!(!pg.contains(n.as_str()), "Postgres holds {n:?}");
        assert!(!pg.contains(&hex::encode(n)), "Postgres holds {n:?} as hex");
    }

    // ── unlink: the notebook is refused and reads nothing new ──
    let devices = w.zoen("ana", &["devices"]);
    assert_eq!(devices.lines().count(), 2, "{devices}");
    w.zoen("ana", &["unlink", &device_b]);
    assert!(w.zoen("ana", &["devices"]).contains(" unlinked"));
    let secret = "só no celular: pitanga";
    w.zoen("ana", &["send", "Casa", secret]);
    w.sync_until("bruno", |s| s.contains("pending=0"));
    assert!(w.zoen("bruno", &["read", "Casa"]).contains(secret));
    assert!(
        w.try_zoen("ana-notebook", &["sync"]).is_err(),
        "an unlinked device can't log in"
    );
    let read = w.zoen("ana-notebook", &["read", "Casa"]);
    assert!(!read.contains(secret), "{read}");
    // Its leaf left the group: Ana's commit took it out, and Bruno agrees.
    assert_eq!(devices_in("ana"), "2");
    assert_eq!(devices_in("bruno"), "2");
    let (epoch, digest) = keys(&w.zoen("ana", &["keys", "Casa"]));
    assert_eq!(keys(&w.zoen("bruno", &["keys", "Casa"])), (epoch, digest));
    let v = w.zoen("ana", &["verify"]);
    assert!(!v.contains("BROKEN"), "{v}");
}
