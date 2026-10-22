//! Complete interactive apps scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn pet_is_shared_state_versioned_and_gated_by_grants() {
    let e = seeded();
    let paraty = find_space(&e, "Paraty com a Marina");
    let out = e
        .agent_create_app(
            paraty.id.clone(),
            zoen(&e).id,
            "adopt_pet".into(),
            r#"{"name":"Burrico"}"#.into(),
            "teste".into(),
            "e se a gente adotasse um burro?".into(),
        )
        .unwrap();
    assert_eq!(out.kind, DecisionKind::ActWithUndo);
    let pet = out.item.unwrap();
    let app = pet.app.clone().unwrap();
    assert_eq!(app.app_id, "pet");
    assert_eq!(
        app.trust,
        TrustLevelDto::Act,
        "Concessão do mini-app: agir só neste Item"
    );
    // O cartão entra na conversa.
    let tl = e.timeline(paraty.id.clone()).unwrap();
    assert!(tl.iter().any(|t| matches!(&t.kind, EntryKind::Message { card: Some(c), .. } if c.item_id == pet.id && c.app.is_some())));

    // Reversível: roda e vira versão, com quem fez o quê.
    let fed = e
        .app_call_tool(pet.id.clone(), "pet_feed".into(), "{}".into(), false)
        .unwrap();
    assert_eq!(fed.status, AppCallStatus::Done);
    let item = fed.item.unwrap();
    assert_eq!(item.version, 2);
    assert!(item.versions[0].note.contains("cenoura"));
    let result: serde_json::Value = serde_json::from_str(&fed.result_json).unwrap();
    assert_eq!(result["isError"], false);
    assert!(result["structuredContent"]["fullness"].as_f64().unwrap() > 72.0);
    assert!(item.app.unwrap().last_action.unwrap().contains("cenoura"));

    // Leitura não cria versão.
    let read = e
        .app_call_tool(pet.id.clone(), "pet_get_state".into(), "{}".into(), false)
        .unwrap();
    assert_eq!(read.item.unwrap().version, 2);

    // Ferramenta só do modelo: a interface não pode chamar.
    assert!(e
        .app_call_tool(pet.id.clone(), "adopt_pet".into(), "{}".into(), false)
        .is_err());

    // Irreversível: pede confirmação nativa; só roda confirmado.
    let ask = e
        .app_call_tool(pet.id.clone(), "pet_release".into(), "{}".into(), false)
        .unwrap();
    assert_eq!(ask.status, AppCallStatus::NeedsConfirmation);
    assert!(ask.confirm_title.is_some());
    assert_eq!(
        e.item(pet.id.clone()).unwrap().version,
        2,
        "nada mudou sem confirmar"
    );
    let done = e
        .app_call_tool(pet.id.clone(), "pet_release".into(), "{}".into(), true)
        .unwrap();
    assert_eq!(done.status, AppCallStatus::Done);
    assert!(e
        .app_call_tool(pet.id.clone(), "pet_feed".into(), "{}".into(), false)
        .unwrap()
        .result_json
        .contains("\"isError\":true"));
    assert!(e.verify_all().iter().all(|r| r.valid));
}

