//! Built-in geography tool actions.

use super::{haversine_km, json, push_log, t, thousands, tr, Applied, Value};
use num_traits::ToPrimitive;

pub(super) fn maptap_guess(
    args: &Value,
    mut st: Value,
    actor: &str,
    now: i64,
) -> Result<Applied, String> {
    let round = args["round"]
        .as_u64()
        .and_then(|round| usize::try_from(round).ok())
        .ok_or_else(|| t("qual rodada?", "which round?"))?;
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
    let points = (5000.0 * (-km / 1500.0).exp())
        .round()
        .to_i64()
        .ok_or_else(|| t("pontuação inválida", "invalid score"))?;
    let rounded_km = km
        .round()
        .to_i64()
        .ok_or_else(|| t("distância inválida", "invalid distance"))?;
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
        .map_or(0, serde_json::Map::len);
    let total: i64 = st["guesses"][actor]
        .as_object()
        .map_or(0, |m| m.values().filter_map(|g| g["points"].as_i64()).sum());
    push_log(
        &mut st,
        actor,
        &tr!(
            "chutou {pname} a {} km",
            "guessed {pname} {} km off",
            rounded_km
        ),
        now,
    );
    a = Applied::new(
        st,
        tr!(
            "{actor} · MapTap rodada {} · {} km",
            "{actor} · MapTap round {} · {} km",
            round + 1,
            rounded_km
        ),
        tr!(
            "{} km de {pname} · {points} pts",
            "{} km from {pname} · {points} pts",
            rounded_km
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
