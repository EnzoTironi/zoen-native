use super::*;

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
    let mut watch = Watch::start(&w, "ana", "history-first-page");
    watch.ready("ana").await;
    let read = w.zoen("ana-notebook", &["read", "Casa", "--older"]);
    assert!(read.contains("(older: 3 more, still more)"), "{read}");
    for m in &before[3..] {
        assert!(read.contains(m.as_str()), "{m:?} missing:\n{read}");
    }
    assert!(!read.contains(&before[0]), "{read}");
    drop(watch);
    // With the phone away, a gentle state instead of a page.
    let offline = w.zoen("ana-notebook", &["read", "Casa", "--older"]);
    assert!(
        offline.contains("Abra o Zoen no celular") || offline.contains("Open Zoen on your phone"),
        "{offline}"
    );
    // The phone is back: the rest arrives, and then there is nothing older to ask for.
    let mut watch = Watch::start(&w, "ana", "history-last-page");
    watch.ready("ana").await;
    let read = w.zoen("ana-notebook", &["read", "Casa", "--older"]);
    assert!(read.contains("(older: 3 more)"), "{read}");
    for m in &before {
        assert!(read.contains(m.as_str()), "{m:?} missing:\n{read}");
    }
    drop(watch);
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
