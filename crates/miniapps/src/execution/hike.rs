//! Built-in hike tool actions.

use super::*;

pub(super) fn hike_vote(
    args: &Value,
    mut st: Value,
    actor: &str,
    now: i64,
) -> Result<Applied, String> {
    if st["decided"].as_str().is_some() {
        return Err(t(
            "a escolha já foi fechada",
            "the choice is already locked in",
        ));
    }
    let trail = s(args, "trail").ok_or_else(|| t("qual trilha?", "which trail?"))?;
    let name = hike_name(&trail).ok_or_else(|| t("trilha desconhecida", "unknown trail"))?;
    for tr_ in st["trails"].as_array_mut().into_iter().flatten() {
        let votes = tr_["votes"]
            .as_array_mut()
            .map(std::mem::take)
            .unwrap_or_default();
        let mut votes: Vec<Value> = votes
            .into_iter()
            .filter(|v| v.as_str() != Some(actor))
            .collect();
        if tr_["id"].as_str() == Some(trail.as_str()) {
            votes.push(json!(actor));
        }
        tr_["votes"] = Value::Array(votes);
    }
    push_log(
        &mut st,
        actor,
        &tr!("votou em {name}", "voted for {name}"),
        now,
    );
    Ok(Applied::new(
        st,
        tr!("{actor} votou em {name}", "{actor} voted for {name}"),
        tr!("Voto: {name}.", "Voted: {name}."),
    ))
}

pub(super) fn hike_decide(mut st: Value, actor: &str, now: i64) -> Result<Applied, String> {
    if st["decided"].as_str().is_some() {
        return Err(t(
            "a escolha já foi fechada",
            "the choice is already locked in",
        ));
    }
    let trails = st["trails"].as_array().cloned().unwrap_or_default();
    let best = trails
        .iter()
        .max_by_key(|t| t["votes"].as_array().map(|a| a.len()).unwrap_or(0))
        .ok_or_else(|| t("sem trilhas", "no trails"))?;
    if best["votes"]
        .as_array()
        .map(|a| a.is_empty())
        .unwrap_or(true)
    {
        return Err(t("ninguém votou ainda", "nobody has voted yet"));
    }
    let id = best["id"].as_str().unwrap_or("").to_string();
    let name = hike_name(&id).unwrap_or("").to_string();
    st["decided"] = json!(id);
    push_log(
        &mut st,
        actor,
        &tr!("fechou: {name}", "locked in {name}"),
        now,
    );
    let mut a = Applied::new(
        st,
        tr!(
            "{actor} fechou a trilha · {name}",
            "{actor} locked in the hike · {name}"
        ),
        tr!("Fechado: {name}.", "Locked in: {name}."),
    );
    a.comment = Some(tr!(
        "Fechado: {name}. Quem tem carro?",
        "Locked in: {name}. Who has a car?"
    ));
    Ok(a)
}

pub(super) fn hike_set_itinerary(
    args: &Value,
    mut st: Value,
    actor: &str,
    now: i64,
) -> Result<Applied, String> {
    let trail = st["decided"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| t("escolham a trilha primeiro", "pick the trail first"))?;
    let driver = s(args, "driver").ok_or_else(|| t("quem dirige?", "who drives?"))?;
    let (_, name, drive_min, hike_h) = HIKE_TRAILS
        .iter()
        .find(|t| t.0 == trail)
        .copied()
        .ok_or_else(|| t("trilha desconhecida", "unknown trail"))?;
    let mut lines = vec![
        json!({ "time": "8:00", "text": tr!("{driver} · carro pronto", "{driver} · car ready") }),
    ];
    let mut minute = 8 * 60;
    for p in args["pickups"].as_array().into_iter().flatten().take(4) {
        let names: Vec<String> = p["names"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|n| {
                n.as_str()
                    .map(|x| x.trim().chars().take(20).collect::<String>())
            })
            .filter(|n| !n.is_empty())
            .collect();
        if names.is_empty() {
            continue;
        }
        minute += 15;
        let place = p["place"]
            .as_str()
            .unwrap_or("")
            .trim()
            .chars()
            .take(24)
            .collect::<String>();
        let who = names.join(" + ");
        lines.push(json!({ "time": clock(minute), "text": if place.is_empty() { who } else { format!("{who} · {place}") } }));
    }
    minute += drive_min;
    lines.push(json!({ "time": clock(minute), "text": tr!("Começo da trilha {name}", "{name} trailhead") }));
    minute += hike_h * 60;
    lines.push(json!({ "time": clock(minute), "text": t("Volta de carro", "Drive back") }));
    let text = lines
        .iter()
        .map(|l| {
            format!(
                "{} {}",
                l["time"].as_str().unwrap_or(""),
                l["text"].as_str().unwrap_or("")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    st["itinerary"] = Value::Array(lines);
    push_log(
        &mut st,
        actor,
        &t("montou o roteiro", "planned the day"),
        now,
    );
    let mut a = Applied::new(
        st,
        tr!("{actor} montou o roteiro", "{actor} planned the day"),
        t("Roteiro pronto.", "Plan ready."),
    );
    a.comment = Some(format!(
        "{}\n{text}",
        tr!("Fica assim no {}:", "Here's {}:", st_day(&a.state))
    ));
    Ok(a)
}

pub(super) fn hike_add_photos(
    args: &Value,
    mut st: Value,
    actor: &str,
    now: i64,
) -> Result<Applied, String> {
    let photos: Vec<String> = args["photos"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| p.as_str().map(str::to_string))
        .collect();
    if photos.is_empty() {
        return Err(t("nenhuma foto", "no photos"));
    }
    for p in &photos {
        if !p.starts_with("data:image/jpeg;base64,") || p.len() > 160_000 {
            return Err(t(
                "foto inválida (o app só aceita JPEG reduzido)",
                "invalid photo (only downscaled JPEG is accepted)",
            ));
        }
    }
    let album = st["album"]
        .as_array_mut()
        .ok_or_else(|| t("estado inválido", "invalid state"))?;
    if album.len() + photos.len() > 12 {
        return Err(t(
            "o álbum está cheio (12 fotos)",
            "the album is full (12 photos)",
        ));
    }
    let n = photos.len();
    for p in photos {
        album.push(json!({ "by": actor, "dataUrl": p }));
    }
    push_log(
        &mut st,
        actor,
        &tr!(
            "pôs {n} foto(s) no álbum",
            "added {n} photo(s) to the album"
        ),
        now,
    );
    Ok(Applied::new(
        st,
        tr!(
            "{actor} pôs {n} foto(s) no álbum",
            "{actor} added {n} photo(s) to the album"
        ),
        tr!("{n} foto(s) no álbum.", "{n} photo(s) in the album."),
    ))
}
