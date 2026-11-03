//! Complete seed locale scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn seed_builds_the_story_and_every_log_verifies() {
    let e = seeded();
    let spaces = e.spaces();
    assert_eq!(spaces.len(), 6, "Pessoal fica oculto");
    assert!(spaces
        .iter()
        .all(|s| s.kind != SpaceKindDto::Community || s.privacy == PrivacyDto::Closed));
    assert!(e.verify_all().iter().all(|r| r.valid));
    let paraty = find_space(&e, "Paraty com a Marina");
    assert_eq!(paraty.pending_requests, 3);
    assert_eq!(paraty.members.len(), 5);
    let me = e.me().unwrap();
    assert!(me.is_me);
    let org = agent(&e, "Organizador");
    assert_eq!(org.owner_name.as_deref(), Some("Marina"));
    assert!(!org.is_mine);
    assert_eq!(e.mentions().len(), 1);
    let plan = paraty_plan(&e);
    assert_eq!(plan.version, 2);
    assert_eq!(plan.plan.as_ref().unwrap().total_cents, 138_800);
}

pub(super) fn demo_receipts_reactions_and_threads_survive_reopen(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::temp_dir().join(format!("demo-{}", types::new_id("t")?));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("demo.sqlite").to_string_lossy().to_string();
    for first in [true, false] {
        let e = Client::open(path.clone(), "pt-BR".into()).unwrap();
        assert_eq!(e.seed_demo_if_empty().unwrap(), first);
        for title in ["Turma do Sábado", "Marina"] {
            let space = find_space(&e, title);
            let entries = e.timeline(space.id.clone()).unwrap();
            for delivery in [Delivery::Sent, Delivery::Delivered, Delivery::Read] {
                assert!(
                    entries
                        .iter()
                        .any(|entry| entry.author.is_me && entry.delivery == delivery),
                    "{title} has an outgoing {delivery:?} message"
                );
            }
            assert!(entries.iter().any(|entry| !entry.reactions.is_empty()));
            if title == "Turma do Sábado" {
                let root = entries
                    .iter()
                    .find(|entry| entry.thread_replies > 0)
                    .unwrap();
                let thread = e.thread(space.id.clone(), root.id.clone()).unwrap();
                assert_eq!(thread.len(), 2);
                assert_eq!(thread[1].in_thread.as_deref(), Some(root.id.as_str()));
            }
            assert!(
                !e.lock().net.synced.contains(&space.id),
                "demo edits stay local"
            );
        }
        assert!(e.verify_all().iter().all(|r| r.valid));
    }
    std::fs::remove_dir_all(dir).unwrap();

    Ok(())
}

pub(super) fn everything_persists_and_reverifies_after_reopen(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::temp_dir().join(format!("client-test-{}", types::new_id("t")?));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("client.sqlite").to_string_lossy().to_string();
    let (plan_id, version, events) = {
        let e = Client::open(path.clone(), "pt-BR".into()).unwrap();
        e.seed_demo_if_empty().unwrap();
        let plan = paraty_plan(&e);
        let line = plan.plan.as_ref().unwrap().sections[0].lines[1].id.clone();
        e.toggle_plan_line(
            plan.id.clone(),
            line.clone(),
            e.plan_page(plan.id.clone()).unwrap().content_hash,
        )
        .unwrap();
        let s = find_space(&e, "Zoen");
        e.send_message(s.id, "Oi Zoen".into()).unwrap();
        (
            plan.id.clone(),
            e.item(plan.id).unwrap().version,
            e.stats().events,
        )
    };
    let e = Client::open(path.clone(), "pt-BR".into()).unwrap();
    assert!(!e.seed_demo_if_empty().unwrap());
    assert_eq!(e.item(plan_id).unwrap().version, version);
    assert_eq!(e.stats().events, events);
    assert!(e.stats().all_logs_valid);
    let zoen_dm = find_space(&e, "Zoen");
    assert!(zoen_dm.last_preview.contains("Oi Zoen"));
    drop(e);

    // Alguém mexe no arquivo por fora: o núcleo detecta na reabertura.
    {
        let store = store::Store::open(&path).unwrap();
        let n = store.raw_execute("UPDATE events SET json = replace(json, 'Oi Zoen', 'Oi Zoen!!') WHERE json LIKE '%Oi Zoen%'").unwrap();
        assert_eq!(n, 1);
    }
    let e = Client::open(path, "pt-BR".into()).unwrap();
    let report = e.verify_log(zoen_dm.id.clone());
    assert!(!report.valid);
    assert!(report.error.unwrap().contains("conteúdo assinado"));
    assert!(!e.stats().all_logs_valid);
    let s = e.space(zoen_dm.id).unwrap();
    assert!(s.last_preview.contains("Log inválido"));
    assert!(!s.last_preview_is_markdown);
    let _ = std::fs::remove_dir_all(dir);

    Ok(())
}

pub(super) fn search_finds_items_and_messages_without_accents() {
    let e = seeded();
    let hits = e.search("onibus".into());
    assert!(hits
        .iter()
        .any(|h| h.item_id.is_some() && h.snippet.contains("Ônibus")));
    let hits = e.search("moqueca".into());
    assert!(!hits.is_empty());
    assert!(e.search("   ".into()).is_empty());
}

