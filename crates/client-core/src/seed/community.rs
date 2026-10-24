use super::{grant, msg, Cast, MIN};
use crate::engine::{Engine, R};
use crate::i18n::t;
use types::new_id;
use types::{Capability, EventBody, GrantScope, Privacy, Role, SpaceKind, TrustLevel};
pub(super) fn seed(e: &mut Engine, cast: &Cast, t0: i64) -> R<String> {
    let Cast {
        enzo, ana, guia, ..
    } = cast;
    let at = |minutes_ago: i64| t0 - minutes_ago * MIN;
    // ── Viajantes do Litoral (comunidade fechada da Ana) ──
    let com = new_id("sp")?;
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
        )?,
    )?;
    e.append_at(
        &com,
        &ana,
        at(60 * 24 * 2),
        msg(&t(
            "Bem-vindos! Aqui a gente troca roteiro de praia, trilha e pousada boa e barata.",
            "Welcome! This is where we swap beach itineraries, trails and good cheap inns.",
        ))?,
    )?;
    e.append_at(&com, &guia, at(60 * 9), msg(&t("Maré baixa no sábado às 9h40 — melhor horário pra chegar na Praia do Sono pela trilha.", "Low tide on Saturday at 9:40 am — the best time to hike to Praia do Sono."))?)?;

    // Trail mini-app pinned in the community (investor showcase: trip-group feel).
    super::coastal_hike::seed(e, &com, cast, t0)?;

    Ok(com)
}
