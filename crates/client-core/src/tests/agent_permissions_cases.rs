//! Complete agent permissions scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn approval_is_bound_to_the_exact_content() {
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
            e.plan_page(plan.id.clone()).unwrap().content_hash,
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

pub(super) fn approve_all_and_deny() {
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

pub(super) fn trust_levels_change_what_agents_can_do() {
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

pub(super) fn budget_stops_at_ceiling_and_can_be_raised() {
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

pub(super) fn budget_rejects_nonpositive_and_overflowing_increases_without_mutation() {
    let e = seeded();
    let fin = agent(&e, "Financeiro");
    let before = e.agent_profile(fin.id.clone());
    for extra in [0, -1, i64::MIN, i64::MAX] {
        assert!(matches!(
            e.raise_budget(fin.id.clone(), extra),
            Err(CoreError::Invalid { .. })
        ));
        assert_eq!(e.agent_profile(fin.id.clone()), before);
    }
    let max = e.raise_budget(fin.id.clone(), i64::MAX - 500).unwrap();
    assert_eq!(max.budget_limit_cents, Some(i64::MAX));
    assert!(matches!(
        e.raise_budget(fin.id.clone(), 1),
        Err(CoreError::Invalid { .. })
    ));
    assert_eq!(e.agent_profile(fin.id), max);
}

pub(super) fn nonmember_agent_cannot_publish_to_community() {
    let e = seeded();
    let com = find_space(&e, "Viajantes do Litoral");
    let zoen = agent(&e, "Zoen");
    assert!(
        e.agent_say(com.id.clone(), zoen.id, "oi".into(), 0)
            .is_err(),
        "Zoen não é membro da comunidade"
    );
}

pub(super) async fn an_offline_invite_cannot_append_a_grant_or_manufacture_a_link() {
    let e = seeded();
    let space = find_space(&e, "Viajantes do Litoral");
    let before = e.log_events(space.id.clone()).unwrap();
    assert!(e.create_invite(space.id.clone()).await.is_err());
    let after = e.log_events(space.id).unwrap();
    assert_eq!(after.len(), before.len());
    assert_eq!(
        after
            .iter()
            .map(|event| (&event.hash, &event.signature))
            .collect::<Vec<_>>(),
        before
            .iter()
            .map(|event| (&event.hash, &event.signature))
            .collect::<Vec<_>>()
    );
}

pub(super) fn agent_activity_reads_the_log_with_costs() {
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

pub(super) fn device_grants_are_per_mini_app_logged_and_revocable() {
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
