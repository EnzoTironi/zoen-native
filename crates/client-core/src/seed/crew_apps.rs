use super::{grant, msg, Cast, MIN};
use crate::apps;
use crate::engine::{Engine, R};
use crate::i18n::t;
use crate::CoreError;
use types::new_id;
use types::{AppDoc, Capability, EventBody, GrantScope, ItemContent, ItemKind, TrustLevel};
/// Saturday Crew: hike (locked + ride), pet Paçoca, MapTap — feeds Home strip + chat tiles.
pub(super) fn seed(e: &mut Engine, turma: &str, cast: &Cast, t0: i64) -> R<()> {
    let Cast {
        enzo,
        marina,
        lucas,
        ana,
        zoen,
        ..
    } = cast;
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
    let hike = new_id("it")?;
    e.append_at(
        turma,
        zoen,
        at(45),
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
        )?,
    )?;
    e.append_at(
        turma,
        zoen,
        at(45),
        EventBody::MessagePosted {
            message: new_id("msg")?,
            text: t(
                &t(
                    "Separei três trilhas boas pra sábado, com mapa e fotos. Comparem e votem: quando fechar, eu monto o roteiro com as caronas.",
                    "Found three good trails for Saturday, with maps and photos. Compare and vote: once it’s locked in, I’ll plan the day and the rides.",
                ),
                "Found three good trails for Saturday, with maps and photos. Compare and vote: once it’s locked in, I’ll plan the day and the rides.",
            ),
            attaches: Some(hike.clone()),
            reply: None,
            media: None,
            forwarded: false,
        },
    )?;
    e.append_at(
        turma,
        marina,
        at(44),
        msg(&t("Pedra Grande!! vista demais 🌄", "Tomales!! the elk 🦌"))?,
    )?;
    e.append_at(
        turma,
        lucas,
        at(43),
        msg(&t(
            "Tô dentro. Eu dirijo, cabem mais 3 no carro",
            "I’m in. I can drive, room for 3 in the car",
        ))?,
    )?;
    e.append_at(
        turma,
        ana,
        at(42),
        msg(&t(
            "tô dentro! mas a Steep Ravine tem sombra…",
            "in! Steep Ravine has shade though…",
        ))?,
    )?;
    e.append_at(
        turma,
        zoen,
        at(41),
        msg(&t(
            "Fechado: Pedra Grande. Roteiro com o carro do Lucas pronto — pega o Enzo em Pinheiros e Ana + Marina na Vila Madalena.",
            "Locked in: Tomales Point. Lucas drives — picks up Enzo in Noe Valley, Ana and Marina in the Mission.",
        ))?,
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
    let pet = new_id("it")?;
    e.append_at(
        turma,
        zoen,
        at(47),
        EventBody::ItemCreated {
            item: pet.clone(),
            kind: ItemKind::App,
            content: ItemContent::App(AppDoc {
                document: None,
                app: "pet".into(),
                resource_uri: "ui://builtin/pet".into(),
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
        )?,
    )?;
    e.append_at(
        turma,
        zoen,
        at(47),
        EventBody::MessagePosted {
            message: new_id("msg")?,
            text: t(
                &t(
                    "Adotei um jumento pro grupo. Cuidem dele!",
                    "I adopted a donkey for the group. Take care of him!",
                ),
                "I adopted a donkey for the group. Take care of him!",
            ),
            attaches: Some(pet.clone()),
            reply: None,
            media: None,
            forwarded: false,
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
    let maptap = new_id("it")?;
    e.append_at(
        turma,
        zoen,
        at(38),
        EventBody::ItemCreated {
            item: maptap.clone(),
            kind: ItemKind::App,
            content: ItemContent::App(AppDoc {
                document: None,
                app: "maptap".into(),
                resource_uri: "ui://builtin/maptap".into(),
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
        )?,
    )?;
    e.append_at(
        turma,
        zoen,
        at(38),
        EventBody::MessagePosted {
            message: new_id("msg")?,
            text: t(
                &t(
                    "MapTap no grupo — 5 lugares, quem chega mais perto.",
                    "MapTap for the group — 5 places, closest guess wins.",
                ),
                "MapTap for the group — 5 places, closest guess wins.",
            ),
            attaches: Some(maptap),
            reply: None,
            media: None,
            forwarded: false,
        },
    )?;

    let _ = (marina, lucas, ana); // used in hike votes / messages
    Ok(())
}
