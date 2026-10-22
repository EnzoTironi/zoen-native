//! Built-in recipe tool actions.

use super::*;

pub(super) fn recipe_servings(
    args: &Value,
    mut st: Value,
    actor: &str,
    now: i64,
) -> Result<Applied, String> {
    let n = args["servings"]
        .as_i64()
        .ok_or_else(|| t("quantas porções?", "how many servings?"))?
        .clamp(1, 12);
    st["servings"] = json!(n);
    push_log(
        &mut st,
        actor,
        &tr!("ajustou para {n} porções", "set it to {n} servings"),
        now,
    );
    Ok(Applied::new(
        st,
        tr!(
            "{actor} ajustou para {n} porções",
            "{actor} set it to {n} servings"
        ),
        tr!("{n} porções.", "{n} servings."),
    ))
}

pub(super) fn recipe_check(
    args: &Value,
    mut st: Value,
    actor: &str,
    now: i64,
) -> Result<Applied, String> {
    let id = s(args, "id").ok_or_else(|| t("qual ingrediente?", "which ingredient?"))?;
    let ing = st["ingredients"]
        .as_array_mut()
        .into_iter()
        .flatten()
        .find(|i| i["id"].as_str() == Some(id.as_str()))
        .ok_or_else(|| t("ingrediente não existe", "no such ingredient"))?;
    let done = !ing["done"].as_bool().unwrap_or(false);
    ing["done"] = json!(done);
    let text = ing["name"].as_str().unwrap_or("").to_string();
    let verb = if done {
        ts("separou", "got")
    } else {
        ts("desmarcou", "unchecked")
    };
    push_log(&mut st, actor, &format!("{verb} {text}"), now);
    Ok(Applied::new(st, format!("{actor} {verb} {text}"), text))
}

pub(super) fn recipe_cook(
    args: &Value,
    mut st: Value,
    actor: &str,
    now: i64,
) -> Result<Applied, String> {
    let step = args["step"].as_i64().unwrap_or(0).max(0);
    let total = st["steps"].as_array().map(|a| a.len() as i64).unwrap_or(0);
    st["cooking"] = json!({ "who": actor, "step": step, "at": now });
    let what = if step >= total {
        t("terminou de cozinhar", "finished cooking")
    } else {
        tr!(
            "está no passo {} de {total}",
            "is on step {} of {total}",
            step + 1
        )
    };
    push_log(&mut st, actor, &what, now);
    let mut a = Applied::new(st, format!("{actor} {what}"), what.clone());
    if step >= total {
        a.comment = Some(t(
            "Jantar pronto! Bom apetite, gente.",
            "Dinner’s ready! Enjoy, everyone.",
        ));
    }
    Ok(a)
}