pub(super) fn pet_follows_the_group_story_rename_sleep_dash_and_members() {
    let e = seeded();
    let paraty = find_space(&e, "Paraty com a Marina");
    let pet = e
        .agent_create_app(
            paraty.id.clone(),
            zoen(&e).id,
            "adopt_pet".into(),
            "{}".into(),
            "teste".into(),
            "e se a gente adotasse um jumento?".into(),
        )
        .unwrap()
        .item
        .unwrap();
    assert_eq!(pet.title, "Jumento");
    let app = pet.app.unwrap();
    assert_eq!(
        app.metrics
            .iter()
            .map(|m| m.label.as_str())
            .collect::<Vec<_>>(),
        vec!["Comida", "Humor", "Descanso"]
    );
    assert!(app.headline.contains("Feliz aqui"));

    // Outro membro age: a versão é assinada por ele (simulação de sincronização).
    let m = e
        .demo_member_app_call(
            pet.id.clone(),
            "marina".into(),
            "pet_feed".into(),
            "{}".into(),
        )
        .unwrap();
    assert_eq!(m.status, AppCallStatus::Done);
    let it = e.item(pet.id.clone()).unwrap();
    assert_eq!(
        it.versions[0].author.handle, "marina",
        "a versão mais nova é dela"
    );
    assert!(it.app.unwrap().last_action.unwrap().starts_with("Marina"));

    // Renomear muda o título do cartão para todos e o Zoen comenta.
    e.app_call_tool(
        pet.id.clone(),
        "pet_rename".into(),
        r#"{"name":"paçoca"}"#.into(),
        false,
    )
    .unwrap();
    let it = e.item(pet.id.clone()).unwrap();
    assert_eq!(it.title, "Paçoca");
    let tl = e.timeline(paraty.id.clone()).unwrap();
    assert!(tl.iter().any(|t| t.author.handle == "zoen"
        && matches!(&t.kind, EntryKind::Message { text, .. } if text.contains("Paçoca"))));

    // Dormindo: comida bloqueada, Acordar libera.
    e.app_call_tool(pet.id.clone(), "pet_nap".into(), "{}".into(), false)
        .unwrap();
    assert!(e
        .item(pet.id.clone())
        .unwrap()
        .app
        .unwrap()
        .headline
        .contains("Dormindo"));
    assert!(e
        .app_call_tool(pet.id.clone(), "pet_feed".into(), "{}".into(), false)
        .unwrap()
        .result_json
        .contains("Acorde primeiro"));
    e.app_call_tool(pet.id.clone(), "pet_wake".into(), "{}".into(), false)
        .unwrap();

    // Corrida do Jumento: placar do grupo, recorde comentado pelo agente.
    e.app_call_tool(
        pet.id.clone(),
        "pet_dash_score".into(),
        r#"{"meters":120,"carrots":6}"#.into(),
        false,
    )
    .unwrap();
    e.demo_member_app_call(
        pet.id.clone(),
        "marina".into(),
        "pet_dash_score".into(),
        r#"{"meters":80,"carrots":3}"#.into(),
    )
    .unwrap();
    e.app_call_tool(
        pet.id.clone(),
        "pet_dash_score".into(),
        r#"{"meters":90,"carrots":2}"#.into(),
        false,
    )
    .unwrap();
    let it = e.item(pet.id.clone()).unwrap();
    let view: serde_json::Value = serde_json::from_str(&it.app.unwrap().view_json).unwrap();
    assert_eq!(
        view["dash"]["best"]["Enzo"]["meters"], 120,
        "vale a melhor corrida"
    );
    assert_eq!(view["dash"]["best"]["Marina"]["meters"], 80);
    assert_eq!(view["dash"]["runs"], 3);
    let tl = e.timeline(paraty.id.clone()).unwrap();
    assert_eq!(tl.iter().filter(|t| matches!(&t.kind, EntryKind::Message { text, .. } if text.contains("Novo recorde"))).count(), 1);
    assert!(e.verify_all().iter().all(|r| r.valid));
}

pub(super) fn maptap_is_turn_based_with_the_same_places_for_everyone() {
    let e = seeded();
    let paraty = find_space(&e, "Paraty com a Marina");
    let game = e
        .agent_create_app(
            paraty.id.clone(),
            zoen(&e).id,
            "start_maptap".into(),
            "{}".into(),
            "teste".into(),
            "dá um jogo de geografia".into(),
        )
        .unwrap()
        .item
        .unwrap();
    let view: serde_json::Value = serde_json::from_str(&game.app.unwrap().view_json).unwrap();
    let places = view["places"].as_array().unwrap().clone();
    assert_eq!(places.len(), 5);
    let (lat, lon) = (
        places[0]["lat"].as_f64().unwrap(),
        places[0]["lon"].as_f64().unwrap(),
    );
    let hit = e
        .app_call_tool(
            game.id.clone(),
            "maptap_guess".into(),
            format!(r#"{{"round":0,"lat":{},"lon":{}}}"#, lat + 0.2, lon),
            false,
        )
        .unwrap();
    let r: serde_json::Value = serde_json::from_str(&hit.result_json).unwrap();
    let mine = &r["structuredContent"]["guesses"]["Enzo"]["0"];
    assert!(mine["km"].as_f64().unwrap() < 30.0);
    assert!(mine["points"].as_i64().unwrap() > 4900);
    // Mesma rodada de novo: não.
    assert!(e
        .app_call_tool(
            game.id.clone(),
            "maptap_guess".into(),
            r#"{"round":0,"lat":0,"lon":0}"#.into(),
            false
        )
        .unwrap()
        .result_json
        .contains("já cravou"));
    // Marina joga os mesmos lugares; o placar ordena.
    e.demo_member_app_call(
        game.id.clone(),
        "marina".into(),
        "maptap_guess".into(),
        r#"{"round":0,"lat":0,"lon":0}"#.into(),
    )
    .unwrap();
    assert!(e
        .item(game.id.clone())
        .unwrap()
        .app
        .unwrap()
        .headline
        .starts_with("Enzo lidera"));
    assert!(
        (crate::apps::haversine_km(-23.55, -46.63, -22.91, -43.17) - 357.0).abs() < 10.0,
        "SP–Rio ≈ 357 km"
    );
}
