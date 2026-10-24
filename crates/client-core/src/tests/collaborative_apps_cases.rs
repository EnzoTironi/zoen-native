//! Complete collaborative apps scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn mcp_specs_follow_the_apps_extension() {
    let e = seeded();
    let specs = e.app_specs();
    assert_eq!(
        specs.len(),
        9 + usize::from(miniapps::deckcraft::registration::available())
            + usize::from(miniapps::binary::gridcraft_registration::available())
    );
    for s in &specs {
        assert!(s.resource_uri.starts_with("ui://"));
        assert!(
            s.tools
                .iter()
                .any(|t| t.visibility == vec!["model".to_string()]),
            "uma ferramenta para o agente criar"
        );
        assert!(s.has_view, "every built-in has an MCP Apps view: {}", s.id);
        let r = e.read_app_resource(s.resource_uri.clone()).unwrap();
        assert_eq!(r.mime_type, "text/html;profile=mcp-app");
        assert!(r.text.to_lowercase().starts_with("<!doctype html>"));
        assert!(
            s.tools
                .iter()
                .any(|t| t.visibility == vec!["model".to_string()]),
            "uma ferramenta para o agente criar"
        );
        let mut app_tools = s
            .tools
            .iter()
            .filter(|t| t.visibility.contains(&"app".to_string()))
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>();
        let mut expected_tools = match s.id.as_str() {
            "excalidraw" => vec!["app_document_read", "app_document_save"],
            "deckcraft" | "gridcraft" => vec![
                "app_binary_read",
                "app_binary_read_chunk",
                "app_binary_begin",
                "app_binary_append",
                "app_binary_commit",
                "app_binary_abort",
                "app_binary_export_source",
            ],
            "pet" => vec![
                "pet_feed",
                "pet_nap",
                "pet_wake",
                "pet_play",
                "pet_rename",
                "pet_dash_score",
                "pet_get_state",
                "pet_release",
            ],
            "maptap" => vec!["maptap_guess", "maptap_get_state"],
            "recipe" => vec![
                "recipe_servings",
                "recipe_check",
                "recipe_cook",
                "recipe_get_state",
            ],
            "poll" => vec![
                "poll_vote",
                "poll_add_option",
                "poll_get_state",
                "poll_close",
            ],
            "list" => vec!["list_add", "list_toggle", "list_get_state"],
            "countdown" => vec!["countdown_get_state"],
            "hike" => vec![
                "hike_vote",
                "hike_decide",
                "hike_set_itinerary",
                "hike_add_photos",
                "hike_get_state",
            ],
            "schedule" => vec![
                "schedule_configure",
                "schedule_book",
                "schedule_resolve",
                "schedule_cancel",
                "schedule_get_state",
            ],
            _ => panic!("unexpected built-in {}", s.id),
        };
        app_tools.sort_unstable();
        expected_tools.sort_unstable();
        assert_eq!(
            app_tools, expected_tools,
            "app-visible contract for {}",
            s.id
        );
    }
    assert!(e.read_app_resource("ui://outro/x".into()).is_err());
}

