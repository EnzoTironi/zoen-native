//! Roteiro de dados fictícios da demo (um elenco, uma história contínua).
//!
//! Todas as Identidades da demo assinam com chaves geradas neste aparelho — inclusive
//! as da Marina e do Organizador, que no produto real chegariam pelo Roda Sync
//! assinadas nos aparelhos delas. Isso é explicitamente uma simulação de pares.

use roda_types::*;

use crate::apps;
use crate::engine::{now_ms, Engine};
use crate::i18n::t;
use crate::CoreError;

const MIN: i64 = 60_000;

fn person(name: &str, handle: &str, tint: &str, bio: &str) -> Identity {
    Identity {
        id: String::new(),
        kind: IdentityKind::Person,
        name: name.into(),
        handle: handle.into(),
        tint_hex: tint.into(),
        glyph: None,
        owner: None,
        bio: bio.into(),
    }
}

fn agent(name: &str, handle: &str, tint: &str, glyph: &str, owner: &str, bio: &str) -> Identity {
    Identity {
        id: String::new(),
        kind: IdentityKind::Agent,
        name: name.into(),
        handle: handle.into(),
        tint_hex: tint.into(),
        glyph: Some(glyph.into()),
        owner: Some(owner.into()),
        bio: bio.into(),
    }
}

fn line(text: &str, reais: i64) -> PlanLine {
    PlanLine {
        id: new_id("ln"),
        text: text.into(),
        cost_cents: reais * 100,
        done: false,
    }
}

fn grant(grantor: &str, grantee: &str, scope: GrantScope, capability: Capability) -> EventBody {
    EventBody::GrantIssued {
        grant: Grant {
            id: new_id("gr"),
            grantor: grantor.into(),
            grantee: Some(grantee.into()),
            scope,
            capability,
            expires_at_ms: None,
        },
    }
}

fn msg(text: &str) -> EventBody {
    EventBody::MessagePosted {
        message: new_id("msg"),
        text: text.into(),
        attaches: None,
    }
}

