use super::*;

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
        .filter(|ev| ev.env.sealed_kind() == Some(protocol::SealedKind::Commit))
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
        &[
            (
                "ZOEN_LIMITS",
                "connect_ip=1000/m:1000,lookup_account=1000/m:1000",
            ),
            ("RUST_LOG", "zoen_relay=debug"),
        ],
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
        w.zoen("ana", &["group", &format!("Grupo {i}"), "@bruno"]);
    }
    assert_eq!(w.count(&sql).await, 7);
    // He comes back: the relay says he's low, his device publishes 25 more.
    w.zoen("bruno", &["sync"]);
    assert_eq!(w.count(&sql).await, 32);

    // Down to 8 while he isn't looking: nothing to do yet.
    for i in 25..49 {
        w.zoen("ana", &["group", &format!("Grupo {i}"), "@bruno"]);
    }
    assert_eq!(
        w.count(&sql).await,
        8,
        "stock after 24 new groups:\n{}",
        w.relay_log_text()
    );
    w.zoen("bruno", &["sync"]);
    assert_eq!(w.count(&sql).await, 8);

    // Online when one more group takes him under 8: he refills while it happens.
    // Wait for this client's catch-up, rather than a session count that can still include
    // the preceding sync. Keep it alive until the assertion; 120 s is only a failsafe.
    let log_path = w.dir.join("bruno-watch.log");
    let status_path = w.dir.join("bruno-watch.stderr.log");
    let mut watch = w.spawn_zoen_logged_streams(
        "bruno",
        &["watch", "--for", "120"],
        "bruno-watch.log",
        "bruno-watch.stderr.log",
    );
    let mut ready = false;
    for _ in 0..300 {
        let log = std::fs::read_to_string(&log_path).unwrap();
        let status_log = std::fs::read_to_string(&status_path).unwrap();
        ready = log.contains("watching as @bruno")
            && status_log
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
            "Bruno never finished starting the watcher:\nstdout:\n{}\nstderr:\n{}",
            std::fs::read_to_string(&log_path).unwrap(),
            std::fs::read_to_string(&status_path).unwrap()
        );
    }
    w.zoen("ana", &["group", "Grupo 49", "@bruno"]);
    let mut stock = 0;
    for _ in 0..200 {
        stock = w.count(&sql).await;
        if stock == 32 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let status = watch.try_wait().expect("watch status");
    let metrics = w.metrics();
    let _ = watch.kill();
    let _ = watch.wait();
    let log = format!(
        "stdout:\n{}\nstderr:\n{}",
        std::fs::read_to_string(&log_path).unwrap(),
        std::fs::read_to_string(&status_path).unwrap()
    );
    if std::env::var_os("ZOEN_NET_DEBUG").is_some() {
        for line in log.lines() {
            eprintln!("[bruno] {line}");
        }
    }
    assert!(
        status.is_none(),
        "watcher exited before the refill assertion: {status:?}\n{log}"
    );
    assert_eq!(
        stock,
        32,
        "refilled while watching:\n{log}\n{}\n{}",
        metrics,
        w.relay_log_text()
    );

    // A group made from a refilled package opens like any other, and so do the old ones.
    w.zoen("ana", &["group", "Grupo novo", "@bruno"]);
    for chat in ["Grupo novo", "Grupo 0", "Grupo 24", "Grupo 49"] {
        let text = format!("oi na {chat}: tucano");
        w.zoen("ana", &["send", chat, &text]);
        assert!(w.zoen("bruno", &["read", chat]).contains(&text), "{chat}");
    }
    let v = w.zoen("bruno", &["verify"]);
    assert!(!v.contains("BROKEN"), "{v}");
}
