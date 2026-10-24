use super::{grant, line, msg, Cast, MIN};
use crate::engine::{Engine, R};
use crate::i18n::t;
use types::new_id;
use types::{
    ActionClass, AppDoc, Capability, EventBody, GrantScope, ItemContent, ItemKind, PlanDoc,
    PlanSection, Privacy, Role, SpaceKind, TrustLevel,
};
pub(super) fn seed(e: &mut Engine, cast: &Cast, t0: i64) -> R<()> {
    let Cast {
        enzo,
        marina,
        zoen,
        fin,
        org,
        ..
    } = cast;
    let at = |minutes_ago: i64| t0 - minutes_ago * MIN;
    // ── Paraty com a Marina (o cenário da demo) ──
    let paraty = new_id("sp")?;
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
        )?,
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
        )?,
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
        )?,
    )?;
    e.append_at(
        &paraty,
        &marina,
        at(190),
        msg(&t(
            "Feriado chegando… bora pra Paraty? 🌊",
            "Long weekend coming up… Paraty? 🌊",
        ))?,
    )?;
    e.append_at(
        &paraty,
        &enzo,
        at(188),
        msg(&t(
            "Bora! @Zoen planeja nosso fim de semana em Paraty, até R$ 1.500",
            "Let's go! @Zoen plan our weekend in Paraty, up to $1,500",
        ))?,
    )?;

    let pousada = line(
        &t(
            "Pousada Casa Azul · 2 noites, café incluso",
            "Casa Azul Inn · 2 nights, breakfast included",
        ),
        640,
    )?;
    let onibus = line(
        &t(
            "Ônibus Rio → Paraty, ida e volta (2 pessoas)",
            "Bus Rio → Paraty, round trip (2 people)",
        ),
        248,
    )?;
    let jantar_v1 = line(
        &t(
            "Jantar no centro histórico",
            "Dinner in the historic center",
        ),
        220,
    )?;
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
                    )?,
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
                    )?,
                    line(&t("Trilha da Praia do Sono", "Praia do Sono trail"), 0)?,
                ],
            },
            PlanSection {
                title: t(&t("Comida", "Food"), "Food"),
                lines: vec![
                    jantar_v1,
                    line(&t("Cafés e lanches", "Coffee and snacks"), 0)?,
                ],
            },
        ],
    };
    let plan_item = new_id("it")?;
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
    e.append_at(&paraty, &zoen, at(187), EventBody::MessagePosted { message: new_id("msg")?, text: t(&t("Montei o plano: ônibus, 2 noites na Casa Azul e escuna. R$ 1.348 de R$ 1.500 · sobra R$ 152.", "Here's the plan: bus, 2 nights at Casa Azul and a schooner. $1,348 of $1,500 · $152 left."), "Here's the plan: bus, 2 nights at Casa Azul and a schooner. $1,348 of $1,500 · $152 left."), attaches: Some(plan_item.clone()), reply: None, media: None, forwarded: false, })?;
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
    let trip = new_id("it")?;
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
                document: None,
                app: "countdown".into(),
                resource_uri: "ui://builtin/countdown".into(),
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
        )?,
    )?;
    e.append_at(
        &paraty,
        &zoen,
        at(186),
        EventBody::MessagePosted {
            message: new_id("msg")?,
            text: t(
                &t(
                    "E pus uma contagem para a viagem.",
                    "And I added a countdown for the trip.",
                ),
                "And I added a countdown for the trip.",
            ),
            attaches: Some(trip.clone()),
            reply: None,
            media: None,
            forwarded: false,
        },
    )?;

    e.append_at(
        &paraty,
        &marina,
        at(175),
        msg(&t(
            "Amei! Troca o jantar por aquele do centro histórico que tem moqueca?",
            "Love it! Can you swap dinner for that place in the historic center with the moqueca?",
        ))?,
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
        ))?,
    )?;

    e.append_at(&paraty, &fin, at(160), msg(&t("Dividi meio a meio: R$ 694 pra cada. Deixei as reservas prontas em Atividade pra você aprovar.", "Split it 50/50: $694 each. The bookings are ready in Activity for you to approve."))?)?;
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
        ))?,
    )?;
    e.append_at(
        &paraty,
        &marina,
        at(18),
        msg(&t(
            "@Enzo já aprovou a pousada? Eles seguram a reserva só até hoje 😅",
            "@Enzo did you approve the inn yet? They only hold the booking until today 😅",
        ))?,
    )?;

    Ok(())
}
