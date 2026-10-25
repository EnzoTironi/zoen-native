use super::{msg, Cast, MIN};
use crate::engine::{Engine, R};
use crate::i18n::t;
use types::new_id;
use types::{EventBody, ItemContent, ItemKind, Privacy, Role, SpaceKind};
pub(super) fn seed(e: &mut Engine, cast: &Cast, t0: i64) -> R<String> {
    let Cast {
        enzo,
        lucas,
        ana,
        zoen,
        ..
    } = cast;
    let at = |minutes_ago: i64| t0 - minutes_ago * MIN;
    // ── Zoen · Produto ──
    let team = new_id("sp")?;
    e.append_at(
        &team,
        enzo,
        at(60 * 24 * 2),
        EventBody::SpaceCreated {
            title: t(&t("Zoen · Produto", "Zoen · Product"), "Zoen · Product"),
            kind: SpaceKind::Group,
            privacy: Privacy::EndToEnd,
        },
    )?;
    for m in [lucas, ana, zoen] {
        e.append_at(
            &team,
            enzo,
            at(60 * 24 * 2),
            EventBody::MemberAdded {
                identity: m.clone(),
                role: Role::Member,
            },
        )?;
    }
    e.append_at(
        &team,
        lucas,
        at(60 * 6),
        msg(&t(
            "Subi o build novo no TestFlight. A cápsula de vidro encolhendo ao rolar ficou linda.",
            "New build is up on TestFlight. The glass capsule shrinking on scroll looks gorgeous.",
        ))?,
    )?;
    e.append_at(
        &team,
        ana,
        at(60 * 5 + 40),
        msg(&t(
            "O cartão de pedido no escuro tá impecável. Só falta o ícone do Mac.",
            "The request card in dark mode is flawless. Only the Mac icon is missing.",
        ))?,
    )?;
    let summary = new_id("it")?;
    e.append_at(&team, zoen, at(60 * 5), EventBody::ItemCreated {
        item: summary.clone(),
        kind: ItemKind::Note,
        content: ItemContent::Text { text: t(&t("Resumo do dia\n• Build novo no TestFlight (Lucas)\n• Cartão de pedido aprovado no escuro (Ana)\n• Pendente: ícone do app no Mac", "Daily summary\n• New build on TestFlight (Lucas)\n• Request card approved in dark mode (Ana)\n• Pending: the Mac app icon"), "Daily summary\n• New build on TestFlight (Lucas)\n• Request card approved in dark mode (Ana)\n• Pending: the Mac app icon") },
        origin: t(&t("Zoen · resumo da conversa", "Zoen · conversation summary"), "Zoen · conversation summary"),
    })?;
    e.append_at(
        &team,
        zoen,
        at(60 * 5),
        EventBody::MessagePosted {
            message: new_id("msg")?,
            text: t(
                &t(
                    "Resumo do dia: 2 entregas, 1 pendência (ícone do Mac).",
                    "Daily summary: 2 shipped, 1 pending (Mac icon).",
                ),
                "Daily summary: 2 shipped, 1 pending (Mac icon).",
            ),
            attaches: Some(summary),
            reply: None,
            media: None,
            forwarded: false,
        },
    )?;
    e.append_at(
        &team,
        zoen,
        at(60 * 5),
        EventBody::UsageRecorded {
            agent: zoen.clone(),
            cents: 140,
            what: t(&t("resumo", "summary"), "summary"),
        },
    )?;

    Ok(team)
}
