//! Built-in geography tool actions.

use super::*;

pub(super) fn maptap_guess(
    args: &Value,
    mut st: Value,
    actor: &str,
    now: i64,
) -> Result<Applied, String> {
    let round = args["round"]
        .as_u64()
        .ok_or_else(|| t("qual rodada?", "which round?"))? as usize;
    let (lat, lon) = (
        args["lat"].as_f64().ok_or("latitude?")?,
        args["lon"].as_f64().ok_or("longitude?")?,
    );
    let place = st["places"]
        .get(round)
        .cloned()
        .ok_or_else(|| t("rodada não existe", "no such round"))?;
    if !st["guesses"][actor][round.to_string()].is_null() {
        return Err(t(
            "você já cravou esta rodada",
            "you already locked in this round",
        ));
    }
    let km = haversine_km(
        lat,
        lon,
        place["lat"].as_f64().unwrap_or(0.0),
        place["lon"].as_f64().unwrap_or(0.0),
    );
    let points = (5000.0 * (-km / 1500.0).exp()).round() as i64;
    if st["guesses"][actor].is_null() {
        st["guesses"][actor] = json!({});
        st["order"]
            .as_array_mut()
            .ok_or_else(|| t("estado inválido", "invalid state"))?
            .push(json!(actor));
    }
    st["guesses"][actor][round.to_string()] =
        json!({ "lat": lat, "lon": lon, "km": km.round(), "points": points });
    let pname = place["name"].as_str().unwrap_or("?").to_string();
    let mut a;
    let n = st["guesses"][actor]
        .as_object()
        .map(|m| m.len())
        .unwrap_or(0);
    let total: i64 = st["guesses"][actor]
        .as_object()
        .map(|m| m.values().filter_map(|g| g["points"].as_i64()).sum())
        .unwrap_or(0);
    push_log(
        &mut st,
        actor,
        &tr!(
            "chutou {pname} a {} km",
            "guessed {pname} {} km off",
            km.round() as i64
        ),
        now,
    );
    a = Applied::new(
        st,
        tr!(
            "{actor} · MapTap rodada {} · {} km",
            "{actor} · MapTap round {} · {} km",
            round + 1,
            km.round() as i64
        ),
        tr!(
            "{} km de {pname} · {points} pts",
            "{} km from {pname} · {points} pts",
            km.round() as i64
        ),
    );
    if n == 5 {
        a.comment = Some(tr!(
            "{actor} fechou o MapTap com {} pts. Próximo!",
            "{actor} finished MapTap with {} pts. Who’s next?",
            thousands(total)
        ));
    }
    Ok(a)
}
