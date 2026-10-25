use super::{msg, Cast, MIN};
use crate::engine::{Engine, R};
use crate::i18n::t;
use types::new_id;
use types::{Annotation, EventBody, Privacy, Role, SpaceKind};
pub(super) fn seed(e: &mut Engine, cast: &Cast, t0: i64) -> R<(String, u64, u64)> {
    let Cast { enzo, marina, .. } = cast;
    let at = |minutes_ago: i64| t0 - minutes_ago * MIN;
    // ── Marina (DM) ──
    let dm_marina = new_id("sp")?;
    e.append_at(
        &dm_marina,
        enzo,
        at(60 * 24 * 5),
        EventBody::SpaceCreated {
            title: "Marina".into(),
            kind: SpaceKind::Direct,
            privacy: Privacy::EndToEnd,
        },
    )?;
    e.append_at(
        &dm_marina,
        enzo,
        at(60 * 24 * 5),
        EventBody::MemberAdded {
            identity: marina.clone(),
            role: Role::Member,
        },
    )?;
    let sunscreen = e.append_at(
        &dm_marina,
        enzo,
        at(52),
        msg(&t("Comprei o protetor solar ☀️", "Bought the sunscreen ☀️"))?,
    )?;
    let charger = e.append_at(
        &dm_marina,
        marina,
        at(44),
        msg(&t(
            "Perfeito! Leva o carregador portátil também?",
            "Perfect! Can you bring the power bank too?",
        ))?,
    )?;
    let marina_delivered = e.append_at(
        &dm_marina,
        enzo,
        at(43),
        msg(&t(
            "Já deixei o carregador na mochila 🔋",
            "The power bank is in my bag 🔋",
        ))?,
    )?;
    e.append_at(
        &dm_marina,
        enzo,
        at(42),
        msg(&t(
            "Saio às oito. Te aviso quando chegar 🙂",
            "Leaving at eight. I'll text when I get there 🙂",
        ))?,
    )?;
    for (target, who, emoji) in [
        (&sunscreen.client_id, &marina, "❤️"),
        (&charger.client_id, &enzo, "👍"),
    ] {
        e.append_at(
            &dm_marina,
            who,
            at(41),
            EventBody::MessageAnnotated {
                target: target.clone(),
                note: Annotation::React {
                    emoji: emoji.into(),
                },
            },
        )?;
    }

    Ok((dm_marina, marina_delivered.seq, sunscreen.seq))
}
