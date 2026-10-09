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
    let s = w.zoen("bruno", &["sync"]);
    assert!(s.contains("pending=0"), "{s}");
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
    std::thread::sleep(std::time::Duration::from_millis(2500));
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
    let w = World::new("m2kp").await;
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
    let watch = w.spawn_zoen("bruno", &["watch", "--for", "6"]);
    std::thread::sleep(std::time::Duration::from_millis(1500));
    w.zoen("ana", &["group", "Roda 49", "@bruno"]);
    let out = watch.wait_with_output().expect("watch");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(w.count(&sql).await, 32);

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
