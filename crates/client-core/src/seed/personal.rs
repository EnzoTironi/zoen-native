use super::{grant, Cast, MIN};
use crate::engine::{Engine, R};
use crate::i18n::t;
use types::new_id;
use types::{Capability, EventBody, GrantScope, Privacy, SpaceKind};
pub(super) fn seed(e: &mut Engine, cast: &Cast, t0: i64) -> R<()> {
    let Cast {
        enzo, zoen, fin, ..
    } = cast;
    let at = |minutes_ago: i64| t0 - minutes_ago * MIN;
    // ── Pessoal (oculto): orçamentos dos meus agentes (plano Plus: R$ 15 de IA) ──
    let me_space = new_id("sp")?;
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
        )?,
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
        )?,
    )?;

    Ok(())
}