pub(super) fn poll_closing_writes_the_winner_into_the_plan() {
    let e = seeded();
    let paraty = find_space(&e, "Paraty com a Marina");
    let before = paraty_plan(&e).version;
    let out = e
        .agent_create_app(
            paraty.id.clone(),
            zoen(&e).id,
            "start_poll".into(),
            r#"{"question":"Pousada?","options":["Casa Azul","Pousada do Ouro"]}"#.into(),
            "teste".into(),
            "pousada A ou B?".into(),
        )
        .unwrap();
    let poll = out.item.unwrap();
    assert!(e
        .app_call_tool(poll.id.clone(), "poll_close".into(), "{}".into(), false)
        .unwrap()
        .result_json
        .contains("ninguém votou"));
    e.app_call_tool(
        poll.id.clone(),
        "poll_vote".into(),
        r#"{"option":"o2"}"#.into(),
        false,
    )
    .unwrap();
    e.app_call_tool(
        poll.id.clone(),
        "poll_add_option".into(),
        r#"{"text":"Camping"}"#.into(),
        false,
    )
    .unwrap();
    let closed = e
        .app_call_tool(poll.id.clone(), "poll_close".into(), "{}".into(), false)
        .unwrap();
    assert_eq!(closed.status, AppCallStatus::Done);
    let plan = paraty_plan(&e);
    assert_eq!(plan.version, before + 1);
    assert!(plan.plan.as_ref().unwrap().sections[0]
        .lines
        .iter()
        .any(|l| l.text.contains("Pousada do Ouro")));
    let poll_after_close = e.item(poll.id.clone()).unwrap();
    let log_after_close = e.verify_log(paraty.id.clone());
    let repeated = e
        .app_call_tool(poll.id.clone(), "poll_close".into(), "{}".into(), false)
        .unwrap();
    let result: serde_json::Value = serde_json::from_str(&repeated.result_json).unwrap();
    assert_eq!(result["isError"], true);
    assert!(repeated.result_json.contains("já fechou"));
    assert_eq!(
        paraty_plan(&e),
        plan,
        "repeated close must not add another plan line"
    );
    assert_eq!(e.item(poll.id).unwrap(), poll_after_close);
    let unchanged_log = e.verify_log(paraty.id.clone());
    assert!(unchanged_log.valid);
    assert_eq!(unchanged_log.events, log_after_close.events);
    assert_eq!(unchanged_log.head_hash, log_after_close.head_hash);
    assert!(e
        .agent_create_app(
            paraty.id,
            zoen(&e).id,
            "start_poll".into(),
            r#"{"question":"?","options":["só uma"]}"#.into(),
            "t".into(),
            "p".into()
        )
        .is_err());
}

pub(super) fn list_shared_updates_do_not_expose_simulated_external_delivery() {
    let e = seeded();
    let paraty = find_space(&e, "Paraty com a Marina");
    let list = e
        .agent_create_app(
            paraty.id.clone(),
            zoen(&e).id,
            "start_list".into(),
            r#"{"title":"O que levar","items":["Protetor","Carregador"]}"#.into(),
            "t".into(),
            "lista do que levar".into(),
        )
        .unwrap()
        .item
        .unwrap();
    let t = e
        .app_call_tool(
            list.id.clone(),
            "list_toggle".into(),
            r#"{"id":"l1"}"#.into(),
            false,
        )
        .unwrap();
    assert!(t.item.unwrap().app.unwrap().headline.starts_with("1 de 2"));
    let before = e.item(list.id.clone()).unwrap();
    let state: serde_json::Value =
        serde_json::from_str(&before.app.as_ref().unwrap().view_json).unwrap();
    assert!(state.get("sent").is_none());
    let log = e.verify_log(paraty.id.clone());
    for confirmed in [false, true] {
        assert!(e
            .app_call_tool(
                list.id.clone(),
                "list_send_whatsapp".into(),
                "{}".into(),
                confirmed,
            )
            .is_err());
        assert_eq!(e.item(list.id.clone()).unwrap(), before);
        let unchanged_log = e.verify_log(paraty.id.clone());
        assert!(unchanged_log.valid);
        assert_eq!(unchanged_log.events, log.events);
        assert_eq!(unchanged_log.head_hash, log.head_hash);
    }
}

