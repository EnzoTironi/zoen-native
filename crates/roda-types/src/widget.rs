//! Pure native widget projection shared with browser clients. Locale and time are explicit.
use serde_json::{json, Value};
fn pick(english: bool, pt: &'static str, en: &'static str) -> &'static str {
    if english {
        en
    } else {
        pt
    }
}
fn clamp(x: f64) -> f64 {
    x.clamp(0.0, 100.0)
}
fn pending(st: &Value) -> usize {
    st["bookings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|b| b["status"] == "pending")
        .count()
}
fn accent(st: &Value) -> &str {
    st["accentHex"].as_str().unwrap_or("#3D7A28")
}
fn thousands(n: i64, english: bool) -> String {
    let s = n.abs().to_string();
    let sep = if english { ',' } else { '.' };
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(sep);
        }
        out.push(ch);
    }
    if n < 0 {
        format!("-{out}")
    } else {
        out
    }
}

pub fn view(app: &str, state: &Value, now: i64, english: bool) -> Value {
    let mut v = state.clone();
    if app == "pet" && !state["released"].as_bool().unwrap_or(false) {
        let h = ((now - state["updated"].as_i64().unwrap_or(now)).max(0) as f64) / 3_600_000.0;
        let asleep = state["asleep"].as_bool().unwrap_or(false);
        v["fullness"] = json!(clamp(state["fullness"].as_f64().unwrap_or(50.0) - 6.0 * h).round());
        v["energy"] = json!(clamp(
            state["energy"].as_f64().unwrap_or(50.0) + if asleep { 12.0 * h } else { -4.0 * h }
        )
        .round());
        v["joy"] = json!(clamp(state["joy"].as_f64().unwrap_or(50.0) - 5.0 * h).round());
        v["mood"] = json!(pet_mood(&v, english));
    }
    v
}
pub fn pet_mood(v: &Value, english: bool) -> &'static str {
    if v["released"].as_bool().unwrap_or(false) {
        return pick(english, "Foi embora", "Gone");
    }
    if v["asleep"].as_bool().unwrap_or(false) {
        return pick(english, "Dormindo", "Sleeping");
    }
    let (f, e, j) = (
        v["fullness"].as_f64().unwrap_or(0.0),
        v["energy"].as_f64().unwrap_or(0.0),
        v["joy"].as_f64().unwrap_or(0.0),
    );
    if f < 25.0 {
        pick(english, "Com fome", "Hungry")
    } else if e < 25.0 {
        pick(english, "Cansado", "Tired")
    } else if j < 30.0 {
        pick(english, "Entediado", "Bored")
    } else if f > 85.0 && j > 85.0 {
        pick(english, "Radiante", "Thrilled")
    } else {
        pick(english, "Feliz aqui", "Happy here")
    }
}
pub fn snapshot(app: &str, state: &Value, item_id: &str, now: i64, english: bool) -> Option<Value> {
    let t = |pt, en| pick(english, pt, en).to_string();
    macro_rules! tr { ($pt:literal, $en:literal $(, $arg:expr)* $(,)?) => { if english { format!($en $(, $arg)*) } else { format!($pt $(, $arg)*) } }; }
    let v = view(app, state, now, english);
    let link = format!("zoen://app/{item_id}");
    let snap = match app {
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
                        .or_else(|| hike_name(id, english).map(|s| s.to_string()))
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
                Some((who, pts, n)) if *n > 0 => (
                    tr!("{who} na frente", "{who} leads"),
                    thousands(*pts, english),
                ),
                _ => (t("5 lugares", "5 places"), "0".to_string()),
            };
            json!({ "template": "stat", "title": title, "value": value, "accentHex": "#1B2A4A", "symbol": "globe.americas.fill", "art": "globe" })
        }

        "schedule" => {
            let open = pending(&v);
            let accent = accent(&v);
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
pub fn maptap_board(v: &Value) -> Vec<(String, i64, usize)> {
    let mut b: Vec<(String, i64, usize)> = v["guesses"]
        .as_object()
        .map(|m| {
            m.iter()
                .map(|(who, g)| {
                    let rounds = g.as_object().map(|r| r.len()).unwrap_or(0);
                    let pts = g
                        .as_object()
                        .map(|r| r.values().filter_map(|x| x["points"].as_i64()).sum())
                        .unwrap_or(0);
                    (who.clone(), pts, rounds)
                })
                .collect()
        })
        .unwrap_or_default();
    b.sort_by_key(|row| std::cmp::Reverse(row.1));
    b
}
pub fn hike_name(id: &str, english: bool) -> Option<&'static str> {
    // English ids stay stable; labels follow the core language.
    match id {
        "tomales" => Some(if english {
            "Tomales Point"
        } else {
            "Praia do Sono"
        }),
        "steep" => Some(if english {
            "Steep Ravine"
        } else {
            "Pedra Grande"
        }),
        "lands" => Some(if english {
            "Lands End"
        } else {
            "Pico do Jaraguá"
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pet_clock_and_locale_are_explicit() {
        let state = json!({"name":"Peanut","updated":0,"fullness":90,"energy":90,"joy":90});
        let en = snapshot("pet", &state, "pet-id", 3_600_000, true).unwrap();
        assert_eq!(en["bars"][0]["value"], 0.84);
        assert_eq!(en["detail"], "Happy here");
        assert_eq!(en["deepLink"], "zoen://app/pet-id");
        assert_eq!(
            snapshot("pet", &state, "pet-id", 0, false).unwrap()["detail"],
            "Radiante"
        );
        assert_eq!(state["fullness"], 90);
    }
    #[test]
    fn unsupported_apps_have_no_fabricated_snapshot() {
        assert!(snapshot("unknown", &json!({}), "id", 0, true).is_none());
    }
    #[test]
    fn schedule_snapshot_exposes_counts_not_private_names() {
        let snapshot = snapshot("schedule", &json!({"business":"Clinic","bookings":[{"status":"pending","name":"Private patient","notes":"Private notes"}],"accentHex":"#123456"}), "id", 0, true).unwrap();
        assert_eq!(snapshot["value"], "1");
        assert_eq!(snapshot["accentHex"], "#123456");
        assert!(!snapshot.to_string().contains("Private"));
    }
}
