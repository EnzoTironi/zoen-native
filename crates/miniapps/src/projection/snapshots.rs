//! Per-app snapshot projections from the existing canonical state.
use super::headline;
use super::{hike_name, maptap_board, schedule, t, thousands, tr, view, visual_count, Value};
use serde_json::json;
#[must_use]
pub fn snapshot(app: &str, state: &Value, item_id: &str, now: i64) -> Option<Value> {
    let v = view(app, state, now);
    let link = format!("zoen://app/{item_id}");
    let snap = match app {
        "pdfcraft" => {
            json!({"template":"caption","title":state["title"],"eyebrow":"PDF","detail":headline(app,state,now).0,"accentHex":"#A55553","symbol":"doc.richtext"})
        }
        "wordcraft" => {
            json!({"template":"caption","title":state["title"],"eyebrow":t("Documento", "Document"),"detail":headline(app,state,now).0,"accentHex":"#667C99","symbol":"doc.text"})
        }
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
        "hike" => snapshot_hike(&v),
        "countdown" => json!({
            "template": "countdown", "title": v["place"].as_str().unwrap_or(v["title"].as_str().unwrap_or("")),
            "eyebrow": v["title"], "targetMs": v["target"], "accentHex": "#3D7A28", "symbol": "airplane", "art": "trip"
        }),
        "list" => snapshot_list(&v),
        "poll" => {
            let opts = v["options"].as_array().cloned().unwrap_or_default();
            let total: usize = opts
                .iter()
                .map(|o| o["votes"].as_array().map_or(0, Vec::len))
                .sum();
            let bars: Vec<Value> = opts.iter().take(3).map(|o| {
                let n = o["votes"].as_array().map_or(0, Vec::len);
                json!({ "label": o["text"].as_str().unwrap_or("").chars().take(16).collect::<String>(), "value": if total == 0 { 0.0 } else { visual_count(n) / visual_count(total) } })
            }).collect();
            json!({ "template": "progress", "title": v["question"], "detail": v["winner"], "bars": bars, "accentHex": "#5B8DEF", "symbol": "chart.bar.fill", "art": "ballot" })
        }
        "recipe" => snapshot_recipe(&v),
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

fn snapshot_hike(v: &Value) -> Value {
    let trails = v["trails"].as_array().cloned().unwrap_or_default();
    let total: usize = trails
        .iter()
        .map(|t| t["votes"].as_array().map_or(0, Vec::len))
        .sum();
    if let Some(id) = v["decided"].as_str() {
        // Prefer the localized trail name from state (Pedra Grande / Praia do Sono / Tomales Point).
        let name = trails
            .iter()
            .find(|t| t["id"].as_str() == Some(id))
            .and_then(|t| t["name"].as_str())
            .map(str::to_string)
            .or_else(|| hike_name(id).map(str::to_string))
            .unwrap_or_else(|| id.to_string());
        let first = v["itinerary"].as_array().and_then(|a| a.first()).map(|l| {
            format!(
                "{} {}",
                l["time"].as_str().unwrap_or(""),
                l["text"].as_str().unwrap_or("")
            )
        });
        json!({ "template": "photo", "title": name, "eyebrow": v["day"], "detail": first.unwrap_or_else(|| v["day"].as_str().unwrap_or("").to_string()), "accentHex": "#3D7A28", "symbol": "figure.hiking", "photo": format!("hike-{id}") })
    } else {
        let bars: Vec<Value> = trails.iter().map(|t| { let n = t["votes"].as_array().map_or(0, Vec::len); json!({ "label": t["name"].as_str().unwrap_or("").chars().take(16).collect::<String>(), "value": if total == 0 { 0.0 } else { visual_count(n) / visual_count(total) } }) }).collect();
        json!({ "template": "progress", "title": v["title"], "bars": bars, "accentHex": "#3D7A28", "symbol": "figure.hiking", "art": "hike" })
    }
}

fn snapshot_recipe(v: &Value) -> Value {
    let ings = v["ingredients"].as_array().cloned().unwrap_or_default();
    let done = ings
        .iter()
        .filter(|i| i["done"].as_bool().unwrap_or(false))
        .count();
    json!({ "template": "stat", "title": v["title"], "value": format!("{done}/{}", ings.len()), "detail": tr!("{} porções", "{} servings", v["servings"].as_i64().unwrap_or(2)), "accentHex": "#E0533E", "symbol": "fork.knife", "art": "pot" })
}

fn snapshot_list(v: &Value) -> Value {
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
