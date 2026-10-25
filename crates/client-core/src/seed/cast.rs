use super::{agent, person};
use crate::{
    engine::{Engine, R},
    i18n::t,
};
pub(super) struct Cast {
    pub(super) enzo: String,
    pub(super) marina: String,
    pub(super) lucas: String,
    pub(super) ana: String,
    pub(super) zoen: String,
    pub(super) fin: String,
    pub(super) org: String,
    pub(super) guia: String,
}
impl Cast {
    pub(super) fn create(e: &mut Engine) -> R<Self> {
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

        Ok(Self {
            enzo,
            marina,
            lucas,
            ana,
            zoen,
            fin,
            org,
            guia,
        })
    }
}
