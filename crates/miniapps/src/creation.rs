//! Initial canonical state for each built-in app.

use super::{
    deckcraft, is_en, json, maptap_places, push_log, recipe_default, s, schedule, t, tr, ts, Actor,
    Value, HIKE_TRAILS,
};

/// # Errors
/// Refuses invalid, unsupported or oversized initialization arguments before producing canonical state.
pub fn create(app: &str, args: &Value, actor: &str, now: i64) -> Result<(String, Value), String> {
    match app {
        "deckcraft" => deckcraft::registration::create(args),
        "gridcraft" => crate::binary::gridcraft_registration::create(args),
        "wordcraft" => crate::binary::wordcraft_registration::create(args),
        "photocraft" => crate::binary::photocraft_registration::create(args),
        "pdfcraft" => crate::binary::pdfcraft_registration::create(args),
        "excalidraw" => create_drawing(args),
        "pet" => Ok(create_pet(args, actor, now)),
        "maptap" => {
            let places = maptap_places(now)?;
            let st = json!({ "places": places, "guesses": {}, "order": [], "log": [] });
            Ok(("MapTap".into(), st))
        }
        "recipe" => {
            let mut st = recipe_default();
            if let Some(n) = args["servings"].as_i64() {
                st["servings"] = json!(n.clamp(1, 12));
            }
            push_log(
                &mut st,
                actor,
                &t("salvou a receita", "saved the recipe"),
                now,
            );
            let title = st["title"].as_str().unwrap_or("").to_string();
            Ok((title, st))
        }
        "poll" => create_poll(args, actor, now),
        "list" => {
            let title = s(args, "title").unwrap_or_else(|| t("O que levar", "What to bring"));
            let items: Vec<Value> = args["items"].as_array().map(|a| a.iter().filter_map(|x| x.as_str()).enumerate().map(|(i, t)| json!({ "id": format!("l{}", i + 1), "text": t, "done": false, "by": null })).collect()).unwrap_or_default();
            let mut st = json!({ "title": title, "items": items, "next": 100, "log": [] });
            push_log(&mut st, actor, &t("criou a lista", "created the list"), now);
            Ok((title, st))
        }
        "hike" => {
            let day = s(args, "day").unwrap_or_else(|| t("Sábado", "Saturday"));
            let area = s(args, "area").unwrap_or_else(|| "Bay Area".into());
            let title = tr!("Trilha de {}", "{} hike", day.to_lowercase());
            let title = if is_en() {
                format!("{day} hike")
            } else {
                title
            };
            let trails: Vec<Value> = HIKE_TRAILS
                .iter()
                .map(|(id, name, _, _)| json!({ "id": id, "name": name, "votes": [] }))
                .collect();
            let mut st = json!({ "title": title, "area": area, "day": day, "trails": trails, "decided": null, "itinerary": null, "album": [], "log": [] });
            push_log(&mut st, actor, &t("abriu a trilha", "opened the hike"), now);
            Ok((title, st))
        }

        "schedule" => schedule::create(
            args,
            &Actor {
                id: actor,
                name: actor,
                owner: None,
            },
            now,
        ),
        "countdown" => {
            let title = s(args, "title").ok_or_else(|| {
                t(
                    "a contagem precisa de um título",
                    "the countdown needs a title",
                )
            })?;
            let target = args["target_ms"].as_i64().ok_or_else(|| {
                t(
                    "a contagem precisa de uma data",
                    "the countdown needs a date",
                )
            })?;
            let mut st = json!({ "title": title.chars().take(32).collect::<String>(), "place": s(args, "place"), "target": target, "log": [] });
            push_log(
                &mut st,
                actor,
                &t("criou a contagem", "started the countdown"),
                now,
            );
            Ok((title, st))
        }
        _ => Err(tr!("app desconhecido: {app}", "unknown app: {app}")),
    }
}

fn create_pet(args: &Value, actor: &str, now: i64) -> (String, Value) {
    let name =
        s(args, "name").map_or_else(|| t("Jumento", "Donkey"), |n| n.chars().take(18).collect());
    let mut st = json!({
        "name": name, "species": "jumento",
        "fullness": 72.0, "energy": 72.0, "joy": 76.0,
        "asleep": false, "released": false,
        "updated": now, "born": now, "log": [],
        "dash": { "best": {}, "runs": 0 }
    });
    push_log(&mut st, actor, &t("adotou", "adopted"), now);
    (name, st)
}

fn create_poll(args: &Value, actor: &str, now: i64) -> Result<(String, Value), String> {
    let question = s(args, "question").ok_or_else(|| {
        t(
            "a enquete precisa de uma pergunta",
            "the poll needs a question",
        )
    })?;
    let opts: Vec<String> = args["options"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(|t| t.trim().to_string()))
                .filter(|t| !t.is_empty())
                .collect()
        })
        .unwrap_or_default();
    if opts.len() < 2 {
        return Err(t(
            "a enquete precisa de pelo menos duas opções",
            "the poll needs at least two options",
        ));
    }
    let options: Vec<Value> = opts
        .iter()
        .enumerate()
        .map(|(i, t)| json!({ "id": format!("o{}", i + 1), "text": t, "votes": [] }))
        .collect();
    let mut st = json!({ "question": question, "options": options, "closed": false, "winner": null, "log": [] });
    push_log(
        &mut st,
        actor,
        &t("abriu a enquete", "opened the poll"),
        now,
    );
    Ok((question, st))
}

fn create_drawing(args: &Value) -> Result<(String, Value), String> {
    let title = args
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or(ts("Desenho", "Drawing"))
        .trim();
    if args.get("title").is_some_and(|v| !v.is_string())
        || title.is_empty()
        || title.len() > 640
        || title.chars().count() > 160
        || title.contains('\0')
        || args
            .as_object()
            .is_none_or(|o| o.keys().any(|k| k != "title"))
    {
        return Err(t("título de desenho inválido", "invalid drawing title"));
    }
    Ok((
        title.into(),
        json!({"title":title,"elementCount":0,"textCount":0}),
    ))
}
