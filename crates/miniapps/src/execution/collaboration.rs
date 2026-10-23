//! Built-in collaboration tool actions.

use super::*;

pub(super) fn poll_vote(
    args: &Value,
    mut st: Value,
    actor: &str,
    now: i64,
) -> Result<Applied, String> {
    if st["closed"].as_bool().unwrap_or(false) {
        return Err(t("a enquete já fechou", "the poll is already closed"));
    }
    let opt = s(args, "option").ok_or_else(|| t("qual opção?", "which option?"))?;
    let mut chosen = None;
    for o in st["options"].as_array_mut().into_iter().flatten() {
        let is_it = o["id"].as_str() == Some(opt.as_str());
        let text = o["text"].as_str().map(String::from);
        if let Some(votes) = o["votes"].as_array_mut() {
            votes.retain(|v| v.as_str() != Some(actor));
            if is_it {
                votes.push(json!(actor));
                chosen = text;
            }
        }
    }
    let chosen = chosen.ok_or_else(|| t("opção não existe", "no such option"))?;
    push_log(
        &mut st,
        actor,
        &tr!("votou em {chosen}", "voted for {chosen}"),
        now,
    );
    Ok(Applied::new(
        st,
        tr!("{actor} votou em {chosen}", "{actor} voted for {chosen}"),
        tr!("Voto registrado: {chosen}.", "Vote recorded: {chosen}."),
    ))
}

pub(super) fn poll_add_option(
    args: &Value,
    mut st: Value,
    actor: &str,
    now: i64,
) -> Result<Applied, String> {
    let text = s(args, "text").ok_or_else(|| t("escreva a opção", "type the option"))?;
    let n = st["options"].as_array().map(|a| a.len()).unwrap_or(0) + 1;
    st["options"]
        .as_array_mut()
        .ok_or_else(|| t("estado inválido", "invalid state"))?
        .push(json!({ "id": format!("o{n}"), "text": text, "votes": [] }));
    push_log(
        &mut st,
        actor,
        &tr!("sugeriu {text}", "suggested {text}"),
        now,
    );
    Ok(Applied::new(
        st,
        tr!("{actor} sugeriu {text}", "{actor} suggested {text}"),
        tr!("Opção adicionada: {text}.", "Option added: {text}."),
    ))
}

pub(super) fn poll_close(mut st: Value, actor: &str, now: i64) -> Result<Applied, String> {
    if st["closed"].as_bool().unwrap_or(false) {
        return Err(t("a enquete já fechou", "the poll is already closed"));
    }
    let opts = st["options"].as_array().cloned().unwrap_or_default();
    let best = opts
        .iter()
        .max_by_key(|o| o["votes"].as_array().map(|a| a.len()).unwrap_or(0))
        .ok_or_else(|| t("sem opções", "no options"))?;
    if best["votes"]
        .as_array()
        .map(|a| a.is_empty())
        .unwrap_or(true)
    {
        return Err(t("ninguém votou ainda", "nobody has voted yet"));
    }
    let winner = best["text"].as_str().unwrap_or("").to_string();
    st["closed"] = json!(true);
    st["winner"] = json!(winner);
    push_log(
        &mut st,
        actor,
        &tr!("fechou: {winner}", "closed it: {winner}"),
        now,
    );
    let mut a = Applied::new(
        st,
        tr!(
            "{actor} fechou a enquete · {winner}",
            "{actor} closed the poll · {winner}"
        ),
        tr!(
            "Fechada: {winner}. Entrou no plano.",
            "Closed: {winner}. Added to the plan."
        ),
    );
    a.effect = Some(Effect::AddPlanLine {
        text: tr!(
            "{winner} (decidido na enquete)",
            "{winner} (decided in the poll)"
        ),
        cost_cents: 0,
    });
    a.comment = Some(tr!(
        "Decidido: {winner}. Já pus no plano.",
        "Decided: {winner}. It’s in the plan."
    ));
    Ok(a)
}

pub(super) fn list_add(
    args: &Value,
    mut st: Value,
    actor: &str,
    now: i64,
) -> Result<Applied, String> {
    let text = s(args, "text").ok_or_else(|| t("escreva o item", "type the item"))?;
    let next = st["next"].as_i64().unwrap_or(100);
    st["next"] = json!(next + 1);
    st["items"]
        .as_array_mut()
        .ok_or_else(|| t("estado inválido", "invalid state"))?
        .push(json!({ "id": format!("l{next}"), "text": text, "done": false, "by": null }));
    push_log(
        &mut st,
        actor,
        &tr!("adicionou {text}", "added {text}"),
        now,
    );
    Ok(Applied::new(
        st,
        tr!("{actor} adicionou {text}", "{actor} added {text}"),
        tr!("Adicionado: {text}.", "Added: {text}."),
    ))
}

pub(super) fn list_toggle(
    args: &Value,
    mut st: Value,
    actor: &str,
    now: i64,
) -> Result<Applied, String> {
    let id = s(args, "id").ok_or_else(|| t("qual item?", "which item?"))?;
    let item = st["items"]
        .as_array_mut()
        .into_iter()
        .flatten()
        .find(|i| i["id"].as_str() == Some(id.as_str()))
        .ok_or_else(|| t("item não existe", "no such item"))?;
    let done = !item["done"].as_bool().unwrap_or(false);
    item["done"] = json!(done);
    item["by"] = if done { json!(actor) } else { Value::Null };
    let text = item["text"].as_str().unwrap_or("").to_string();
    let what = if done {
        tr!("marcou {text}", "checked {text}")
    } else {
        tr!("desmarcou {text}", "unchecked {text}")
    };
    push_log(&mut st, actor, &what, now);
    Ok(Applied::new(
        st,
        format!("{actor} {what}"),
        format!(
            "{}: {text}.",
            if done {
                ts("Marcado", "Checked")
            } else {
                ts("Desmarcado", "Unchecked")
            }
        ),
    ))
}