pub(crate) fn seed(e: &mut Engine) -> Result<(), CoreError> {
    let t0 = now_ms();
    let at = |minutes_ago: i64| t0 - minutes_ago * MIN;

    let enzo = e.create_identity(
        person(
            "Enzo",
            "enzo",
            "#4F7CFF",
            &t("Construindo o Zoen.", "Building Zoen."),
        ),
        true,
    )?;
    e.store.set_meta("me", &enzo)?;
    e.me = Some(enzo.clone());
    let marina = e.create_identity(
        person(
            "Marina",
            "marina",
            "#FF5C8A",
            &t(
                "Arquiteta. Planeja tudo em planilha.",
                "Architect. Plans everything in a spreadsheet.",
            ),
        ),
        true,
    )?;
    let lucas = e.create_identity(
        person(
            "Lucas",
            "lucas",
            "#19B5A5",
            &t("iOS no Zoen.", "iOS at Zoen."),
        ),
        true,
    )?;
    let ana = e.create_identity(
        person(
            "Ana",
            "ana",
            "#F59E0B",
            &t(
                "Design. Cria a comunidade Viajantes do Litoral.",
                "Design. Runs the Coastal Travelers community.",
            ),
        ),
        true,
    )?;
    let zoen = e.create_identity(
        agent(
            "Zoen",
            "zoen",
            "#7C5CFF",
            "sparkles",
            &enzo,
            &t(
                "Seu agente. Transforma conversa em coisa.",
                "Your agent. Turns conversation into things.",
            ),
        ),
        true,
    )?;
    let fin = e.create_identity(
        agent(
            &t("Financeiro", "Finance"),
            "financeiro",
            "#22C55E",
            "chart.pie.fill",
            &enzo,
            &t(
                "Cuida do dinheiro: orçamento, divisão e reservas.",
                "Handles money: budget, splits and bookings.",
            ),
        ),
        true,
    )?;
    let org = e.create_identity(
        agent(
            &t("Organizador", "Organizer"),
            "organizador",
            "#FF9F0A",
            "checklist",
            &marina,
            &t(
                "Listas, lembretes e agenda da Marina.",
                "Marina's lists, reminders and calendar.",
            ),
        ),
        true,
    )?;
    let guia = e.create_identity(
        agent(
            &t("Guia", "Guide"),
            "guia",
            "#0EA5E9",
            "map.fill",
            &ana,
            &t(
                "Marés, trilhas e dicas da comunidade.",
                "Tides, trails and community tips.",
            ),
        ),
        true,
    )?;

    // ── Pessoal (oculto): orçamentos dos meus agentes (plano Plus: R$ 15 de IA) ──
    let me_space = new_id("sp");
    e.append_at(
        &me_space,
        &enzo,
        at(60 * 24 * 6),
        EventBody::SpaceCreated {
            title: t(&t("Pessoal", "Personal"), "Personal"),
            kind: SpaceKind::Personal,
            privacy: Privacy::EndToEnd,
        },
    )?;
    e.append_at(
        &me_space,
        &enzo,
        at(60 * 24 * 6),
        grant(
            &enzo,
            &zoen,
            GrantScope::Everywhere,
            Capability::MonthlyBudget { cents: 1_000 },
        ),
    )?;
    e.append_at(
        &me_space,
        &enzo,
        at(60 * 24 * 6),
        grant(
            &enzo,
            &fin,
            GrantScope::Everywhere,
            Capability::MonthlyBudget { cents: 500 },
        ),
    )?;

    // ── Viajantes do Litoral (comunidade fechada da Ana) ──
    let com = new_id("sp");
    e.append_at(
        &com,
        &ana,
        at(60 * 24 * 3),
        EventBody::SpaceCreated {
            title: t(
                &t("Viajantes do Litoral", "Coastal Travelers"),
                "Coastal Travelers",
            ),
            kind: SpaceKind::Community,
            privacy: Privacy::Closed,
        },
    )?;
    e.append_at(
        &com,
        &ana,
        at(60 * 24 * 3),
        EventBody::MemberAdded {
            identity: guia.clone(),
            role: Role::Member,
        },
    )?;
    e.append_at(
        &com,
        &ana,
        at(60 * 24 * 3),
        EventBody::MemberAdded {
            identity: enzo.clone(),
            role: Role::Member,
        },
    )?;
    e.append_at(
        &com,
        &ana,
        at(60 * 24 * 3),
        grant(
            &ana,
            &guia,
            GrantScope::Space(com.clone()),
            Capability::Trust(TrustLevel::Listen),
        ),
    )?;
    e.append_at(
        &com,
        &ana,
        at(60 * 24 * 2),
        msg(&t(
            "Bem-vindos! Aqui a gente troca roteiro de praia, trilha e pousada boa e barata.",
            "Welcome! This is where we swap beach itineraries, trails and good cheap inns.",
        )),
    )?;
    e.append_at(&com, &guia, at(60 * 9), msg(&t("Maré baixa no sábado às 9h40 — melhor horário pra chegar na Praia do Sono pela trilha.", "Low tide on Saturday at 9:40 am — the best time to hike to Praia do Sono.")))?;

    // Trail mini-app pinned in the community (investor showcase: trip-group feel).
    seed_coastal_hike(e, &com, &enzo, &ana, &guia, t0)?;

    // ── Zoen · Produto ──
    let team = new_id("sp");
    e.append_at(
        &team,
        &enzo,
        at(60 * 24 * 2),
        EventBody::SpaceCreated {
            title: t(&t("Zoen · Produto", "Zoen · Product"), "Zoen · Product"),
            kind: SpaceKind::Group,
            privacy: Privacy::EndToEnd,
        },
    )?;
    for m in [&lucas, &ana, &zoen] {
        e.append_at(
            &team,
            &enzo,
            at(60 * 24 * 2),
            EventBody::MemberAdded {
                identity: m.clone(),
                role: Role::Member,
            },
        )?;
    }
    e.append_at(
        &team,
        &lucas,
        at(60 * 6),
        msg(&t(
            "Subi o build novo no TestFlight. A cápsula de vidro encolhendo ao rolar ficou linda.",
            "New build is up on TestFlight. The glass capsule shrinking on scroll looks gorgeous.",
        )),
    )?;
    e.append_at(
        &team,
        &ana,
        at(60 * 5 + 40),
        msg(&t(
            "O cartão de pedido no escuro tá impecável. Só falta o ícone do Mac.",
            "The request card in dark mode is flawless. Only the Mac icon is missing.",
        )),
    )?;
    let summary = new_id("it");
    e.append_at(&team, &zoen, at(60 * 5), EventBody::ItemCreated {
        item: summary.clone(),
        kind: ItemKind::Note,
        content: ItemContent::Text { text: t(&t("Resumo do dia\n• Build novo no TestFlight (Lucas)\n• Cartão de pedido aprovado no escuro (Ana)\n• Pendente: ícone do app no Mac", "Daily summary\n• New build on TestFlight (Lucas)\n• Request card approved in dark mode (Ana)\n• Pending: the Mac app icon"), "Daily summary\n• New build on TestFlight (Lucas)\n• Request card approved in dark mode (Ana)\n• Pending: the Mac app icon") },
        origin: t(&t("Zoen · resumo da conversa", "Zoen · conversation summary"), "Zoen · conversation summary"),
    })?;
    e.append_at(
        &team,
        &zoen,
        at(60 * 5),
        EventBody::MessagePosted {
            message: new_id("msg"),
            text: t(
                &t(
                    "Resumo do dia: 2 entregas, 1 pendência (ícone do Mac).",
                    "Daily summary: 2 shipped, 1 pending (Mac icon).",
                ),
                "Daily summary: 2 shipped, 1 pending (Mac icon).",
            ),
            attaches: Some(summary),
        },
    )?;
    e.append_at(
        &team,
        &zoen,
        at(60 * 5),
        EventBody::UsageRecorded {
            agent: zoen.clone(),
            cents: 140,
            what: t(&t("resumo", "summary"), "summary"),
        },
    )?;

    // ── Turma do Sábado (grupo de amigos: onde moram os mini-apps do Wabi) ──
    let turma = new_id("sp");
    e.append_at(
        &turma,
        &enzo,
        at(60 * 24 * 4),
        EventBody::SpaceCreated {
            title: t(&t("Turma do Sábado", "Saturday Crew"), "Saturday Crew"),
            kind: SpaceKind::Group,
            privacy: Privacy::EndToEnd,
        },
    )?;
    for m in [&marina, &lucas, &ana, &zoen] {
        e.append_at(
            &turma,
            &enzo,
            at(60 * 24 * 4),
            EventBody::MemberAdded {
                identity: m.clone(),
                role: Role::Member,
            },
        )?;
    }
    e.append_at(
        &turma,
        &enzo,
        at(60 * 24 * 4),
        grant(
            &enzo,
            &zoen,
            GrantScope::Space(turma.clone()),
            Capability::Trust(TrustLevel::Act),
        ),
    )?;
    e.append_at(
        &turma,
        &lucas,
        at(52),
        msg(&t(
            "Vocês ainda acham que Liubliana fica na Eslováquia? 😂",
            "Do you guys still think Ljubljana is in Slovakia? 😂",
        )),
    )?;
    e.append_at(
        &turma,
        &ana,
        at(50),
        msg(&t("Foi UMA vez", "That was ONE time")),
    )?;
    e.append_at(
        &turma,
        &marina,
        at(48),
        msg(&t(
            "Esse grupo precisa de um mascote",
            "This group needs a mascot",
        )),
    )?;

    // Live mini-apps pinned in the chat (and on Home): hike, pet, MapTap.
    // These are real Items in the Space — ChatPinStrip / HomeStrip read them from the core.
    seed_turma_apps(e, &turma, &enzo, &marina, &lucas, &ana, &zoen, t0)?;

    // ── Zoen (DM com o meu agente: onde o momento mágico acontece) ──
    let dm_zoen = new_id("sp");
    e.append_at(
        &dm_zoen,
        &enzo,
        at(60 * 26),
        EventBody::SpaceCreated {
            title: "Zoen".into(),
            kind: SpaceKind::Direct,
            privacy: Privacy::EndToEnd,
        },
    )?;
    e.append_at(
        &dm_zoen,
        &enzo,
        at(60 * 26),
        EventBody::MemberAdded {
            identity: zoen.clone(),
            role: Role::Member,
        },
    )?;
    e.append_at(
        &dm_zoen,
        &zoen,
        at(60 * 26),
        msg(&t(
            "Oi, Enzo. O que você quer resolver?",
            "Hi, Enzo. What do you want to get done?",
        )),
    )?;
    e.append_at(
        &dm_zoen,
        &enzo,
        at(60 * 25),
        msg(&t(
            "Me lembra de renovar o passaporte antes de dezembro",
            "Remind me to renew my passport before December",
        )),
    )?;
    let task = new_id("it");
    e.append_at(&dm_zoen, &zoen, at(60 * 25), EventBody::ItemCreated {
        item: task.clone(),
        kind: ItemKind::Task,
        content: ItemContent::Text { text: t(&t("Renovar o passaporte\nAgendar na PF até 15/11 · levar RG e passaporte antigo", "Renew passport\nBook an appointment by Nov 15 · bring ID and the old passport"), "Renew passport\nBook an appointment by Nov 15 · bring ID and the old passport") },
        origin: t(&t("Zoen · a partir de “Me lembra de renovar o passaporte”", "Zoen · from “Remind me to renew my passport”"), "Zoen · from “Remind me to renew my passport”"),
    })?;
    e.append_at(
        &dm_zoen,
        &zoen,
        at(60 * 25),
        EventBody::MessagePosted {
            message: new_id("msg"),
            text: t(
                &t(
                    "Anotado. Criei a tarefa e te lembro na segunda.",
                    "Got it. I created the task and I'll remind you on Monday.",
                ),
                "Got it. I created the task and I'll remind you on Monday.",
            ),
            attaches: Some(task),
        },
    )?;
    e.append_at(
        &dm_zoen,
        &zoen,
        at(60 * 25),
        EventBody::UsageRecorded {
            agent: zoen.clone(),
            cents: 170,
            what: t(&t("tarefa", "task"), "task"),
        },
    )?;

    // ── Marina (DM) ──
    let dm_marina = new_id("sp");
    e.append_at(
        &dm_marina,
        &enzo,
        at(60 * 24 * 5),
        EventBody::SpaceCreated {
            title: "Marina".into(),
            kind: SpaceKind::Direct,
            privacy: Privacy::EndToEnd,
        },
    )?;
    e.append_at(
        &dm_marina,
        &enzo,
        at(60 * 24 * 5),
        EventBody::MemberAdded {
            identity: marina.clone(),
            role: Role::Member,
        },
    )?;
    e.append_at(
        &dm_marina,
        &enzo,
        at(52),
        msg(&t("Comprei o protetor solar ☀️", "Bought the sunscreen ☀️")),
    )?;
    e.append_at(
        &dm_marina,
        &marina,
        at(44),
        msg(&t(
            "Perfeito! Leva o carregador portátil também?",
            "Perfect! Can you bring the power bank too?",
        )),
    )?;

    // ── Paraty com a Marina (o cenário da demo) ──
    let paraty = new_id("sp");
    e.append_at(
        &paraty,
        &enzo,
        at(200),
        EventBody::SpaceCreated {
            title: t(
                &t("Paraty com a Marina", "Paraty with Marina"),
                "Paraty with Marina",
            ),
            kind: SpaceKind::Group,
            privacy: Privacy::EndToEnd,
        },
    )?;
    for m in [&marina, &zoen, &fin, &org] {
        e.append_at(
            &paraty,
            &enzo,
            at(200),
            EventBody::MemberAdded {
                identity: m.clone(),
                role: Role::Member,
            },
        )?;
    }
    e.append_at(
        &paraty,
        &enzo,
        at(200),
        grant(
            &enzo,
            &zoen,
            GrantScope::Space(paraty.clone()),
            Capability::Trust(TrustLevel::Act),
        ),
    )?;
    e.append_at(
        &paraty,
        &enzo,
        at(200),
        grant(
            &enzo,
            &fin,
            GrantScope::Space(paraty.clone()),
            Capability::Trust(TrustLevel::Act),
        ),
    )?;
    e.append_at(
        &paraty,
        &marina,
        at(199),
        grant(
            &marina,
            &org,
            GrantScope::Space(paraty.clone()),
            Capability::Trust(TrustLevel::Suggest),
        ),
    )?;
    e.append_at(
        &paraty,
        &marina,
        at(190),
        msg(&t(
            "Feriado chegando… bora pra Paraty? 🌊",
            "Long weekend coming up… Paraty? 🌊",
        )),
    )?;
    e.append_at(
        &paraty,
        &enzo,
        at(188),
        msg(&t(
            "Bora! @Zoen planeja nosso fim de semana em Paraty, até R$ 1.500",
            "Let's go! @Zoen plan our weekend in Paraty, up to $1,500",
        )),
    )?;

    let pousada = line(
        &t(
            "Pousada Casa Azul · 2 noites, café incluso",
            "Casa Azul Inn · 2 nights, breakfast included",
        ),
        640,
    );
    let onibus = line(
        &t(
            "Ônibus Rio → Paraty, ida e volta (2 pessoas)",
            "Bus Rio → Paraty, round trip (2 people)",
        ),
        248,
    );
    let jantar_v1 = line(
        &t(
            "Jantar no centro histórico",
            "Dinner in the historic center",
        ),
        220,
    );
    let jantar_id = jantar_v1.id.clone();
    let plan = PlanDoc {
        title: t(
            &t("Fim de semana em Paraty", "Weekend in Paraty"),
            "Weekend in Paraty",
        ),
        summary: t(
            &t(
                "Sexta a domingo · Enzo e Marina · centro histórico, escuna e Praia do Sono",
                "Friday to Sunday · Enzo and Marina · historic center, schooner and Praia do Sono",
            ),
            "Friday to Sunday · Enzo and Marina · historic center, schooner and Praia do Sono",
        ),
        budget_cents: Some(150_000),
        sections: vec![
            PlanSection {
                title: t(&t("Transporte", "Transport"), "Transport"),
                lines: vec![
                    onibus.clone(),
                    line(
                        &t("Táxi rodoviária ↔ pousada", "Taxi bus station ↔ inn"),
                        60,
                    ),
                ],
            },
            PlanSection {
                title: t(&t("Hospedagem", "Stay"), "Stay"),
                lines: vec![pousada.clone()],
            },
            PlanSection {
                title: t(&t("Passeios", "Tours"), "Tours"),
                lines: vec![
                    line(
                        &t(
                            "Escuna pelas ilhas (2 pessoas)",
                            "Schooner around the islands (2 people)",
                        ),
                        180,
                    ),
                    line(&t("Trilha da Praia do Sono", "Praia do Sono trail"), 0),
                ],
            },
            PlanSection {
                title: t(&t("Comida", "Food"), "Food"),
                lines: vec![
                    jantar_v1,
                    line(&t("Cafés e lanches", "Coffee and snacks"), 0),
                ],
            },
        ],
    };
    let plan_item = new_id("it");
    e.append_at(
        &paraty,
        &zoen,
        at(187),
        EventBody::ItemCreated {
            item: plan_item.clone(),
            kind: ItemKind::Plan,
            content: ItemContent::Plan(plan.clone()),
            origin: t(
                &t(
                    "Zoen · a partir de “planeja nosso fim de semana em Paraty, até R$ 1.500”",
                    "Zoen · from “plan our weekend in Paraty, up to $1,500”",
                ),
                "Zoen · from “plan our weekend in Paraty, up to $1,500”",
            ),
        },
    )?;
    e.append_at(&paraty, &zoen, at(187), EventBody::MessagePosted { message: new_id("msg"), text: t(&t("Montei o plano: ônibus, 2 noites na Casa Azul e escuna. R$ 1.348 de R$ 1.500 · sobra R$ 152.", "Here's the plan: bus, 2 nights at Casa Azul and a schooner. $1,348 of $1,500 · $152 left."), "Here's the plan: bus, 2 nights at Casa Azul and a schooner. $1,348 of $1,500 · $152 left."), attaches: Some(plan_item.clone()) })?;
    e.append_at(
        &paraty,
        &zoen,
        at(187),
        EventBody::UsageRecorded {
            agent: zoen.clone(),
            cents: 0,
            what: t(
                &t("plano (no aparelho)", "plan (on device)"),
                "plan (on device)",
            ),
        },
    )?;

    // A countdown mini-app for the trip (it feeds the Home strip and the widget).
    let trip = new_id("it");
    let trip_title = t(
        &t("Feriado em Paraty", "Paraty long weekend"),
        "Paraty long weekend",
    );
    let trip_state = serde_json::json!({ "title": trip_title, "place": "Paraty", "target": t0 + 9 * 24 * 60 * MIN + 7 * 60 * MIN, "log": [{ "who": "Zoen", "what": t(&t("criou a contagem", "started the countdown"), "started the countdown"), "at": at(186) }] });
    e.append_at(
        &paraty,
        &zoen,
        at(186),
        EventBody::ItemCreated {
            item: trip.clone(),
            kind: ItemKind::App,
            content: ItemContent::App(AppDoc {
                app: "countdown".into(),
                resource_uri: "ui://roda/countdown".into(),
                title: trip_title.clone(),
                state_json: trip_state.to_string(),
            }),
            origin: t(
                &t("Zoen · contagem da viagem", "Zoen · trip countdown"),
                "Zoen · trip countdown",
            ),
        },
    )?;
    e.append_at(
        &paraty,
        &enzo,
        at(186),
        grant(
            &enzo,
            &format!("app:{trip}"),
            GrantScope::Item(trip.clone()),
            Capability::Trust(TrustLevel::Act),
        ),
    )?;
    e.append_at(
        &paraty,
        &zoen,
        at(186),
        EventBody::MessagePosted {
            message: new_id("msg"),
            text: t(
                &t(
                    "E pus uma contagem para a viagem.",
                    "And I added a countdown for the trip.",
                ),
                "And I added a countdown for the trip.",
            ),
            attaches: Some(trip.clone()),
        },
    )?;

    e.append_at(
        &paraty,
        &marina,
        at(175),
        msg(&t(
            "Amei! Troca o jantar por aquele do centro histórico que tem moqueca?",
            "Love it! Can you swap dinner for that place in the historic center with the moqueca?",
        )),
    )?;
    let mut plan_v2 = plan.clone();
    if let Some(l) = plan_v2.line_mut(&jantar_id) {
        l.text = t(
            &t(
                "Jantar no Banana da Terra (moqueca)",
                "Dinner at Banana da Terra (moqueca)",
            ),
            "Dinner at Banana da Terra (moqueca)",
        );
        l.cost_cents = 26_000;
    }
    e.append_at(
        &paraty,
        &zoen,
        at(174),
        EventBody::ItemVersioned {
            item: plan_item.clone(),
            content: ItemContent::Plan(plan_v2),
            note: t(
                &t(
                    "Trocou o jantar pelo Banana da Terra",
                    "Swapped dinner for Banana da Terra",
                ),
                "Swapped dinner for Banana da Terra",
            ),
        },
    )?;
    e.append_at(
        &paraty,
        &zoen,
        at(174),
        msg(&t(
            "Troquei: jantar no Banana da Terra (R$ 260). Total agora R$ 1.388.",
            "Swapped: dinner at Banana da Terra ($260). Total is now $1,388.",
        )),
    )?;

    e.append_at(&paraty, &fin, at(160), msg(&t("Dividi meio a meio: R$ 694 pra cada. Deixei as reservas prontas em Atividade pra você aprovar.", "Split it 50/50: $694 each. The bookings are ready in Activity for you to approve.")))?;
    e.append_at(
        &paraty,
        &fin,
        at(160),
        EventBody::UsageRecorded {
            agent: fin.clone(),
            cents: 420,
            what: t(
                &t("divisão e reservas", "split and bookings"),
                "split and bookings",
            ),
        },
    )?;

    // Pedidos do Financeiro: linhas vermelhas (dinheiro acima do teto) e envio para fora.
    e.open_request_at(
        &paraty,
        &fin,
        at(159),
        &t("Reservar a Pousada Casa Azul", "Book the Casa Azul Inn"),
        &t(
            "2 noites, sexta e sábado, café incluso. Cancelamento grátis até quarta.",
            "2 nights, Friday and Saturday, breakfast included. Free cancellation until Wednesday.",
        ),
        &t("Pousada Casa Azul", "Casa Azul Inn"),
        ActionClass::Money { cents: 64_000 },
        Some((plan_item.clone(), pousada.id.clone())),
    )?;
    e.open_request_at(
        &paraty,
        &fin,
        at(158),
        &t("Comprar as passagens de ônibus", "Buy the bus tickets"),
        &t(
            "Rio → Paraty sexta 18h, volta domingo 17h · 2 pessoas.",
            "Rio → Paraty Friday 6 pm, back Sunday 5 pm · 2 people.",
        ),
        "Viação Costa Verde",
        ActionClass::Money { cents: 24_800 },
        Some((plan_item.clone(), onibus.id.clone())),
    )?;
    e.open_request_at(
        &paraty,
        &fin,
        at(157),
        &t(
            "Mandar a cobrança de R$ 694 para a Marina",
            "Send Marina the $694 payment request",
        ),
        &t(
            "Pix com o resumo do plano. Ela recebe pelo WhatsApp.",
            "Payment link with the plan summary. She gets it on WhatsApp.",
        ),
        &t("Marina, fora do Zoen", "Marina, outside Zoen"),
        ActionClass::External,
        None,
    )?;

    e.append_at(
        &paraty,
        &org,
        at(150),
        msg(&t(
            "Criei a lista de malas da Marina e um lembrete pra sexta, 17h.",
            "I made Marina's packing list and a reminder for Friday at 5 pm.",
        )),
    )?;
    e.append_at(
        &paraty,
        &marina,
        at(18),
        msg(&t(
            "@Enzo já aprovou a pousada? Eles seguram a reserva só até hoje 😅",
            "@Enzo did you approve the inn yet? They only hold the booking until today 😅",
        )),
    )?;

    // Tudo lido, menos o que chegou nos últimos minutos.
    for s in [&com, &team, &dm_zoen] {
        e.mark_read(s)?;
    }
    Ok(())
}

