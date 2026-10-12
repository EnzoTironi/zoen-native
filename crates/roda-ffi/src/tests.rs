//! Testes de ponta a ponta do motor: seed → ações → reabrir → reverificar.

use super::*;

fn seeded() -> Arc<RodaEngine> {
    let e = RodaEngine::open(":memory:".into(), "pt-BR".into()).unwrap();
    assert!(e.seed_demo_if_empty().unwrap());
    assert!(!e.seed_demo_if_empty().unwrap(), "seed é idempotente");
    e
}

fn find_space(e: &RodaEngine, title: &str) -> SpaceSummary {
    e.spaces()
        .into_iter()
        .find(|s| s.title == title)
        .unwrap_or_else(|| panic!("sem Espaço {title}"))
}

fn agent(e: &RodaEngine, name: &str) -> Persona {
    e.personas().into_iter().find(|p| p.name == name).unwrap()
}

fn paraty_plan(e: &RodaEngine) -> ItemDetail {
    let items = e.items();
    items
        .into_iter()
        .find(|i| i.title == "Fim de semana em Paraty")
        .unwrap()
}

#[test]
fn seed_builds_the_story_and_every_log_verifies() {
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

#[test]
fn edit_then_undo_restores_content_and_keeps_history() {
    let e = seeded();
    let plan = paraty_plan(&e);
    let p = plan.plan.clone().unwrap();
    let line = &p.sections[2].lines[0]; // escuna
    let out = e
        .edit_plan_line(
            plan.id.clone(),
            line.id.clone(),
            "Escuna + mergulho".into(),
            30_000,
        )
        .unwrap();
    assert_eq!(out.item.version, 3);
    assert_eq!(
        out.item.plan.as_ref().unwrap().total_cents,
        138_800 - 18_000 + 30_000
    );
    let reaction = out.reaction.expect("o Financeiro comenta o total");
    assert!(reaction.contains("passou"), "{reaction}");

    let undone = e.undo(out.undo.clone()).unwrap();
    assert_eq!(undone.version, 4, "desfazer é uma versão nova");
    assert_eq!(
        undone.plan.as_ref().unwrap(),
        &p,
        "conteúdo idêntico ao de antes"
    );
    assert!(undone.versions[0].is_undo);
    assert_eq!(undone.versions.len(), 4);

    // Restaurar a v1 (antes da troca do jantar).
    let v1 = e.restore_version(plan.id.clone(), 1).unwrap();
    assert_eq!(v1.plan.unwrap().total_cents, 134_800);
    assert!(e.verify_all().iter().all(|r| r.valid));
}

#[test]
fn approval_is_bound_to_the_exact_content() {
    let e = seeded();
    let plan = paraty_plan(&e);
    let reqs = e.requests();
    let pousada = reqs
        .iter()
        .find(|r| r.title.contains("Pousada"))
        .unwrap()
        .clone();
    assert_eq!(pousada.status, RequestStatus::Pending);
    assert!(
        pousada.reason.contains("Linha vermelha"),
        "{}",
        pousada.reason
    );

    // Edito a linha da pousada → o pedido fica desatualizado e não pode ser aprovado.
    let line_id = pousada.line_id.clone().unwrap();
    let out = e
        .edit_plan_line(
            plan.id.clone(),
            line_id.clone(),
            "Pousada Casa Azul · 2 noites".into(),
            60_000,
        )
        .unwrap();
    let stale = e
        .requests()
        .into_iter()
        .find(|r| r.id == pousada.id)
        .unwrap();
    assert_eq!(stale.status, RequestStatus::Stale);
    assert!(matches!(
        e.approve_request(pousada.id.clone()),
        Err(CoreError::Stale { .. })
    ));

    // Desfazer volta o conteúdo exato → o pedido volta a valer.
    e.undo(out.undo).unwrap();
    let back = e
        .requests()
        .into_iter()
        .find(|r| r.id == pousada.id)
        .unwrap();
    assert_eq!(back.status, RequestStatus::Pending);

    let approved = e.approve_request(pousada.id.clone()).unwrap();
    assert_eq!(approved.request.status, RequestStatus::Approved);
    assert!(approved.message.contains("Aguardando a execução"));
    let after = e.item(plan.id.clone()).unwrap();
    let l = after
        .plan
        .unwrap()
        .sections
        .iter()
        .flat_map(|s| s.lines.clone())
        .find(|l| l.id == line_id)
        .unwrap();
    assert!(
        !l.done,
        "approval cannot claim a payment or external send was executed"
    );
    assert!(
        e.approve_request(pousada.id).is_ok(),
        "a repeated decision returns the stored receipt without executing again"
    );
}

#[test]
fn approve_all_and_deny() {
    let e = seeded();
    let fin = agent(&e, "Financeiro");
    let cobranca = e
        .requests()
        .into_iter()
        .find(|r| r.title.contains("cobrança"))
        .unwrap();
    e.deny_request(cobranca.id.clone()).unwrap();
    assert_eq!(e.approve_all(fin.id.clone()).unwrap(), 2);
    assert!(e
        .requests()
        .iter()
        .all(|r| r.status != RequestStatus::Pending));
    assert_eq!(find_space(&e, "Paraty com a Marina").pending_requests, 0);
    assert_eq!(e.agent_profile(fin.id).approvals_streak, 2);
}

#[test]
fn trust_levels_change_what_agents_can_do() {
    let e = seeded();
    let paraty = find_space(&e, "Paraty com a Marina");
    let zoen = agent(&e, "Zoen");
    let plan = PlanDto {
        title: "Jantar de aniversário".into(),
        summary: "Sábado".into(),
        budget_cents: Some(60_000),
        total_cents: 0,
        sections: vec![PlanSectionDto {
            title: "Comida".into(),
            lines: vec![PlanLineDto {
                id: String::new(),
                text: "Restaurante".into(),
                cost_cents: 40_000,
                done: false,
            }],
        }],
    };
    let acted = e
        .agent_create_plan(
            paraty.id.clone(),
            zoen.id.clone(),
            "jantar".into(),
            plan.clone(),
            "Teste".into(),
            0,
        )
        .unwrap();
    assert_eq!(acted.kind, DecisionKind::ActWithUndo);
    assert!(acted.item.is_some());

    e.set_trust(zoen.id.clone(), paraty.id.clone(), TrustLevelDto::Suggest)
        .unwrap();
    let proposed = e
        .agent_create_plan(
            paraty.id.clone(),
            zoen.id.clone(),
            "jantar".into(),
            plan.clone(),
            "Teste".into(),
            0,
        )
        .unwrap();
    assert_eq!(proposed.kind, DecisionKind::Request);

    e.set_trust(zoen.id.clone(), paraty.id.clone(), TrustLevelDto::Listen)
        .unwrap();
    let blocked = e
        .agent_create_plan(
            paraty.id.clone(),
            zoen.id.clone(),
            "jantar".into(),
            plan,
            "Teste".into(),
            0,
        )
        .unwrap();
    assert_eq!(blocked.kind, DecisionKind::Block);

    let preview = e.preview_decisions(zoen.id.clone(), paraty.id.clone());
    assert_eq!(preview[0].kind, DecisionKind::Act); // responder sempre pode
    assert_eq!(preview[1].kind, DecisionKind::Block); // editar não, no Ouvir
    assert!(
        preview
            .iter()
            .find(|p| p.action == "Pagamento grande")
            .unwrap()
            .red_line
    );

    // Não mudo a confiança do agente da Marina.
    let org = agent(&e, "Organizador");
    assert!(matches!(
        e.set_trust(org.id, paraty.id, TrustLevelDto::Act),
        Err(CoreError::Forbidden { .. })
    ));
}

#[test]
fn budget_stops_at_ceiling_and_can_be_raised() {
    let e = seeded();
    let fin = agent(&e, "Financeiro");
    let paraty = find_space(&e, "Paraty com a Marina");
    let p = e.agent_profile(fin.id.clone());
    assert_eq!(p.budget_limit_cents, Some(500));
    assert_eq!(p.budget_spent_cents, Some(420));
    assert!(p.near_limit, "alerta aos 80%");
    assert!(matches!(
        e.agent_say(paraty.id.clone(), fin.id.clone(), "oi".into(), 200),
        Err(CoreError::Forbidden { .. })
    ));
    let raised = e.raise_budget(fin.id.clone(), 2_000).unwrap();
    assert_eq!(raised.budget_limit_cents, Some(2_500));
    assert!(e
        .agent_say(paraty.id, fin.id.clone(), "oi".into(), 200)
        .is_ok());
    assert_eq!(e.agent_profile(fin.id).budget_spent_cents, Some(620));
    // O orçamento do agente da Marina não vive aqui.
    assert_eq!(
        e.agent_profile(agent(&e, "Organizador").id)
            .budget_limit_cents,
        None
    );
}

#[test]
fn everything_persists_and_reverifies_after_reopen() {
    let dir = std::env::temp_dir().join(format!("roda-test-{}", roda_types::new_id("t")));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("roda.sqlite").to_string_lossy().to_string();
    let (plan_id, version, events) = {
        let e = RodaEngine::open(path.clone(), "pt-BR".into()).unwrap();
        e.seed_demo_if_empty().unwrap();
        let plan = paraty_plan(&e);
        let line = plan.plan.as_ref().unwrap().sections[0].lines[1].id.clone();
        e.toggle_plan_line(plan.id.clone(), line).unwrap();
        let s = find_space(&e, "Zoen");
        e.send_message(s.id, "Oi Zoen".into()).unwrap();
        (
            plan.id.clone(),
            e.item(plan.id).unwrap().version,
            e.stats().events,
        )
    };
    let e = RodaEngine::open(path.clone(), "pt-BR".into()).unwrap();
    assert!(!e.seed_demo_if_empty().unwrap());
    assert_eq!(e.item(plan_id).unwrap().version, version);
    assert_eq!(e.stats().events, events);
    assert!(e.stats().all_logs_valid);
    let zoen_dm = find_space(&e, "Zoen");
    assert!(zoen_dm.last_preview.contains("Oi Zoen"));
    drop(e);

    // Alguém mexe no arquivo por fora: o núcleo detecta na reabertura.
    {
        let store = roda_store::Store::open(&path).unwrap();
        let n = store.raw_execute("UPDATE events SET json = replace(json, 'Oi Zoen', 'Oi Zoen!!') WHERE json LIKE '%Oi Zoen%'").unwrap();
        assert_eq!(n, 1);
    }
    let e = RodaEngine::open(path, "pt-BR".into()).unwrap();
    let report = e.verify_log(zoen_dm.id.clone());
    assert!(!report.valid);
    assert!(report.error.unwrap().contains("conteúdo assinado"));
    assert!(!e.stats().all_logs_valid);
    let s = e.space(zoen_dm.id).unwrap();
    assert!(s.last_preview.contains("Log inválido"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn search_finds_items_and_messages_without_accents() {
    let e = seeded();
    let hits = e.search("onibus".into());
    assert!(hits
        .iter()
        .any(|h| h.item_id.is_some() && h.snippet.contains("Ônibus")));
    let hits = e.search("moqueca".into());
    assert!(!hits.is_empty());
    assert!(e.search("   ".into()).is_empty());
}

#[test]
fn log_events_are_signed_and_chained() {
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

#[test]
fn members_only_and_invites() {
    let e = seeded();
    let com = find_space(&e, "Viajantes do Litoral");
    let zoen = agent(&e, "Zoen");
    assert!(
        e.agent_say(com.id.clone(), zoen.id, "oi".into(), 0)
            .is_err(),
        "Zoen não é membro da comunidade"
    );
    let link = e.invite_link(com.id).unwrap();
    assert!(link.starts_with("https://roda.app/c/"));
}

#[test]
fn agent_activity_reads_the_log_with_costs() {
    let e = seeded();
    let fin = agent(&e, "Financeiro");
    let acts = e.agent_activity(fin.id.clone()).unwrap();
    assert!(!acts.is_empty());
    assert!(
        acts.windows(2).all(|w| w[0].at_ms >= w[1].at_ms),
        "mais recentes primeiro"
    );
    assert!(
        acts.iter().any(|a| a.cost_cents == Some(420)),
        "custo vem do UsageRecorded assinado"
    );
    assert!(acts.iter().filter(|a| a.label == "Pediu aprovação").count() >= 3);
    assert!(e.agent_activity("nao-existe".into()).is_err());
}

// ── MCP Apps ──

fn zoen(e: &RodaEngine) -> Persona {
    agent(e, "Zoen")
}

#[test]
fn mcp_specs_follow_the_apps_extension() {
    let e = seeded();
    let specs = e.app_specs();
    assert_eq!(specs.len(), 7);
    for s in &specs {
        assert!(s.resource_uri.starts_with("ui://"));
        assert!(
            s.tools
                .iter()
                .any(|t| t.visibility == vec!["model".to_string()]),
            "uma ferramenta para o agente criar"
        );
        if !s.has_view {
            // Nativos (MapTap, receita): sem View MCP, e o host não inventa uma.
            assert!(e.read_app_resource(s.resource_uri.clone()).is_err());
            continue;
        }
        let r = e.read_app_resource(s.resource_uri.clone()).unwrap();
        assert_eq!(r.mime_type, "text/html;profile=mcp-app");
        assert!(r.text.to_lowercase().starts_with("<!doctype html>"));
        assert!(
            s.tools
                .iter()
                .any(|t| t.visibility == vec!["model".to_string()]),
            "uma ferramenta para o agente criar"
        );
        assert!(
            s.tools
                .iter()
                .filter(|t| t.visibility.contains(&"app".to_string()))
                .count()
                >= 3
        );
    }
    assert!(e.read_app_resource("ui://outro/x".into()).is_err());
}

#[test]
fn pet_is_shared_state_versioned_and_gated_by_grants() {
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

#[test]
fn poll_closing_writes_the_winner_into_the_plan() {
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
    assert!(plan.plan.unwrap().sections[0]
        .lines
        .iter()
        .any(|l| l.text.contains("Pousada do Ouro")));
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

#[test]
fn list_sending_outside_needs_confirmation() {
    let e = seeded();
    let paraty = find_space(&e, "Paraty com a Marina");
    let list = e
        .agent_create_app(
            paraty.id,
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
    let ask = e
        .app_call_tool(
            list.id.clone(),
            "list_send_whatsapp".into(),
            "{}".into(),
            false,
        )
        .unwrap();
    assert_eq!(ask.status, AppCallStatus::NeedsConfirmation);
    let sent = e
        .app_call_tool(list.id, "list_send_whatsapp".into(), "{}".into(), true)
        .unwrap();
    assert!(sent.message.contains("Simulação"));
}

#[test]
fn pet_follows_the_group_story_rename_sleep_dash_and_members() {
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

#[test]
fn maptap_is_turn_based_with_the_same_places_for_everyone() {
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

#[test]
fn recipe_servings_and_checklist_are_shared() {
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

#[test]
fn english_is_the_default_language_for_demo_data_agents_and_money() {
    let e = RodaEngine::open(":memory:".into(), "en-US".into()).unwrap();
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
    assert_eq!(format_money(134_800, "en-US".into()), "$1,348");
    assert_eq!(format_money(1_240, "en".into()), "$12.40");
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
        )
        .unwrap();
    let reaction = edit.reaction.unwrap();
    assert!(
        reaction.contains('$') && !reaction.contains("R$"),
        "{reaction}"
    );
}

#[test]
fn switching_the_device_language_restarts_the_demo_in_that_language() {
    let dir = std::env::temp_dir().join(format!("roda-lang-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("roda.sqlite").to_string_lossy().to_string();
    let _ = std::fs::remove_file(&path);
    {
        let e = RodaEngine::open(path.clone(), "pt-BR".into()).unwrap();
        assert!(e.seed_demo_if_empty().unwrap());
        assert!(e.spaces().iter().any(|s| s.title == "Turma do Sábado"));
    }
    {
        let e = RodaEngine::open(path.clone(), "pt-BR".into()).unwrap();
        assert!(
            !e.seed_demo_if_empty().unwrap(),
            "same language: keep the data"
        );
    }
    let e = RodaEngine::open(path.clone(), "en-GB".into()).unwrap();
    assert!(e.seed_demo_if_empty().unwrap(), "new language: reseed");
    assert!(e.spaces().iter().any(|s| s.title == "Saturday Crew"));
    assert!(e.verify_all().iter().all(|r| r.valid));
    let _ = std::fs::remove_dir_all(&dir);
}

fn plan_line_id(plan: &ItemDetail) -> String {
    match &plan.plan {
        Some(p) => p.sections[0].lines[0].id.clone(),
        None => panic!("not a plan"),
    }
}

#[test]
fn widget_snapshots_are_strict_and_deep_link_to_their_item() {
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

#[test]
fn hike_votes_decide_plan_and_album() {
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
    let r = e.read_app_resource("ui://roda/hike".into()).unwrap();
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

#[test]
fn device_grants_are_per_mini_app_logged_and_revocable() {
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
        .unwrap();
    assert!(!e.app_device_allowed(trip.id.clone(), "location.approximate".into()));
    let g = e
        .grant_app_device(
            trip.id.clone(),
            "location.approximate".into(),
            "Show how far it is".into(),
            true,
        )
        .unwrap();
    assert!(e.app_device_allowed(trip.id.clone(), "location.approximate".into()));
    assert!(
        !e.app_device_allowed(trip.id.clone(), "photos.pick".into()),
        "a grant covers one capability"
    );
    assert_eq!(e.app_device_grants().len(), 1);
    // "Allow once" is a grant that expires: logged, but it doesn't stick.
    e.grant_app_device(trip.id.clone(), "photos.pick".into(), "Album".into(), false)
        .unwrap();
    assert!(e
        .app_device_grants()
        .iter()
        .any(|d| d.capability == "photos.pick" && !d.always));
    assert!(e
        .grant_app_device(trip.id.clone(), "bad cap!".into(), "x".into(), true)
        .is_err());
    e.revoke_app_device(g).unwrap();
    assert!(!e.app_device_allowed(trip.id.clone(), "location.approximate".into()));
}

#[test]
fn background_is_a_typed_event_with_verified_media() {
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

#[test]
fn background_and_media_survive_reopen() {
    let dir = std::env::temp_dir().join(format!("roda-bg-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("t.sqlite").to_string_lossy().to_string();
    let _ = std::fs::remove_file(&path);
    let (sid, bg, sha) = {
        let e = RodaEngine::open(path.clone(), "en".into()).unwrap();
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
    let e = RodaEngine::open(path.clone(), "en".into()).unwrap();
    assert_eq!(e.background(sid).unwrap(), Some(bg));
    assert_eq!(e.media(sha).unwrap(), Some(vec![9, 8, 7, 6]));
    assert!(e.verify_all().iter().all(|r| r.valid));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn universal_search_is_prefix_accent_insensitive_and_fast() {
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

#[test]
fn universal_search_timing_report() {
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

#[derive(Default)]
struct MemVault(std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>);

impl SecretVault for MemVault {
    fn load(&self, key: String) -> Option<Vec<u8>> {
        self.0.lock().unwrap().get(&key).cloned()
    }
    fn save(&self, key: String, value: Vec<u8>) -> bool {
        self.0.lock().unwrap().insert(key, value);
        true
    }
    fn delete(&self, key: String) {
        self.0.lock().unwrap().remove(&key);
    }
}

/// The app's first launch in the simulator journey: erase, sign up, chat, search; then the
/// process dies. Everything must be on disk when it opens again.
#[test]
fn account_and_chats_survive_a_relaunch_after_search() {
    let dir = std::env::temp_dir().join(format!("zoen-relaunch-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("core.sqlite").to_string_lossy().to_string();
    let vault: Arc<dyn SecretVault> = Arc::new(MemVault::default());
    {
        let e = RodaEngine::open(path.clone(), "en".into()).unwrap();
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
    let e = RodaEngine::open(path, "en".into()).unwrap();
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

#[test]
fn unconfirmed_genesis_holds_its_descendants_across_retry_and_relaunch() {
    use roda_log::chain_hash;
    use roda_proto::Sequenced;
    use roda_types::GENESIS_PREV;

    let dir = std::env::temp_dir().join(format!("zoen-genesis-{}", roda_types::new_id("t")));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("core.sqlite").to_string_lossy().to_string();
    let vault: Arc<dyn SecretVault> = Arc::new(MemVault::default());
    let (space, genesis, message_id) = {
        let e = RodaEngine::open(path.clone(), "en".into()).unwrap();
        e.create_account(
            "Ana".into(),
            "ana".into(),
            "http://127.0.0.1:9".into(),
            vault.clone(),
        )
        .unwrap();
        let space = e
            .create_group_with("Offline".into(), vec![], PrivacyDto::Closed)
            .unwrap();
        e.send_message(space.clone(), "keep this queued".into())
            .unwrap();
        let engine = e.lock();
        let queued = engine.store.outbox().unwrap();
        let genesis = queued
            .iter()
            .find(|p| p.event.space == space && p.event.seen.is_none())
            .unwrap();
        let message_id = queued
            .iter()
            .find(|p| p.event.space == space && p.event.seen.is_some())
            .unwrap()
            .event
            .client_id
            .clone();
        let genesis = roda_proto::Envelope::plain(&genesis.event);
        let outgoing = engine.outbox_envelopes();
        assert!(outgoing
            .iter()
            .any(|e| e.client_id() == genesis.client_id()));
        assert!(!outgoing.iter().any(|e| e.client_id() == message_id));
        drop(engine);
        e.lock().reject(
            genesis.client_id(),
            "owner temporarily unavailable",
            false,
            None,
        );
        assert!(!e
            .lock()
            .outbox_envelopes()
            .iter()
            .any(|e| e.client_id() == message_id));
        (space, genesis, message_id)
    };
    let e = RodaEngine::open(path, "en".into()).unwrap();
    e.unlock(vault).unwrap();
    assert!(!e
        .lock()
        .outbox_envelopes()
        .iter()
        .any(|e| e.client_id() == message_id));
    let ev = Sequenced {
        seq: 0,
        prev: GENESIS_PREV.into(),
        hash: chain_hash(&space, 0, GENESIS_PREV, &genesis.wire_hash()),
        env: genesis,
    };
    assert_eq!(e.lock().ingest(ev), crate::sync::Ingest::Confirmed);
    let outgoing = e.lock().outbox_envelopes();
    assert_eq!(
        outgoing
            .iter()
            .filter(|e| e.client_id() == message_id)
            .count(),
        1
    );
    assert_eq!(
        e.timeline(space)
            .unwrap()
            .iter()
            .filter(
                |e| matches!(&e.kind, EntryKind::Message { text, .. } if text == "keep this queued")
            )
            .count(),
        1
    );
    drop(e);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn permanently_rejected_genesis_fails_descendants_atomically_across_relaunch() {
    let dir =
        std::env::temp_dir().join(format!("zoen-genesis-refusal-{}", roda_types::new_id("t")));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("core.sqlite").to_string_lossy().to_string();
    let vault: Arc<dyn SecretVault> = Arc::new(MemVault::default());
    let (bad, good) = {
        let e = RodaEngine::open(path.clone(), "en".into()).unwrap();
        e.create_account(
            "Ana".into(),
            "ana".into(),
            "http://127.0.0.1:9".into(),
            vault.clone(),
        )
        .unwrap();
        let good = e
            .create_group_with("Keep queued".into(), vec![], PrivacyDto::Closed)
            .unwrap();
        e.send_message(good.clone(), "unrelated".into()).unwrap();
        let bad = e
            .create_group_with("x".repeat(100 * 1024), vec![], PrivacyDto::Closed)
            .unwrap();
        for text in ["first descendant", "second descendant"] {
            e.send_message(bad.clone(), text.into()).unwrap();
        }
        let genesis = e
            .lock()
            .outbox_envelopes()
            .into_iter()
            .find(|env| env.space() == bad)
            .unwrap();
        assert!(
            genesis.stored_len() > 90 * 1024,
            "this creation is permanently too large for the relay"
        );
        let child = e
            .lock()
            .store
            .outbox()
            .unwrap()
            .into_iter()
            .find(|p| p.event.space == bad && p.event.seen.is_some())
            .unwrap()
            .event
            .client_id;
        let invite_key = format!("invite:{child}");
        {
            let engine = e.lock();
            engine.store.set_meta(&invite_key, "0123456789").unwrap();
            engine.store.conn().execute_batch("CREATE TEMP TRIGGER refuse_meta_delete BEFORE DELETE ON meta WHEN OLD.key LIKE 'invite:%' BEGIN SELECT RAISE(ABORT, 'injected write failure'); END;").unwrap();
        }
        assert!(!e
            .lock()
            .reject(genesis.client_id(), "too large", true, None));
        {
            let engine = e.lock();
            assert!(engine.store.conn().is_autocommit());
            assert!(engine
                .store
                .outbox()
                .unwrap()
                .iter()
                .filter(|p| p.event.space == bad)
                .all(|p| !p.failed));
            assert_eq!(
                engine.store.meta(&invite_key).unwrap().as_deref(),
                Some("0123456789")
            );
            engine
                .store
                .conn()
                .execute_batch("DROP TRIGGER refuse_meta_delete")
                .unwrap();
        }
        assert!(e
            .lock()
            .reject(genesis.client_id(), "too large", true, None));
        {
            let engine = e.lock();
            let queued = engine.store.outbox().unwrap();
            let refused: Vec<_> = queued.iter().filter(|p| p.event.space == bad).collect();
            assert_eq!(refused.len(), 3);
            assert!(refused
                .iter()
                .all(|p| p.failed && p.last_error.as_deref() == Some("too large")));
            assert!(queued
                .iter()
                .filter(|p| p.event.space == good)
                .all(|p| !p.failed));
            assert!(engine.store.meta(&invite_key).unwrap().is_none());
            assert!(!engine.net.pending.values().any(|space| space == &bad));
            assert!(!engine
                .outbox_envelopes()
                .iter()
                .any(|env| env.space() == bad));
        }
        (bad, good)
    };
    let e = RodaEngine::open(path, "en".into()).unwrap();
    e.unlock(vault).unwrap();
    let engine = e.lock();
    assert!(engine
        .store
        .outbox()
        .unwrap()
        .iter()
        .filter(|p| p.event.space == bad)
        .all(|p| p.failed));
    assert!(!engine.net.pending.values().any(|space| space == &bad));
    assert!(!engine
        .outbox_envelopes()
        .iter()
        .any(|env| env.space() == bad));
    assert!(engine
        .outbox_envelopes()
        .iter()
        .any(|env| env.space() == good));
    drop(engine);
    drop(e);
    std::fs::remove_dir_all(dir).unwrap();
}

/// A relay that hides one message and rehashes the chain around the gap shows this device
/// a valid-looking chain, but the next author's causal link names the hidden event, so the
/// device refuses the forged history instead of showing it.
#[test]
fn a_device_refuses_a_relay_that_hides_a_message_and_rehashes() {
    use roda_log::{chain_hash, content_hash_of, Author, Signer, SpaceLog};
    use roda_proto::{Envelope, Sequenced};
    use roda_types::{EventBody, Privacy, SpaceKind};

    let e = RodaEngine::open(":memory:".into(), "pt-BR".into()).unwrap();
    let ana = Author::root(Signer::generate());
    let bia = Author::root(Signer::generate());
    let mut honest = SpaceLog::new("sp_fork");
    let genesis = honest
        .sequence(ana.sign_event(
            "sp_fork",
            "g",
            1,
            None,
            EventBody::SpaceCreated {
                title: "Viagem".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::Closed,
            },
        ))
        .clone();
    let seen = honest.head();
    let hidden = honest
        .sequence(ana.sign_event(
            "sp_fork",
            "h",
            2,
            seen,
            EventBody::MessagePosted {
                message: "m1".into(),
                text: "não vou mais".into(),
                attaches: None,
                reply: None,
            },
        ))
        .clone();
    let reply = bia.sign_event(
        "sp_fork",
        "r",
        3,
        honest.head(),
        EventBody::MessagePosted {
            message: "m2".into(),
            text: "ok, entendi".into(),
            attaches: None,
            reply: None,
        },
    );
    assert_eq!(
        reply.seen.as_ref().map(|s| s.hash.as_str()),
        Some(hidden.hash.as_str())
    );

    let shown = |ev: &roda_types::Event, seq: u64, prev: &str| Sequenced {
        seq,
        prev: prev.to_string(),
        hash: chain_hash("sp_fork", seq, prev, &content_hash_of(ev)),
        env: Envelope::plain(ev),
    };
    assert!(matches!(
        e.lock().ingest(shown(&genesis, 0, &genesis.prev)),
        crate::sync::Ingest::Applied
    ));
    match e.lock().ingest(shown(&reply, 1, &genesis.hash)) {
        crate::sync::Ingest::Invalid(why) => assert!(why.contains("história"), "{why}"),
        _ => panic!("the forged history was accepted"),
    }
    let space = e
        .spaces()
        .into_iter()
        .find(|s| s.title == "Viagem")
        .unwrap();
    assert!(!space.last_preview.contains("ok, entendi"));
}

#[test]
fn inline_replies_quote_and_thread_replies_gather_under_the_root() {
    let e = seeded();
    let paraty = find_space(&e, "Paraty com a Marina");
    let tl = e.timeline(paraty.id.clone()).unwrap();
    let root = tl
        .iter()
        .find(|x| matches!(&x.kind, EntryKind::Message { text, .. } if !text.is_empty()))
        .expect("a message to answer")
        .clone();
    assert!(
        tl.iter()
            .all(|x| x.reply_to.is_none() && x.in_thread.is_none()),
        "seeded messages are plain"
    );

    // Inline: stays in the chat, quoting the root.
    let inline = e
        .send_reply(paraty.id.clone(), "Fechado!".into(), root.id.clone(), false)
        .unwrap();
    let q = inline.reply_to.clone().expect("a quote");
    assert_eq!(q.id, root.id);
    assert_eq!(q.author.id, root.author.id);
    assert!(inline.in_thread.is_none());

    // Thread: two replies, the second aimed at the first, both land under the root.
    let t1 = e
        .send_reply(
            paraty.id.clone(),
            "Eu levo o protetor".into(),
            root.id.clone(),
            true,
        )
        .unwrap();
    assert_eq!(t1.in_thread.as_deref(), Some(root.id.as_str()));
    let t2 = e
        .send_reply(paraty.id.clone(), "E eu a água".into(), t1.id.clone(), true)
        .unwrap();
    assert_eq!(
        t2.in_thread.as_deref(),
        Some(root.id.as_str()),
        "threads stay one level deep"
    );

    let tl = e.timeline(paraty.id.clone()).unwrap();
    let r = tl.iter().find(|x| x.id == root.id).unwrap();
    assert_eq!(r.thread_replies, 2);
    let th = e.thread(paraty.id.clone(), root.id.clone()).unwrap();
    assert_eq!(th.len(), 3);
    assert_eq!(th[0].id, root.id);
    assert_eq!(th[0].thread_replies, 2);
    assert_eq!(th[2].id, t2.id);

    // Only messages can be answered, and the target must exist.
    assert!(e
        .send_reply(paraty.id.clone(), "?".into(), "nope".into(), false)
        .is_err());
    assert!(
        e.verify_all().iter().all(|r| r.valid),
        "replies are signed like any message"
    );
}
