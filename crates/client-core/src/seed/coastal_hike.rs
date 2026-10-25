use super::{grant, Cast, MIN};
use crate::engine::{Engine, R};
use crate::i18n::t;
use types::new_id;
use types::{AppDoc, Capability, EventBody, GrantScope, ItemContent, ItemKind, TrustLevel};
/// Coastal Travelers: trail app pinned for the community trip vibe.
pub(super) fn seed(e: &mut Engine, com: &str, cast: &Cast, t0: i64) -> R<()> {
    let Cast {
        enzo, ana, guia, ..
    } = cast;
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
    let hike = new_id("it")?;
    e.append_at(
        com,
        guia,
        at(60 * 8),
        EventBody::ItemCreated {
            item: hike.clone(),
            kind: ItemKind::App,
            content: ItemContent::App(AppDoc {
                document: None,
                app: "hike".into(),
                resource_uri: "ui://builtin/hike".into(),
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
        )?,
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
        )?,
    )?;
    e.append_at(
        com,
        guia,
        at(60 * 8),
        EventBody::MessagePosted {
            message: new_id("msg")?,
            text: t(
                &t(
                    "Fechei a trilha da Praia do Sono pro sábado (maré boa). O placar fica pinado aqui no topo.",
                    "Locked in the Praia do Sono trail for Saturday (good tide). The scoreboard stays pinned at the top.",
                ),
                "Locked in the Praia do Sono trail for Saturday (good tide). The scoreboard stays pinned at the top.",
            ),
            attaches: Some(hike),
            reply: None,
            media: None,
            forwarded: false,
        },
    )?;
    Ok(())
}
