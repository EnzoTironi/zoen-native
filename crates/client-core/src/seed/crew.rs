use super::{grant, msg, Cast, MIN};
use crate::engine::{Engine, R};
use crate::i18n::t;
use types::new_id;
use types::{
    Annotation, Capability, EventBody, GrantScope, Privacy, ReplyRef, Role, SpaceKind, TrustLevel,
};
pub(super) fn seed(e: &mut Engine, cast: &Cast, t0: i64) -> R<(String, u64, u64)> {
    let Cast {
        enzo,
        marina,
        lucas,
        ana,
        zoen,
        ..
    } = cast;
    let at = |minutes_ago: i64| t0 - minutes_ago * MIN;
    // ── Turma do Sábado (grupo de amigos: onde moram os mini-apps do Wabi) ──
    let turma = new_id("sp")?;
    e.append_at(
        &turma,
        enzo,
        at(60 * 24 * 4),
        EventBody::SpaceCreated {
            title: t(&t("Turma do Sábado", "Saturday Crew"), "Saturday Crew"),
            kind: SpaceKind::Group,
            privacy: Privacy::EndToEnd,
        },
    )?;
    for m in [marina, lucas, ana, zoen] {
        e.append_at(
            &turma,
            enzo,
            at(60 * 24 * 4),
            EventBody::MemberAdded {
                identity: m.clone(),
                role: Role::Member,
            },
        )?;
    }
    e.append_at(
        &turma,
        enzo,
        at(60 * 24 * 4),
        grant(
            enzo,
            zoen,
            GrantScope::Space(turma.clone()),
            Capability::Trust(TrustLevel::Act),
        )?,
    )?;
    let joke = e.append_at(
        &turma,
        lucas,
        at(52),
        msg(&t(
            "Vocês ainda acham que Liubliana fica na Eslováquia? 😂",
            "Do you guys still think Ljubljana is in Slovakia? 😂",
        ))?,
    )?;
    let group_read = e.append_at(
        &turma,
        enzo,
        at(51),
        msg(&t(
            "Eu levo água e uns lanches 🥾",
            "I'll bring water and snacks 🥾",
        ))?,
    )?;
    e.append_at(
        &turma,
        marina,
        at(50),
        EventBody::MessagePosted {
            message: new_id("msg")?,
            text: t("Levo as frutas. Fechado! 🍊", "I'll bring fruit. Deal! 🍊"),
            attaches: None,
            reply: Some(ReplyRef::thread(group_read.client_id.clone())),
            media: None,
            forwarded: false,
        },
    )?;
    e.append_at(
        &turma,
        ana,
        at(50),
        msg(&t("Foi UMA vez", "That was ONE time"))?,
    )?;
    e.append_at(
        &turma,
        marina,
        at(48),
        msg(&t(
            "Esse grupo precisa de um mascote",
            "This group needs a mascot",
        ))?,
    )?;
    let group_delivered = e.append_at(
        &turma,
        enzo,
        at(47),
        msg(&t(
            "A Paçoca já faz parte da turma 😂",
            "Paçoca is already part of the crew 😂",
        ))?,
    )?;

    // Live mini-apps pinned in the chat (and on Home): hike, pet, MapTap.
    // These are real Items in the Space — ChatPinStrip / HomeStrip read them from the core.
    super::crew_apps::seed(e, &turma, cast, t0)?;
    e.append_at(
        &turma,
        enzo,
        at(37),
        msg(&t(
            "Sábado às oito na praça?",
            "Saturday at eight in the square?",
        ))?,
    )?;
    for (target, who, emoji) in [
        (&joke.client_id, &enzo, "😂"),
        (&joke.client_id, &ana, "😂"),
        (&group_read.client_id, &marina, "👍"),
    ] {
        e.append_at(
            &turma,
            who,
            at(36),
            EventBody::MessageAnnotated {
                target: target.clone(),
                note: Annotation::React {
                    emoji: emoji.into(),
                },
            },
        )?;
    }

    Ok((turma, group_delivered.seq, group_read.seq))
}
