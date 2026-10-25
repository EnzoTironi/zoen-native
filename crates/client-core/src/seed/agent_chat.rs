use super::{msg, Cast, MIN};
use crate::engine::{Engine, R};
use crate::i18n::t;
use types::new_id;
use types::{EventBody, ItemContent, ItemKind, Privacy, Role, SpaceKind};
pub(super) fn seed(e: &mut Engine, cast: &Cast, t0: i64) -> R<String> {
    let Cast { enzo, zoen, .. } = cast;
    let at = |minutes_ago: i64| t0 - minutes_ago * MIN;
    // ── Zoen (DM com o meu agente: onde o momento mágico acontece) ──
    let dm_zoen = new_id("sp")?;
    e.append_at(
        &dm_zoen,
        enzo,
        at(60 * 26),
        EventBody::SpaceCreated {
            title: "Zoen".into(),
            kind: SpaceKind::Direct,
            privacy: Privacy::EndToEnd,
        },
    )?;
    e.append_at(
        &dm_zoen,
        enzo,
        at(60 * 26),
        EventBody::MemberAdded {
            identity: zoen.clone(),
            role: Role::Member,
        },
    )?;
    e.append_at(
        &dm_zoen,
        zoen,
        at(60 * 26),
        msg(&t(
            "Oi, Enzo. O que você quer resolver?",
            "Hi, Enzo. What do you want to get done?",
        ))?,
    )?;
    e.append_at(
        &dm_zoen,
        enzo,
        at(60 * 25),
        msg(&t(
            "Me lembra de renovar o passaporte antes de dezembro",
            "Remind me to renew my passport before December",
        ))?,
    )?;
    let task = new_id("it")?;
    e.append_at(&dm_zoen, zoen, at(60 * 25), EventBody::ItemCreated {
        item: task.clone(),
        kind: ItemKind::Task,
        content: ItemContent::Text { text: t(&t("Renovar o passaporte\nAgendar na PF até 15/11 · levar RG e passaporte antigo", "Renew passport\nBook an appointment by Nov 15 · bring ID and the old passport"), "Renew passport\nBook an appointment by Nov 15 · bring ID and the old passport") },
        origin: t(&t("Zoen · a partir de “Me lembra de renovar o passaporte”", "Zoen · from “Remind me to renew my passport”"), "Zoen · from “Remind me to renew my passport”"),
    })?;
    e.append_at(
        &dm_zoen,
        zoen,
        at(60 * 25),
        EventBody::MessagePosted {
            message: new_id("msg")?,
            text: t(
                &t(
                    "Anotado. Criei a tarefa e te lembro na segunda.",
                    "Got it. I created the task and I'll remind you on Monday.",
                ),
                "Got it. I created the task and I'll remind you on Monday.",
            ),
            attaches: Some(task),
            reply: None,
            media: None,
            forwarded: false,
        },
    )?;
    e.append_at(
        &dm_zoen,
        zoen,
        at(60 * 25),
        EventBody::UsageRecorded {
            agent: zoen.clone(),
            cents: 170,
            what: t(&t("tarefa", "task"), "task"),
        },
    )?;

    Ok(dm_zoen)
}
