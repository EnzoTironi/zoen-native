use super::*;

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
        .find(|ev| ev.env.sealed_kind() == Some(protocol::SealedKind::Application))
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