/// Saturday Crew: hike (locked + ride), pet Paçoca, MapTap — feeds Home strip + chat tiles.
#[expect(clippy::too_many_arguments, reason = "the seeded cast, by name")]
fn seed_turma_apps(
    e: &mut Engine,
    turma: &str,
    enzo: &str,
    marina: &str,
    lucas: &str,
    ana: &str,
    zoen: &str,
    t0: i64,
) -> Result<(), CoreError> {
    let at = |m: i64| t0 - m * MIN;

    // ── Hike (Tomales locked in, day planned) ──
    let hike_day = t(&t("Sábado", "Saturday"), "Saturday");
    let hike_title = if crate::i18n::is_en() {
        format!("{hike_day} hike")
    } else {
        format!("Trilha de {}", hike_day.to_lowercase())
    };
    let hike_state = serde_json::json!({
        "title": hike_title,
        "area": t(&t("Serra da Cantareira", "Bay Area"), "Bay Area"),
        "day": hike_day,
        // pt-BR: São Paulo crew → Pedra Grande (Cantareira, forest art = "steep");
        // en: Bay Area → Tomales Point (coastal art). Photos are keyed by trail id.
        "trails": if crate::i18n::is_en() { serde_json::json!([
            { "id": "tomales", "name": "Tomales Point", "votes": ["Marina", "Lucas", "Enzo"] },
            { "id": "steep", "name": "Steep Ravine", "votes": ["Ana"] },
            { "id": "lands", "name": "Lands End", "votes": [] }
        ]) } else { serde_json::json!([
            { "id": "steep", "name": "Pedra Grande", "votes": ["Marina", "Lucas", "Enzo"] },
            { "id": "tomales", "name": "Praia Brava", "votes": ["Ana"] },
            { "id": "lands", "name": "Pico do Jaraguá", "votes": [] }
        ]) },
        "decided": if crate::i18n::is_en() { "tomales" } else { "steep" },
        "itinerary": [
            { "time": "8:00", "text": t(&t("Lucas · carro pronto", "Lucas · car ready"), "Lucas · car ready") },
            { "time": "8:15", "text": t(&t("Enzo · Pinheiros", "Enzo · Noe Valley"), "Enzo · Noe Valley") },
            { "time": "8:30", "text": t(&t("Ana + Marina · Vila Madalena", "Ana + Marina · Mission"), "Ana + Marina · Mission") },
            { "time": "10:00", "text": t(&t("Começo da trilha Pedra Grande", "Tomales Point trailhead"), "Tomales Point trailhead") },
            { "time": "15:00", "text": t(&t("Volta de carro", "Drive back"), "Drive back") }
        ],
        "album": [],
        "log": [
            { "who": "Zoen", "what": t(&t("abriu a trilha", "opened the hike"), "opened the hike"), "at": at(45) },
            { "who": "Marina", "what": t(&t("votou em Pedra Grande", "voted for Tomales Point"), "voted for Tomales Point"), "at": at(44) },
            { "who": "Lucas", "what": t(&t("fechou: Pedra Grande", "locked in Tomales Point"), "locked in Tomales Point"), "at": at(42) },
            { "who": "Zoen", "what": t(&t("montou o roteiro", "planned the day"), "planned the day"), "at": at(41) }
        ]
    });
    let hike = new_id("it");
    e.append_at(
        turma,
        zoen,
        at(45),
        EventBody::ItemCreated {
            item: hike.clone(),
            kind: ItemKind::App,
            content: ItemContent::App(AppDoc {
                app: "hike".into(),
                resource_uri: "ui://roda/hike".into(),
                title: hike_title.clone(),
                state_json: hike_state.to_string(),
            }),
            origin: t(
                &t(
                    "Zoen · a partir de “planeja uma trilha”",
                    "Zoen · from “plan us a hike”",
                ),
                "Zoen · from “plan us a hike”",
            ),
        },
    )?;
    e.append_at(
        turma,
        enzo,
        at(45),
        grant(
            enzo,
            &format!("app:{hike}"),
            GrantScope::Item(hike.clone()),
            Capability::Trust(TrustLevel::Act),
        ),
    )?;
    e.append_at(
        turma,
        zoen,
        at(45),
        EventBody::MessagePosted {
            message: new_id("msg"),
            text: t(
                &t(
                    "Separei três trilhas boas pra sábado, com mapa e fotos. Comparem e votem: quando fechar, eu monto o roteiro com as caronas.",
                    "Found three good trails for Saturday, with maps and photos. Compare and vote: once it’s locked in, I’ll plan the day and the rides.",
                ),
                "Found three good trails for Saturday, with maps and photos. Compare and vote: once it’s locked in, I’ll plan the day and the rides.",
            ),
            attaches: Some(hike.clone()),
        },
    )?;
    e.append_at(
        turma,
        marina,
        at(44),
        msg(&t("Pedra Grande!! vista demais 🌄", "Tomales!! the elk 🦌")),
    )?;
    e.append_at(
        turma,
        lucas,
        at(43),
        msg(&t(
            "Tô dentro. Eu dirijo, cabem mais 3 no carro",
            "I’m in. I can drive, room for 3 in the car",
        )),
    )?;
    e.append_at(
        turma,
        ana,
        at(42),
        msg(&t(
            "tô dentro! mas a Steep Ravine tem sombra…",
            "in! Steep Ravine has shade though…",
        )),
    )?;
    e.append_at(
        turma,
        zoen,
        at(41),
        msg(&t(
            "Fechado: Pedra Grande. Roteiro com o carro do Lucas pronto — pega o Enzo em Pinheiros e Ana + Marina na Vila Madalena.",
            "Locked in: Tomales Point. Lucas drives — picks up Enzo in Noe Valley, Ana and Marina in the Mission.",
        )),
    )?;

    // ── Pet Paçoca ──
    let pet_name = t(&t("Paçoca", "Peanut"), "Peanut");
    let pet_state = serde_json::json!({
        "name": pet_name,
        "species": "jumento",
        "fullness": 68.0, "energy": 54.0, "joy": 82.0,
        "asleep": false, "released": false,
        "updated": at(40), "born": at(47),
        "mood": t(&t("contente", "content"), "content"),
        "dash": { "best": { "Lucas": { "meters": 142, "carrots": 7, "at": at(39) }, "Ana": { "meters": 96, "carrots": 4, "at": at(39) } }, "runs": 2 },
        "log": [
            { "who": "Zoen", "what": t(&t("adotou", "adopted"), "adopted"), "at": at(47) },
            { "who": "Marina", "what": t(&t("deu comida", "fed"), "fed"), "at": at(46) },
            { "who": "Enzo", "what": t(&t("renomeou o Jumento para Paçoca", "renamed Donkey to Peanut"), "renamed Donkey to Peanut"), "at": at(40) }
        ]
    });
    let pet = new_id("it");
    e.append_at(
        turma,
        zoen,
        at(47),
        EventBody::ItemCreated {
            item: pet.clone(),
            kind: ItemKind::App,
            content: ItemContent::App(AppDoc {
                app: "pet".into(),
                resource_uri: "ui://roda/pet".into(),
                title: pet_name.clone(),
                state_json: pet_state.to_string(),
            }),
            origin: t(
                &t("Zoen · mascote do grupo", "Zoen · group mascot"),
                "Zoen · group mascot",
            ),
        },
    )?;
    e.append_at(
        turma,
        enzo,
        at(47),
        grant(
            enzo,
            &format!("app:{pet}"),
            GrantScope::Item(pet.clone()),
            Capability::Trust(TrustLevel::Act),
        ),
    )?;
    e.append_at(
        turma,
        zoen,
        at(47),
        EventBody::MessagePosted {
            message: new_id("msg"),
            text: t(
                &t(
                    "Adotei um jumento pro grupo. Cuidem dele!",
                    "I adopted a donkey for the group. Take care of him!",
                ),
                "I adopted a donkey for the group. Take care of him!",
            ),
            attaches: Some(pet.clone()),
        },
    )?;

    // ── MapTap (Lucas leads) ──
    let (map_title, mut map_st) = apps::create("maptap", &serde_json::json!({}), "Zoen", at(38))
        .map_err(|m| CoreError::Invalid { reason: m })?;
    // One locked guess so the Home tile shows a leaderboard.
    if let Some(place) = map_st["places"].as_array().and_then(|a| a.first()) {
        let lat = place["lat"].as_f64().unwrap_or(0.0);
        let lon = place["lon"].as_f64().unwrap_or(0.0);
        map_st["guesses"] = serde_json::json!({
            "Lucas": { "0": { "lat": lat + 1.4, "lon": lon - 2.1, "km": 180.0, "points": 4148 } }
        });
        map_st["order"] = serde_json::json!(["Lucas"]);
        map_st["log"] = serde_json::json!([
            { "who": "Lucas", "what": t(&t("chutou", "guessed"), "guessed"), "at": at(37) }
        ]);
    }
    let maptap = new_id("it");
    e.append_at(
        turma,
        zoen,
        at(38),
        EventBody::ItemCreated {
            item: maptap.clone(),
            kind: ItemKind::App,
            content: ItemContent::App(AppDoc {
                app: "maptap".into(),
                resource_uri: "ui://roda/maptap".into(),
                title: map_title,
                state_json: map_st.to_string(),
            }),
            origin: t(&t("Zoen · MapTap", "Zoen · MapTap"), "Zoen · MapTap"),
        },
    )?;
    e.append_at(
        turma,
        enzo,
        at(38),
        grant(
            enzo,
            &format!("app:{maptap}"),
            GrantScope::Item(maptap.clone()),
            Capability::Trust(TrustLevel::Act),
        ),
    )?;
    e.append_at(
        turma,
        zoen,
        at(38),
        EventBody::MessagePosted {
            message: new_id("msg"),
            text: t(
                &t(
                    "MapTap no grupo — 5 lugares, quem chega mais perto.",
                    "MapTap for the group — 5 places, closest guess wins.",
                ),
                "MapTap for the group — 5 places, closest guess wins.",
            ),
            attaches: Some(maptap),
        },
    )?;

    let _ = (marina, lucas, ana); // used in hike votes / messages
    Ok(())
}

