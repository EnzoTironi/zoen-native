//! Complete media search scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn background_is_a_typed_event_with_verified_media() {
    let e = seeded();
    let s = find_space(&e, "Paraty com a Marina");
    let photo = vec![0xFFu8, 0xD8, 0xFF, 1, 2, 3, 4, 5];
    let m = e
        .put_media(photo.clone(), "image/jpeg".into(), 2048, 1536)
        .unwrap();
    assert_eq!(m.sha256.len(), 64);
    assert_eq!(e.media(m.sha256.clone()).unwrap(), Some(photo.clone()));
    assert!(
        e.put_media(vec![1], "text/html".into(), 1, 1).is_err(),
        "só imagens"
    );
    assert!(
        !e.lock()
            .import_media(&"00".repeat(32), &photo, "image/jpeg")
            .unwrap(),
        "hash errado é recusado"
    );
    let bg = BackgroundDto {
        style: "photo".into(),
        media: Some(m.clone()),
        zoom_pm: 1300,
        offset_x_pm: -120,
        offset_y_pm: 40,
        dim_pm: None,
        blur_pm: 0,
        appearance: "auto".into(),
    };
    let entry = e.set_background(s.id.clone(), bg.clone()).unwrap();
    assert!(matches!(entry.kind, EntryKind::Background { ref background } if background == &bg));
    assert_eq!(e.background(s.id.clone()).unwrap(), Some(bg.clone()));
    let mut missing = bg.clone();
    missing.media = Some(MediaRefDto {
        sha256: "ab".repeat(32),
        ..m.clone()
    });
    assert!(
        e.set_background(s.id.clone(), missing).is_err(),
        "foto precisa do anexo local"
    );
    assert!(e.verify_all().iter().all(|r| r.valid));
    assert!(e
        .spaces()
        .iter()
        .find(|x| x.id == s.id)
        .unwrap()
        .last_preview
        .contains("foto"));
}

pub(super) fn background_and_media_survive_reopen() {
    let dir = std::env::temp_dir().join(format!("background-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("t.sqlite").to_string_lossy().to_string();
    let _ = std::fs::remove_file(&path);
    let (sid, bg, sha) = {
        let e = Client::open(path.clone(), "en".into()).unwrap();
        e.seed_demo_if_empty().unwrap();
        let s = e.spaces()[0].clone();
        let m = e
            .put_media(vec![9, 8, 7, 6], "image/heic".into(), 10, 20)
            .unwrap();
        let bg = BackgroundDto {
            style: "photo".into(),
            media: Some(m.clone()),
            zoom_pm: 1000,
            offset_x_pm: 0,
            offset_y_pm: 0,
            dim_pm: Some(300),
            blur_pm: 200,
            appearance: "dark".into(),
        };
        e.set_background(s.id.clone(), bg.clone()).unwrap();
        (s.id, bg, m.sha256)
    };
    let e = Client::open(path.clone(), "en".into()).unwrap();
    assert_eq!(e.background(sid).unwrap(), Some(bg));
    assert_eq!(e.media(sha).unwrap(), Some(vec![9, 8, 7, 6]));
    assert!(e.verify_all().iter().all(|r| r.valid));
    let _ = std::fs::remove_file(&path);
}

pub(super) fn universal_search_is_prefix_accent_insensitive_and_fast() {
    let e = seeded();
    // "parat" (prefixo) acha o Espaço e mensagens; sem acento acha com acento.
    let r = e.universal_search("parat".into(), vec![], 20).unwrap();
    assert!(r
        .hits
        .iter()
        .any(|h| h.kind == "space" && h.title.contains("Paraty")));
    let r = e
        .universal_search("organizador".into(), vec!["agent".into()], 5)
        .unwrap();
    assert!(r.hits.iter().all(|h| h.kind == "agent"));
    assert!(r.hits.iter().any(|h| h.title == "Organizador"));
    // Mensagens trazem o hash do evento (= id da entrada na timeline) para pular até ela.
    let s = find_space(&e, "Paraty com a Marina");
    let sent = e
        .send_message(s.id.clone(), "Vamos fazer a ação da trilha amanhã".into())
        .unwrap();
    let r = e
        .universal_search("acao trilh".into(), vec!["message".into()], 10)
        .unwrap();
    let hit = r
        .hits
        .iter()
        .find(|h| h.ref_id == sent.id)
        .expect("acha a mensagem nova");
    assert!(hit.snippet.contains("[[ação]]"), "{}", hit.snippet);
    assert_eq!(hit.space_id.as_deref(), Some(s.id.as_str()));
    assert!(hit.persona.as_ref().unwrap().is_me);
    // Notas de voz entram pela transcrição.
    e.send_message(
        s.id.clone(),
        "⟦voice:abc:7000:00ff⟧\nlevo o fogareiro".into(),
    )
    .unwrap();
    let r = e.universal_search("fogar".into(), vec![], 10).unwrap();
    assert_eq!(r.hits.iter().filter(|h| h.kind == "message").count(), 1);
    assert!(r.hits[0].snippet.starts_with("levo"));
    // Nada encontrado.
    assert!(e
        .universal_search("xyzzyq".into(), vec![], 10)
        .unwrap()
        .hits
        .is_empty());
    assert!(e
        .universal_search("  ".into(), vec![], 10)
        .unwrap()
        .hits
        .is_empty());
    // Já indexado: consulta quente < 50 ms.
    let r = e.universal_search("hike".into(), vec![], 20).unwrap();
    assert!(r.took_us < 50_000, "{}µs", r.took_us);
}

pub(super) fn universal_search_timing_report() {
    let e = seeded();
    let cold = e.universal_search("hike".into(), vec![], 20).unwrap();
    let warm = e.universal_search("paraty".into(), vec![], 20).unwrap();
    let s = find_space(&e, "Paraty com a Marina");
    e.send_message(s.id, "nova mensagem".into()).unwrap();
    let after_write = e.universal_search("nova".into(), vec![], 20).unwrap();
    eprintln!(
        "search timing: cold(with index build) {}µs, warm {}µs, after write (rebuild) {}µs",
        cold.took_us, warm.took_us, after_write.took_us
    );
    assert_eq!(
        after_write
            .hits
            .iter()
            .filter(|h| h.kind == "message")
            .count(),
        1
    );
}

pub(super) fn account_and_chats_survive_a_relaunch_after_search() {
    let dir = std::env::temp_dir().join(format!("zoen-relaunch-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("core.sqlite").to_string_lossy().to_string();
    let vault: Arc<dyn SecretVault> = Arc::new(MemVault::default());
    {
        let e = Client::open(path.clone(), "en".into()).unwrap();
        e.erase_device(vault.clone()).unwrap();
        e.create_account(
            "Ana".into(),
            "ana_relaunch".into(),
            "http://127.0.0.1:9".into(),
            vault.clone(),
        )
        .unwrap();
        let space = e.create_group("Trip".into(), vec![]).unwrap();
        e.send_message(space.clone(), "first words".into()).unwrap();
        let _ = e.universal_search("first".into(), vec![], 5).unwrap();
        e.send_message(space, "after the search".into()).unwrap();
    }
    let e = Client::open(path, "en".into()).unwrap();
    assert!(e.account().is_some(), "the account is still here");
    let trip = e
        .spaces()
        .into_iter()
        .find(|s| s.title == "Trip")
        .expect("the chat is still here");
    assert!(
        trip.last_preview.contains("after the search"),
        "{}",
        trip.last_preview
    );
    std::fs::remove_dir_all(dir).ok();
}
