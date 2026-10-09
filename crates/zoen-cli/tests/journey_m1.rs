//! Milestone 1 journeys: real relay process, real Postgres, real `zoen` processes.
//! Each `zoen` call is a fresh process (open → unlock → connect → act → exit), so every
//! step is also a kill-and-relaunch.
//!
//! Needs a Postgres the tests can create databases in:
//!   ZOEN_TEST_PG=postgres://zoen@127.0.0.1:55432/postgres cargo test -p zoen-cli --test journey_m1
//! The harness builds the relay from this checkout first.

mod common;
use common::*;
use roda_proto::SealedKind;
use std::{path::PathBuf, process::Child, time::Duration};

struct Watch {
    child: Child,
    log: PathBuf,
}

impl Watch {
    async fn until(&mut self, expected: &str, observed: impl Fn(&str) -> bool) -> String {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        loop {
            let log = std::fs::read_to_string(&self.log).expect("watch log");
            let status = self.child.try_wait().expect("watch status");
            assert!(
                status.is_none(),
                "watch exited while waiting for {expected}: {status:?}\n{log}"
            );
            if observed(&log) {
                return log;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "watch never observed {expected}:\n{log}"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[tokio::test]
async fn dm_roundtrip_survives_relaunch_and_verifies() {
    let w = World::new("dm").await;
    w.init("ana", "Ana");
    w.init("bruno", "Bruno");

    // Not a contact yet: the directory knows only the @handle (ADR 0016).
    let found = w.zoen("ana", &["people", "bru"]);
    assert!(found.contains("@bruno\t@bruno"), "{found}");

    w.zoen("ana", &["dm", "@bruno", "oi Bruno, tudo bem?"]);
    let chats = w.zoen("bruno", &["chats"]);
    assert!(
        chats.contains("Ana\tDirect\trelay\te2e\tunread=1\toi Bruno, tudo bem?"),
        "{chats}"
    );

    w.zoen("bruno", &["send", "@ana", "tudo ótimo! e aí?"]);
    let ana_sees = w.zoen("ana", &["read", "@bruno"]);
    assert_eq!(
        ana_sees.trim(),
        "Ana: oi Bruno, tudo bem?\nBruno: tudo ótimo! e aí?"
    );

    // Both devices re-verify every signature and the relay's chain from disk.
    assert!(!w.zoen("ana", &["verify"]).contains("BROKEN"));
    assert!(!w.zoen("bruno", &["verify"]).contains("BROKEN"));
    assert_eq!(
        w.events().await.len(),
        10,
        "SpaceCreated, MemberAdded, a profile key share each way, the commit and Welcome, \
         a checkpoint each, 2 sealed messages"
    );
}

#[tokio::test]
async fn group_and_invite_code() {
    let w = World::new("group").await;
    for (h, n) in [("ana", "Ana"), ("bruno", "Bruno"), ("carla", "Carla")] {
        w.init(h, n);
    }
    w.zoen("ana", &["group", "Trilha sábado", "@bruno", "--readable"]);
    w.zoen("ana", &["send", "Trilha sábado", "bora fazer trilha?"]);
    assert!(w
        .zoen("bruno", &["read", "Trilha sábado"])
        .contains("Ana: bora fazer trilha?"));

    // Carla isn't a member: she has no such chat.
    assert!(!w.zoen("carla", &["chats"]).contains("Trilha sábado"));

    let code = w.zoen("ana", &["invite", "Trilha sábado"]);
    let code = code.split('\t').next().unwrap().trim().to_string();
    assert_eq!(code.len(), 10);
    w.zoen("carla", &["join", &format!("zoen://join/{code}")]);
    w.zoen(
        "carla",
        &["send", "Trilha sábado", "cheguei! eu levo lanche"],
    );
    w.zoen("bruno", &["send", "Trilha sábado", "fechado"]);

    let ana = w.zoen("ana", &["read", "Trilha sábado"]);
    assert_eq!(
        ana.trim(),
        "Ana: bora fazer trilha?\nCarla: cheguei! eu levo lanche\nBruno: fechado"
    );
    // Carla joined late and still gets the whole history of the relay-readable group.
    // (In an end-to-end group she reads from her Welcome on: journey_m2.)
    let carla = w.zoen("carla", &["read", "Trilha sábado"]);
    assert_eq!(carla, ana);
}

#[tokio::test]
async fn offline_writes_flush_after_the_relay_comes_back() {
    let mut w = World::new("offline").await;
    w.init("ana", "Ana");
    w.init("bruno", "Bruno");
    w.zoen("ana", &["dm", "@bruno", "primeira"]);
    // Bruno's app was open and got the chat before the connection dropped.
    assert_eq!(w.zoen("bruno", &["read", "@ana"]).trim(), "Ana: primeira");

    w.stop_relay();
    let pending = |status: &str| {
        status
            .split_whitespace()
            .find_map(|field| field.strip_prefix("pending="))
            .expect("queue count")
            .parse::<usize>()
            .expect("numeric queue count")
    };
    let before = pending(&w.zoen("bruno", &["status", "--offline"]));
    let q = w.zoen(
        "bruno",
        &["send", "@ana", "escrita sem internet", "--offline"],
    );
    assert!(q.starts_with("queued"), "{q}");
    let local = w.zoen("bruno", &["read", "@ana", "--offline"]);
    assert!(
        local.contains("Bruno: escrita sem internet (sending)"),
        "{local}"
    );
    // Relaunching while offline keeps it queued (outbox survives the process).
    let status = w.zoen("bruno", &["status", "--offline"]);
    assert_eq!(pending(&status), before + 1, "{status}");
    // A group created and written in while offline: the message is signed on top of a
    // genesis the relay hasn't seen yet (its link is known in advance).
    w.zoen("bruno", &["group", "Ideias", "--offline"]);
    w.zoen("bruno", &["send", "Ideias", "anotar isso", "--offline"]);

    w.start_relay();
    let s = w.zoen("bruno", &["sync"]);
    assert!(s.contains("pending=0"), "{s}");
    assert_eq!(
        w.zoen("bruno", &["read", "Ideias"]).trim(),
        "Bruno: anotar isso"
    );
    assert!(!w.zoen("bruno", &["verify"]).contains("BROKEN"));
    assert_eq!(
        w.zoen("ana", &["read", "@bruno"]).trim(),
        "Ana: primeira\nBruno: escrita sem internet"
    );
    // Sent exactly once.
    assert_eq!(
        w.events()
            .await
            .iter()
            .filter(|e| e.env.sealed_kind() == Some(SealedKind::Application))
            .count(),
        3
    );
}

#[tokio::test]
async fn catch_up_after_being_away() {
    let w = World::new("catchup").await;
    w.init("ana", "Ana");
    w.init("bruno", "Bruno");
    w.zoen("ana", &["dm", "@bruno", "msg 0"]);
    // Bruno's app isn't running while Ana sends a burst.
    for i in 1..=25 {
        w.zoen(
            "ana",
            &["send", "@bruno", &format!("msg {i}"), "--timeout", "4000"],
        );
    }
    let bruno = w.zoen("bruno", &["read", "@ana"]);
    let lines: Vec<&str> = bruno.lines().collect();
    assert_eq!(lines.len(), 26, "{bruno}");
    for (i, l) in lines.iter().enumerate() {
        assert_eq!(*l, format!("Ana: msg {i}"));
    }
}

#[tokio::test]
async fn typing_reaches_the_other_person_and_is_never_stored() {
    let mut w = World::new("typing").await;
    w.set_client_env("ZOEN_NET_DEBUG", "1");
    w.init("ana", "Ana");
    w.init("bruno", "Bruno");
    w.zoen("ana", &["dm", "@bruno", "oi"]);
    w.zoen("bruno", &["sync"]);
    let before = w.events().await.len();

    // Startup and the typing/sending CLI calls must not consume a fixed watcher
    // lifetime. Start only after this listener caught up, and stop on receipt.
    let mut watcher = Watch {
        child: w.spawn_zoen_logged("bruno", &["watch"], "bruno-typing-watch.log"),
        log: w.dir.join("bruno-typing-watch.log"),
    };
    watcher
        .until("Bruno online and synced", |log| {
            log.contains("watching as @bruno")
                && log
                    .lines()
                    .rev()
                    .find(|line| line.starts_with("[zoen-net] connection="))
                    .is_some_and(|line| line.contains("connection=online synced=true"))
        })
        .await;
    w.zoen("ana", &["typing", "@bruno", "--for", "2"]);
    w.zoen("ana", &["send", "@bruno", "chegando!"]);
    let out = watcher
        .until("typing and the following message", |log| {
            log.contains("[Ana] Ana is typing…") && log.contains("[Ana] Ana: chegando!")
        })
        .await;
    drop(watcher);
    assert!(out.contains("[Ana] Ana is typing…"), "{out}");
    assert!(out.contains("[Ana] Ana: chegando!"), "{out}");
    assert_eq!(
        w.events().await.len(),
        before + 1,
        "typing never touches the log"
    );
}

#[tokio::test]
async fn relay_refuses_writes_from_non_members_and_forgeries() {
    let w = World::new("authz").await;
    w.init("ana", "Ana");
    w.init("bruno", "Bruno");
    w.zoen("ana", &["group", "Só nós", "@bruno"]);
    let space = w
        .spaces_of("ana")
        .await
        .into_iter()
        .next()
        .expect("the group");

    // Mallory registers with her own keys, then tries to post into the group.
    let mut m = RawClient::connect(&w.relay_url(), "mallory").await;
    let r = m.publish_message(&space, "não sou membro").await;
    assert!(r.contains("not a member"), "{r}");

    // And to impersonate Ana by putting Ana's id as author.
    let ana_id = w.id_of("ana").await;
    let r = m.publish_as(&space, &ana_id, "sou a Ana").await;
    assert!(r.contains("only publish as yourself"), "{r}");

    assert!(!w.zoen("bruno", &["read", "Só nós"]).contains("Mallory"));
    let (ana, bruno) = (w.id_of("ana").await, w.id_of("bruno").await);
    assert!(w
        .events_in(&space)
        .await
        .iter()
        .all(|e| e.env.author() == ana || e.env.author() == bruno));
}

/// A photo background set in a shared chat reaches the other person through the relay's
/// blob store, and the relay only ever holds ciphertext.
#[tokio::test]
async fn photo_background_travels_encrypted() {
    let w = World::new("photo").await;
    w.init("ana", "Ana");
    w.init("bruno", "Bruno");
    w.zoen("ana", &["dm", "@bruno", "olha o fundo novo"]);

    let mut photo =
        b"\xFF\xD8\xFF\xE0 not really a jpeg but unmistakable: ZOEN-PLAINTEXT-MARKER ".to_vec();
    photo.extend((0..20_000u32).map(|i| (i % 251) as u8));
    let src = w.dir.join("praia.jpg");
    std::fs::write(&src, &photo).unwrap();
    w.zoen(
        "ana",
        &["background", "@bruno", "--photo", src.to_str().unwrap()],
    );

    let out = w.dir.join("bruno-got.jpg");
    w.zoen("bruno", &["photo", "@ana", "--out", out.to_str().unwrap()]);
    assert_eq!(
        std::fs::read(&out).unwrap(),
        photo,
        "Bruno has exactly Ana's photo"
    );

    let mut stored = vec![];
    let mut stack = vec![w.dir.join("blobs")];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            if e.path().is_dir() {
                stack.push(e.path())
            } else {
                stored.push(std::fs::read(e.path()).unwrap())
            }
        }
    }
    assert_eq!(stored.len(), 1, "one encrypted copy on the relay");
    let blob = &stored[0];
    assert!(
        !blob.windows(21).any(|x| x == b"ZOEN-PLAINTEXT-MARKER"),
        "the relay never sees the plaintext"
    );
    assert_ne!(blob.len(), photo.len());
}