/// Coastal Travelers: trail app pinned for the community trip vibe.
fn seed_coastal_hike(
    e: &mut Engine,
    com: &str,
    enzo: &str,
    ana: &str,
    guia: &str,
    t0: i64,
) -> Result<(), CoreError> {
    let at = |m: i64| t0 - m * MIN;
    let hike_day = t(&t("Sábado", "Saturday"), "Saturday");
    let hike_title = t(
        &t("Trilha Praia do Sono", "Praia do Sono trail"),
        "Praia do Sono trail",
    );
    let hike_state = serde_json::json!({
        "title": hike_title,
        "area": t(&t("Paraty", "Paraty"), "Paraty"),
        "day": hike_day,
        "trails": [
            { "id": "tomales", "name": t(&t("Praia do Sono", "Praia do Sono"), "Praia do Sono"), "votes": ["Ana", "Enzo"] },
            { "id": "steep", "name": t(&t("Cachoeira da Pedra Branca", "Pedra Branca falls"), "Pedra Branca falls"), "votes": [] },
            { "id": "lands", "name": t(&t("Ponta Negra", "Ponta Negra"), "Ponta Negra"), "votes": [] }
        ],
        "decided": "tomales",
        "itinerary": [
            { "time": "8:00", "text": t(&t("Encontro no Cais", "Meet at the pier"), "Meet at the pier") },
            { "time": "9:40", "text": t(&t("Maré baixa · início da trilha", "Low tide · trail start"), "Low tide · trail start") },
            { "time": "12:30", "text": t(&t("Praia do Sono · almoço", "Praia do Sono · lunch"), "Praia do Sono · lunch") },
            { "time": "16:00", "text": t(&t("Volta", "Return"), "Return") }
        ],
        "album": [],
        "log": [
            { "who": "Guia", "what": t(&t("abriu a trilha", "opened the hike"), "opened the hike"), "at": at(60 * 8) },
            { "who": "Ana", "what": t(&t("fechou: Praia do Sono", "locked in Praia do Sono"), "locked in Praia do Sono"), "at": at(60 * 7) }
        ]
    });
    // Note: trail ids stay tomales/steep/lands (engine enum); display names above are local flavor in votes UI via name field.
    let hike = new_id("it");
    e.append_at(
        com,
        guia,
        at(60 * 8),
        EventBody::ItemCreated {
            item: hike.clone(),
            kind: ItemKind::App,
            content: ItemContent::App(AppDoc {
                app: "hike".into(),
                resource_uri: "ui://roda/hike".into(),
                title: hike_title.clone(),
                state_json: hike_state.to_string(),
            }),
            origin: t(
                &t("Guia · trilha da comunidade", "Guide · community trail"),
                "Guide · community trail",
            ),
        },
    )?;
    e.append_at(
        com,
        ana,
        at(60 * 8),
        grant(
            ana,
            &format!("app:{hike}"),
            GrantScope::Item(hike.clone()),
            Capability::Trust(TrustLevel::Act),
        ),
    )?;
    // Enzo is a member — grant act for him too so he can open/vote as himself
    e.append_at(
        com,
        enzo,
        at(60 * 8),
        grant(
            enzo,
            &format!("app:{hike}"),
            GrantScope::Item(hike.clone()),
            Capability::Trust(TrustLevel::Act),
        ),
    )?;
    e.append_at(
        com,
        guia,
        at(60 * 8),
        EventBody::MessagePosted {
            message: new_id("msg"),
            text: t(
                &t(
                    "Fechei a trilha da Praia do Sono pro sábado (maré boa). O placar fica pinado aqui no topo.",
                    "Locked in the Praia do Sono trail for Saturday (good tide). The scoreboard stays pinned at the top.",
                ),
                "Locked in the Praia do Sono trail for Saturday (good tide). The scoreboard stays pinned at the top.",
            ),
            attaches: Some(hike),
        },
    )?;
    Ok(())
}