pub(super) fn log_events_are_signed_and_chained() {
    let e = seeded();
    let paraty = find_space(&e, "Paraty com a Marina");
    let events = e.log_events(paraty.id.clone()).unwrap();
    assert_eq!(events.len() as u64, paraty.event_count);
    assert_eq!(events.last().unwrap().seq, 0);
    for w in events.windows(2) {
        assert_eq!(w[1].hash, w[0].prev, "cada evento aponta para o anterior");
    }
    assert!(events.iter().all(|ev| ev.signature.len() == 128));
}

pub(super) fn english_is_the_default_language_for_demo_data_agents_and_money() {
    let e = Client::open(":memory:".into(), "en-US".into()).unwrap();
    assert_eq!(e.language(), "en");
    assert!(e.seed_demo_if_empty().unwrap());
    let paraty = find_space(&e, "Paraty with Marina");
    assert!(e.spaces().iter().any(|s| s.title == "Saturday Crew"));
    let plan = e
        .items()
        .into_iter()
        .find(|i| i.title == "Weekend in Paraty")
        .unwrap();
    assert!(e
        .requests()
        .iter()
        .any(|r| r.title == "Book the Casa Azul Inn"));
    assert_eq!(format_money(134_800, "en-US".into()), "R$ 1,348");
    assert_eq!(format_money(1_240, "en".into()), "R$ 12.40");
    assert_eq!(format_money(134_800, "pt-BR".into()), "R$ 1.348");

    // Agent output and mini-apps speak English too.
    let out = e
        .agent_create_app(
            paraty.id.clone(),
            zoen(&e).id,
            "adopt_pet".into(),
            "{}".into(),
            "test".into(),
            "what if we adopted a donkey?".into(),
        )
        .unwrap();
    let pet = out.item.unwrap();
    assert_eq!(pet.title, "Donkey");
    let fed = e
        .app_call_tool(pet.id.clone(), "pet_feed".into(), "{}".into(), false)
        .unwrap();
    assert!(fed.item.unwrap().versions[0]
        .note
        .contains("fed Donkey a carrot"));
    let app = e.item(pet.id.clone()).unwrap().app.unwrap();
    assert!(app.headline.starts_with("Donkey · "), "{}", app.headline);
    let edit = e
        .edit_plan_line(
            plan.id.clone(),
            plan_line_id(&plan),
            "Dinner by the water".into(),
            30_000,
            e.plan_page(plan.id.clone()).unwrap().content_hash,
        )
        .unwrap();
    let reaction = edit.reaction.unwrap();
    assert!(
        reaction.contains("R$")
            && (reaction.starts_with("Recalculated:") || reaction.starts_with("Heads up:")),
        "{reaction}"
    );
}

pub(super) fn switching_the_device_language_restarts_the_demo_in_that_language() {
    let dir = std::env::temp_dir().join(format!("locale-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("client.sqlite").to_string_lossy().to_string();
    let _ = std::fs::remove_file(&path);
    {
        let e = Client::open(path.clone(), "pt-BR".into()).unwrap();
        assert!(e.seed_demo_if_empty().unwrap());
        assert!(e.spaces().iter().any(|s| s.title == "Turma do Sábado"));
    }
    {
        let e = Client::open(path.clone(), "pt-BR".into()).unwrap();
        assert!(
            !e.seed_demo_if_empty().unwrap(),
            "same language: keep the data"
        );
    }
    let e = Client::open(path.clone(), "en-GB".into()).unwrap();
    assert!(e.seed_demo_if_empty().unwrap(), "new language: reseed");
    assert!(e.spaces().iter().any(|s| s.title == "Saturday Crew"));
    assert!(e.verify_all().iter().all(|r| r.valid));
    let _ = std::fs::remove_dir_all(&dir);
}

pub(super) fn widget_snapshots_are_strict_and_deep_link_to_their_item() {
    let e = seeded();
    let trip = e
        .items()
        .into_iter()
        .find(|i| {
            i.app
                .as_ref()
                .map(|a| a.app_id == "countdown")
                .unwrap_or(false)
        })
        .expect("seeded countdown");
    let app = trip.app.unwrap();
    let snap: serde_json::Value = serde_json::from_str(&app.snapshot_json).unwrap();
    assert_eq!(snap["template"], "countdown");
    assert_eq!(snap["deepLink"], format!("zoen://app/{}", trip.id));
    assert!(snap["targetMs"].as_i64().unwrap() > crate::engine::now_ms());
    // Every snapshot is content only: no remote URL anywhere in it.
    assert!(!app.snapshot_json.contains("http"));
    for spec in crate::apps::specs() {
        let start = spec
            .tools
            .iter()
            .find(|t| t.visibility.contains(&"model"))
            .unwrap();
        let args = match spec.id {
            "poll" => serde_json::json!({"question": "Q?", "options": ["a", "b"]}),
            "countdown" => serde_json::json!({"title": "T", "target_ms": 1}),
            _ => serde_json::json!({}),
        };
        let (_, st) = crate::apps::create(spec.id, &args, "Zoen", 0)
            .unwrap_or_else(|e| panic!("{} {}: {e}", spec.id, start.name));
        let s = crate::apps::snapshot(spec.id, &st, "it_x", 0)
            .expect("every built-in app has a widget");
        assert_eq!(s["deepLink"], "zoen://app/it_x");
        assert!(["stat", "progress", "countdown", "list", "caption"]
            .contains(&s["template"].as_str().unwrap()));
    }
}
