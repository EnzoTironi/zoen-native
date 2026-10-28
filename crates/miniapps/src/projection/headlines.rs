//! Per-app headline projections from the existing canonical state.
use super::{
    elapsed_millis, hike_name, maptap_board, schedule, t, thousands, tr, ts, view, visual_count,
    Value,
};
#[must_use]
pub fn headline(app: &str, state: &Value, now: i64) -> (String, Vec<(String, f64)>) {
    let v = view(app, state, now);
    match app {
        "pdfcraft" => {
            let pages = state["declaredPages"].as_u64().unwrap_or(0);
            let label = if pages == 1 {
                t("1 página", "1 page")
            } else {
                tr!("{} páginas", "{} pages", pages)
            };
            (label, vec![])
        }
        "wordcraft" => {
            let words = state["words"].as_u64().unwrap_or(0);
            let label = if words == 1 {
                t("1 palavra", "1 word")
            } else {
                tr!("{} palavras", "{} words", words)
            };
            (label, vec![])
        }
        "photocraft" => {
            let width = state["width"].as_u64().unwrap_or(0);
            let height = state["height"].as_u64().unwrap_or(0);
            (format!("{width} × {height}"), vec![])
        }
        "gridcraft" => {
            let sheets = state["sheets"].as_u64().unwrap_or(0);
            let label = if sheets == 1 {
                t("1 aba", "1 sheet")
            } else {
                tr!("{} abas", "{} sheets", sheets)
            };
            (label, vec![])
        }
        "deckcraft" => {
            let slides = state["slides"].as_u64().unwrap_or(0);
            let label = if slides == 1 {
                t("1 slide", "1 slide")
            } else {
                tr!("{} slides", "{} slides", slides)
            };
            (label, vec![])
        }
        "excalidraw" => {
            let count = state["elementCount"].as_u64().unwrap_or(0);
            let label = if count == 1 {
                t("1 elemento", "1 element")
            } else {
                tr!("{} elementos", "{} elements", count)
            };
            (label, vec![])
        }
        "pet" => (
            format!(
                "{} · {}",
                v["name"].as_str().unwrap_or(ts("O bichinho", "The pet")),
                v["mood"].as_str().unwrap_or("")
            ),
            vec![
                (
                    t("Comida", "Food"),
                    v["fullness"].as_f64().unwrap_or(0.0) / 100.0,
                ),
                (t("Humor", "Mood"), v["joy"].as_f64().unwrap_or(0.0) / 100.0),
                (
                    t("Descanso", "Rest"),
                    v["energy"].as_f64().unwrap_or(0.0) / 100.0,
                ),
            ],
        ),
        "maptap" => headline_maptap(&v),
        "recipe" => headline_recipe(&v),
        "poll" => headline_poll(&v),
        "list" => headline_list(&v),
        "hike" => headline_hike(&v),

        "schedule" => headline_schedule(&v),

        "countdown" => {
            let left = elapsed_millis(v["target"].as_i64().unwrap_or(now), now);
            let days = left / 86_400_000;
            (
                tr!(
                    "{} · faltam {days} dias",
                    "{} · {days} days to go",
                    v["title"].as_str().unwrap_or("")
                ),
                vec![],
            )
        }
        _ => (String::new(), vec![]),
    }
}

fn headline_recipe(v: &Value) -> (String, Vec<(String, f64)>) {
    let ings = v["ingredients"].as_array().cloned().unwrap_or_default();
    let done = ings
        .iter()
        .filter(|i| i["done"].as_bool().unwrap_or(false))
        .count();
    let frac = if ings.is_empty() {
        0.0
    } else {
        visual_count(done) / visual_count(ings.len())
    };
    (
        tr!(
            "{} · {} porções",
            "{} · {} servings",
            v["title"].as_str().unwrap_or(""),
            v["servings"].as_i64().unwrap_or(2)
        ),
        vec![(t("Ingredientes", "Ingredients"), frac)],
    )
}

fn headline_poll(v: &Value) -> (String, Vec<(String, f64)>) {
    let opts = v["options"].as_array().cloned().unwrap_or_default();
    let total: usize = opts
        .iter()
        .map(|o| o["votes"].as_array().map_or(0, Vec::len))
        .sum();
    let mut metrics: Vec<(String, f64)> = opts
        .iter()
        .take(3)
        .map(|o| {
            let n = o["votes"].as_array().map_or(0, Vec::len);
            (
                o["text"].as_str().unwrap_or("").to_string(),
                if total == 0 {
                    0.0
                } else {
                    visual_count(n) / visual_count(total)
                },
            )
        })
        .collect();
    metrics.truncate(3);
    let line = if v["closed"].as_bool().unwrap_or(false) {
        tr!(
            "Decidido: {}",
            "Decided: {}",
            v["winner"].as_str().unwrap_or("—")
        )
    } else {
        tr!(
            "{total} voto{} · {}",
            "{total} vote{} · {}",
            if total == 1 { "" } else { "s" },
            v["question"].as_str().unwrap_or("")
        )
    };
    (line, metrics)
}

fn headline_list(v: &Value) -> (String, Vec<(String, f64)>) {
    let items = v["items"].as_array().cloned().unwrap_or_default();
    let done = items
        .iter()
        .filter(|i| i["done"].as_bool().unwrap_or(false))
        .count();
    let frac = if items.is_empty() {
        0.0
    } else {
        visual_count(done) / visual_count(items.len())
    };
    (
        tr!(
            "{done} de {} marcados · {}",
            "{done} of {} checked · {}",
            items.len(),
            v["title"].as_str().unwrap_or("")
        ),
        vec![(t("Feito", "Done"), frac)],
    )
}

fn headline_hike(v: &Value) -> (String, Vec<(String, f64)>) {
    let trails = v["trails"].as_array().cloned().unwrap_or_default();
    let total: usize = trails
        .iter()
        .map(|t| t["votes"].as_array().map_or(0, Vec::len))
        .sum();
    let metrics = trails
        .iter()
        .map(|t| {
            let n = t["votes"].as_array().map_or(0, Vec::len);
            (
                t["name"].as_str().unwrap_or("").to_string(),
                if total == 0 {
                    0.0
                } else {
                    visual_count(n) / visual_count(total)
                },
            )
        })
        .collect();
    let line = match v["decided"].as_str().and_then(hike_name) {
        Some(n) => tr!("Vamos de {n}", "Going: {n}"),
        None => tr!(
            "3 trilhas · {total} voto{}",
            "3 trails · {total} vote{}",
            if total == 1 { "" } else { "s" }
        ),
    };
    (line, metrics)
}

fn headline_maptap(v: &Value) -> (String, Vec<(String, f64)>) {
    let board = maptap_board(v);
    let line = match board.first() {
        Some((who, pts, n)) if *n > 0 => tr!(
            "{who} lidera com {} pts",
            "{who} leads with {} pts",
            thousands(*pts)
        ),
        _ => t("5 lugares · um toque cada", "5 places · one tap each"),
    };
    (line, vec![])
}

fn headline_schedule(v: &Value) -> (String, Vec<(String, f64)>) {
    let open = schedule::pending(v);
    let line = if open > 0 {
        tr!(
            "{open} pedido{} aguardando",
            "{open} request{} waiting",
            if open == 1 { "" } else { "s" }
        )
    } else {
        tr!(
            "{} · marque um horário",
            "{} · book a slot",
            v["business"]
                .as_str()
                .unwrap_or(v["title"].as_str().unwrap_or(""))
        )
    };
    (line, vec![])
}
