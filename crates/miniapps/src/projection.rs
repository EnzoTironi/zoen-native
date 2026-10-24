//! Time-derived state, card headlines, and portable snapshots.

use super::{clamp, hike_name, maptap_board, schedule, t, thousands, tr, ts, Value};
use serde_json::json;
use std::time::Duration;

#[cfg(test)]
mod tests;

/// A rollback contributes no elapsed time; `abs_diff` preserves the full signed range.
fn elapsed_millis(later: i64, earlier: i64) -> u64 {
    if later > earlier {
        later.abs_diff(earlier)
    } else {
        0
    }
}

// ── leitura (com o tempo passando, no caso do bichinho) ──

/// O estado como a interface deve ver agora (o bichinho sente fome com o tempo).
#[must_use]
pub fn view(app: &str, state: &Value, now: i64) -> Value {
    let mut v = state.clone();
    if app == "pet" && !state["released"].as_bool().unwrap_or(false) {
        let elapsed = elapsed_millis(now, state["updated"].as_i64().unwrap_or(now));
        let h = Duration::from_millis(elapsed).as_secs_f64() / 3_600.0;
        let asleep = state["asleep"].as_bool().unwrap_or(false);
        v["fullness"] = json!(clamp(state["fullness"].as_f64().unwrap_or(50.0) - 6.0 * h).round());
        v["energy"] = json!(clamp(
            state["energy"].as_f64().unwrap_or(50.0) + if asleep { 12.0 * h } else { -4.0 * h }
        )
        .round());
        v["joy"] = json!(clamp(state["joy"].as_f64().unwrap_or(50.0) - 5.0 * h).round());
        v["mood"] = json!(pet_mood(&v));
    }
    v
}

pub(super) fn pet_mood(v: &Value) -> &'static str {
    if v["released"].as_bool().unwrap_or(false) {
        return ts("Foi embora", "Gone");
    }
    if v["asleep"].as_bool().unwrap_or(false) {
        return ts("Dormindo", "Sleeping");
    }
    let (f, e, j) = (
        v["fullness"].as_f64().unwrap_or(0.0),
        v["energy"].as_f64().unwrap_or(0.0),
        v["joy"].as_f64().unwrap_or(0.0),
    );
    if f < 25.0 {
        ts("Com fome", "Hungry")
    } else if e < 25.0 {
        ts("Cansado", "Tired")
    } else if j < 30.0 {
        ts("Entediado", "Bored")
    } else if f > 85.0 && j > 85.0 {
        ts("Radiante", "Thrilled")
    } else {
        ts("Feliz aqui", "Happy here")
    }
}