pub(super) fn recipe_servings_and_checklist_are_shared() {
    let e = seeded();
    let paraty = find_space(&e, "Paraty com a Marina");
    let r = e
        .agent_create_app(
            paraty.id.clone(),
            zoen(&e).id,
            "start_recipe".into(),
            r#"{"servings":3}"#.into(),
            "teste".into(),
            "jantar vegetariano para 3".into(),
        )
        .unwrap()
        .item
        .unwrap();
    assert!(r.title.contains("Moqueca"));
    e.app_call_tool(
        r.id.clone(),
        "recipe_servings".into(),
        r#"{"servings":4}"#.into(),
        false,
    )
    .unwrap();
    e.demo_member_app_call(
        r.id.clone(),
        "marina".into(),
        "recipe_check".into(),
        r#"{"id":"i2"}"#.into(),
    )
    .unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&e.item(r.id.clone()).unwrap().app.unwrap().view_json).unwrap();
    assert_eq!(v["servings"], 4);
    assert_eq!(v["ingredients"][1]["done"], true);
    assert!(e
        .app_call_tool(
            r.id.clone(),
            "recipe_servings".into(),
            r#"{"servings":99}"#.into(),
            false
        )
        .is_ok());
    let v: serde_json::Value =
        serde_json::from_str(&e.item(r.id).unwrap().app.unwrap().view_json).unwrap();
    assert_eq!(v["servings"], 12, "limitado a 12");
}

pub(super) fn hike_votes_decide_plan_and_album() {
    let e = seeded();
    let crew = find_space(&e, "Turma do Sábado");
    let zoen = zoen(&e);
    e.agent_create_app(
        crew.id.clone(),
        zoen.id.clone(),
        "start_hike".into(),
        "{}".into(),
        "test".into(),
        "trilha".into(),
    )
    .unwrap();
    let hike = e
        .items()
        .into_iter()
        .find(|i| i.app.as_ref().map(|a| a.app_id == "hike").unwrap_or(false))
        .expect("hike");
    let r = e.read_app_resource("ui://builtin/hike".into()).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&r.manifest_json).unwrap();
    // The bundle is pinned: the host refuses it if this hash doesn't match.
    use sha2::Digest;
    assert_eq!(
        manifest["sha256"].as_str().unwrap(),
        format!("{:x}", sha2::Sha256::digest(r.text.as_bytes()))
    );
    assert_eq!(manifest["allowedDomains"][0], "tiles.openfreemap.org");
    for who in ["marina", "lucas"] {
        e.demo_member_app_call(
            hike.id.clone(),
            who.into(),
            "hike_vote".into(),
            r#"{"trail":"tomales"}"#.into(),
        )
        .unwrap();
    }
    e.app_call_tool(
        hike.id.clone(),
        "hike_vote".into(),
        r#"{"trail":"steep"}"#.into(),
        false,
    )
    .unwrap();
    // Changing your vote moves it.
    e.app_call_tool(
        hike.id.clone(),
        "hike_vote".into(),
        r#"{"trail":"tomales"}"#.into(),
        false,
    )
    .unwrap();
    e.app_call_tool(hike.id.clone(), "hike_decide".into(), "{}".into(), false)
        .unwrap();
    let out = e.app_call_tool(hike.id.clone(), "hike_set_itinerary".into(), r#"{"driver":"Lucas","pickups":[{"names":["Marina"],"place":"Hayes Valley"},{"names":["Ana","Enzo"],"place":"Mission"}]}"#.into(), false).unwrap();
    let v: serde_json::Value =
        serde_json::from_str(&out.item.unwrap().app.unwrap().view_json).unwrap();
    assert_eq!(v["decided"], "tomales");
    let trail = v["trails"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == "tomales")
        .unwrap()
        .clone();
    assert_eq!(trail["votes"].as_array().unwrap().len(), 3);
    let plan = v["itinerary"].as_array().unwrap();
    assert_eq!(plan.len(), 5);
    assert!(plan[1]["text"].as_str().unwrap().contains("Hayes Valley"));
    // Album accepts only small JPEG data URLs.
    assert!(e
        .app_call_tool(
            hike.id.clone(),
            "hike_add_photos".into(),
            r#"{"photos":["https://example.com/x.jpg"]}"#.into(),
            false
        )
        .unwrap()
        .result_json
        .contains("\"isError\":true"));
    e.app_call_tool(
        hike.id.clone(),
        "hike_add_photos".into(),
        r#"{"photos":["data:image/jpeg;base64,AAAA"]}"#.into(),
        false,
    )
    .unwrap();
}