/// Linha e métricas para o cartão vivo da tela inicial.
#[must_use]
pub fn headline(app: &str, state: &Value, now: i64) -> (String, Vec<(String, f64)>) {
    let v = view(app, state, now);
    match app {
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
        "maptap" => {
            let board = maptap_board(&v);
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
        "recipe" => {
            let ings = v["ingredients"].as_array().cloned().unwrap_or_default();
            let done = ings
                .iter()
                .filter(|i| i["done"].as_bool().unwrap_or(false))
                .count();
            let frac = if ings.is_empty() {
                0.0
            } else {
                done as f64 / ings.len() as f64
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
        "poll" => {
            let opts = v["options"].as_array().cloned().unwrap_or_default();
            let total: usize = opts
                .iter()
                .map(|o| o["votes"].as_array().map(|a| a.len()).unwrap_or(0))
                .sum();
            let mut metrics: Vec<(String, f64)> = opts
                .iter()
                .take(3)
                .map(|o| {
                    let n = o["votes"].as_array().map(|a| a.len()).unwrap_or(0);
                    (
                        o["text"].as_str().unwrap_or("").to_string(),
                        if total == 0 {
                            0.0
                        } else {
                            n as f64 / total as f64
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
        "list" => {
            let items = v["items"].as_array().cloned().unwrap_or_default();
            let done = items
                .iter()
                .filter(|i| i["done"].as_bool().unwrap_or(false))
                .count();
            let frac = if items.is_empty() {
                0.0
            } else {
                done as f64 / items.len() as f64
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
        "hike" => {
            let trails = v["trails"].as_array().cloned().unwrap_or_default();
            let total: usize = trails
                .iter()
                .map(|t| t["votes"].as_array().map(|a| a.len()).unwrap_or(0))
                .sum();
            let metrics = trails
                .iter()
                .map(|t| {
                    let n = t["votes"].as_array().map(|a| a.len()).unwrap_or(0);
                    (
                        t["name"].as_str().unwrap_or("").to_string(),
                        if total == 0 {
                            0.0
                        } else {
                            n as f64 / total as f64
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

        "schedule" => {
            let open = schedule::pending(&v);
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

/// The mini-app's widget snapshot (one of a few native templates; see `WidgetSnapshot` in
/// the Apple app). Content only, no labels about the app itself. `None` = no widget.
pub fn snapshot(app: &str, state: &Value, item_id: &str, now: i64) -> Option<Value> {
    let v = view(app, state, now);
    let link = format!("zoen://app/{item_id}");
    let snap = match app {
        "photocraft" => {
            json!({"template":"caption","title":state["title"],"eyebrow":t("Imagem", "Image"),"detail":headline(app,state,now).0,"accentHex":"#447B83","symbol":"photo"})
        }
        "gridcraft" => {
            json!({"template":"caption","title":state["title"],"eyebrow":t("Planilha", "Spreadsheet"),"detail":headline(app,state,now).0,"accentHex":"#3F7B63","symbol":"tablecells"})
        }
        "deckcraft" => {
            json!({"template":"caption","title":state["title"],"eyebrow":t("Apresentação", "Presentation"),"detail":headline(app,state,now).0,"accentHex":"#9B6E51","symbol":"rectangle.stack","art":"slides"})
        }
        "excalidraw" => {
            json!({"template":"caption","title":state["title"],"eyebrow":"Excalidraw","detail":headline(app,state,now).0,"accentHex":"#6965DB","symbol":"pencil.and.outline","art":"drawing"})
        }
        "pet" => {
            let asleep = v["asleep"].as_bool().unwrap_or(false);
            let gone = v["released"].as_bool().unwrap_or(false);
            json!({
                "template": "progress", "title": v["name"], "detail": v["mood"],
                "bars": [
                    { "label": t("Comida", "Food"), "value": v["fullness"].as_f64().unwrap_or(0.0) / 100.0 },
                    { "label": t("Humor", "Mood"), "value": v["joy"].as_f64().unwrap_or(0.0) / 100.0 },
                    { "label": t("Descanso", "Rest"), "value": v["energy"].as_f64().unwrap_or(0.0) / 100.0 }
                ],
                "accentHex": "#E8833A", "symbol": "pawprint.fill",
                "art": if gone { "pet.gone" } else if asleep { "pet.asleep" } else { "pet" },
                "actions": if gone { json!([]) } else if asleep { json!([{ "tool": "pet_wake", "label": t("Acordar", "Wake") }]) } else { json!([{ "tool": "pet_feed", "label": t("Comer", "Feed") }, { "tool": "pet_nap", "label": t("Cochilar", "Nap") }]) }
            })
        }
        "hike" => {
            let trails = v["trails"].as_array().cloned().unwrap_or_default();
            let total: usize = trails
                .iter()
                .map(|t| t["votes"].as_array().map(|a| a.len()).unwrap_or(0))
                .sum();
            match v["decided"].as_str() {
                Some(id) => {
                    // Prefer the localized trail name from state (Pedra Grande / Praia do Sono / Tomales Point).
                    let name = trails
                        .iter()
                        .find(|t| t["id"].as_str() == Some(id))
                        .and_then(|t| t["name"].as_str())
                        .map(|s| s.to_string())
                        .or_else(|| hike_name(id).map(|s| s.to_string()))
                        .unwrap_or_else(|| id.to_string());
                    let first = v["itinerary"].as_array().and_then(|a| a.first()).map(|l| {
                        format!(
                            "{} {}",
                            l["time"].as_str().unwrap_or(""),
                            l["text"].as_str().unwrap_or("")
                        )
                    });
                    json!({ "template": "photo", "title": name, "eyebrow": v["day"], "detail": first.unwrap_or_else(|| v["day"].as_str().unwrap_or("").to_string()), "accentHex": "#3D7A28", "symbol": "figure.hiking", "photo": format!("hike-{id}") })
                }
                None => {
                    let bars: Vec<Value> = trails.iter().map(|t| { let n = t["votes"].as_array().map(|a| a.len()).unwrap_or(0); json!({ "label": t["name"].as_str().unwrap_or("").chars().take(16).collect::<String>(), "value": if total == 0 { 0.0 } else { n as f64 / total as f64 } }) }).collect();
                    json!({ "template": "progress", "title": v["title"], "bars": bars, "accentHex": "#3D7A28", "symbol": "figure.hiking", "art": "hike" })
                }
            }
        }
        "countdown" => json!({
            "template": "countdown", "title": v["place"].as_str().unwrap_or(v["title"].as_str().unwrap_or("")),
            "eyebrow": v["title"], "targetMs": v["target"], "accentHex": "#3D7A28", "symbol": "airplane", "art": "trip"
        }),
        "list" => {
            let items = v["items"].as_array().cloned().unwrap_or_default();
            let done = items
                .iter()
                .filter(|i| i["done"].as_bool().unwrap_or(false))
                .count();
            let rows: Vec<Value> = items
                .iter()
                .take(4)
                .map(|i| json!({ "text": i["text"], "done": i["done"].as_bool().unwrap_or(false) }))
                .collect();
            json!({ "template": "list", "title": v["title"], "value": format!("{done}/{}", items.len()), "rows": rows, "accentHex": "#3FB27F", "symbol": "checklist", "art": "notepad", "sensitive": true })
        }
        "poll" => {
            let opts = v["options"].as_array().cloned().unwrap_or_default();
            let total: usize = opts
                .iter()
                .map(|o| o["votes"].as_array().map(|a| a.len()).unwrap_or(0))
                .sum();
            let bars: Vec<Value> = opts.iter().take(3).map(|o| {
                let n = o["votes"].as_array().map(|a| a.len()).unwrap_or(0);
                json!({ "label": o["text"].as_str().unwrap_or("").chars().take(16).collect::<String>(), "value": if total == 0 { 0.0 } else { n as f64 / total as f64 } })
            }).collect();
            json!({ "template": "progress", "title": v["question"], "detail": v["winner"], "bars": bars, "accentHex": "#5B8DEF", "symbol": "chart.bar.fill", "art": "ballot" })
        }
        "recipe" => {
            let ings = v["ingredients"].as_array().cloned().unwrap_or_default();
            let done = ings
                .iter()
                .filter(|i| i["done"].as_bool().unwrap_or(false))
                .count();
            json!({ "template": "stat", "title": v["title"], "value": format!("{done}/{}", ings.len()), "detail": tr!("{} porções", "{} servings", v["servings"].as_i64().unwrap_or(2)), "accentHex": "#E0533E", "symbol": "fork.knife", "art": "pot" })
        }
        "maptap" => {
            let board = maptap_board(&v);
            let (title, value) = match board.first() {
                Some((who, pts, n)) if *n > 0 => {
                    (tr!("{who} na frente", "{who} leads"), thousands(*pts))
                }
                _ => (t("5 lugares", "5 places"), "0".to_string()),
            };
            json!({ "template": "stat", "title": title, "value": value, "accentHex": "#1B2A4A", "symbol": "globe.americas.fill", "art": "globe" })
        }

        "schedule" => {
            let open = schedule::pending(&v);
            let accent = schedule::accent(&v);
            json!({
                "template": "stat",
                "title": v["business"].as_str().unwrap_or(v["title"].as_str().unwrap_or("")),
                "value": format!("{open}"),
                "detail": if open == 0 {
                    t("Horários livres", "Free slots")
                } else {
                    t("Pedidos abertos", "Open requests")
                },
                "accentHex": accent,
                "symbol": "calendar",
                "art": "notepad"
            })
        }

        _ => return None,
    };
    let mut snap = snap;
    snap["id"] = json!(item_id);
    snap["appId"] = json!(app);
    snap["deepLink"] = json!(link);
    Some(snap)
}

pub fn last_action(state: &Value) -> Option<String> {
    let first = state["log"].as_array()?.first()?;
    Some(format!(
        "{} {}",
        first["who"].as_str()?,
        first["what"].as_str()?
    ))
}
