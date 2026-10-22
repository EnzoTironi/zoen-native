//! Built-in pet tool actions.

use super::*;

pub(super) fn apply(
    tool: &str,
    args: &Value,
    mut st: Value,
    actor: &str,
    now: i64,
) -> Result<Applied, String> {
    if st["released"].as_bool().unwrap_or(false) {
        return Err(t("o bichinho já foi embora", "the pet is already gone"));
    }
    let name = st["name"]
        .as_str()
        .unwrap_or(ts("O bichinho", "The pet"))
        .to_string();
    let asleep = st["asleep"].as_bool().unwrap_or(false);
    let mut title = None;
    let mut comment = None;
    let (what, text) = match tool {
        "pet_feed" => {
            if asleep {
                return Err(tr!(
                    "{name} está dormindo. Acorde primeiro.",
                    "{name} is asleep. Wake him up first."
                ));
            }
            st["fullness"] = json!(clamp(st["fullness"].as_f64().unwrap_or(0.0) + 22.0));
            st["joy"] = json!(clamp(st["joy"].as_f64().unwrap_or(0.0) + 4.0));
            (
                tr!("deu uma cenoura para o {name}", "fed {name} a carrot"),
                tr!(
                    "{name} comeu a cenoura. Nham.",
                    "{name} ate the carrot. Yum."
                ),
            )
        }
        "pet_nap" => {
            if asleep {
                return Err(tr!("{name} já está dormindo.", "{name} is already asleep."));
            }
            st["energy"] = json!(clamp(st["energy"].as_f64().unwrap_or(0.0) + 8.0));
            st["asleep"] = json!(true);
            (
                tr!("pôs o {name} para cochilar", "put {name} down for a nap"),
                tr!("{name} está cochilando. Zzz.", "{name} is napping. Zzz."),
            )
        }
        "pet_wake" => {
            if !asleep {
                return Err(tr!("{name} já está acordado.", "{name} is already awake."));
            }
            st["asleep"] = json!(false);
            st["energy"] = json!(clamp(st["energy"].as_f64().unwrap_or(0.0) + 6.0));
            (
                tr!("acordou o {name}", "woke {name} up"),
                tr!(
                    "{name} acordou espreguiçando.",
                    "{name} woke up stretching."
                ),
            )
        }
        "pet_play" => {
            if asleep {
                return Err(tr!(
                    "{name} está dormindo. Acorde primeiro.",
                    "{name} is asleep. Wake him up first."
                ));
            }
            st["joy"] = json!(clamp(st["joy"].as_f64().unwrap_or(0.0) + 18.0));
            st["energy"] = json!(clamp(st["energy"].as_f64().unwrap_or(0.0) - 8.0));
            st["fullness"] = json!(clamp(st["fullness"].as_f64().unwrap_or(0.0) - 4.0));
            (
                tr!("brincou com o {name}", "played with {name}"),
                tr!("{name} correu atrás da bola!", "{name} chased the ball!"),
            )
        }
        "pet_rename" => {
            let new: String = s(args, "name")
                .ok_or_else(|| t("qual o nome novo?", "what’s the new name?"))?
                .chars()
                .take(18)
                .collect();
            let new = {
                let mut c = new.chars();
                match c.next() {
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    None => new,
                }
            };
            st["name"] = json!(new);
            title = Some(new.clone());
            comment = Some(tr!(
                "{new}, então. Combina com ele.",
                "{new} it is. Suits him."
            ));
            (
                tr!("renomeou o {name} para {new}", "renamed {name} to {new}"),
                tr!("Agora ele se chama {new}.", "His name is {new} now."),
            )
        }
        "pet_dash_score" => {
            let meters = args["meters"].as_i64().unwrap_or(0).max(0);
            let carrots = args["carrots"].as_i64().unwrap_or(0).max(0);
            let group_best = st["dash"]["best"]
                .as_object()
                .map(|m| {
                    m.values()
                        .filter_map(|v| v["meters"].as_i64())
                        .max()
                        .unwrap_or(0)
                })
                .unwrap_or(0);
            let mine = st["dash"]["best"][actor]["meters"].as_i64().unwrap_or(0);
            if meters > mine {
                st["dash"]["best"][actor] =
                    json!({ "meters": meters, "carrots": carrots, "at": now });
            }
            st["dash"]["runs"] = json!(st["dash"]["runs"].as_i64().unwrap_or(0) + 1);
            st["joy"] = json!(clamp(st["joy"].as_f64().unwrap_or(0.0) + 6.0));
            st["energy"] = json!(clamp(st["energy"].as_f64().unwrap_or(0.0) - 4.0));
            if meters > group_best && meters > 0 {
                comment = Some(tr!("Novo recorde do grupo na Corrida do Jumento: {actor}, {meters} m e {carrots} cenouras. Quem bate?", "New group record in Donkey Dash: {actor}, {meters} m and {carrots} carrots. Who can beat it?"));
            }
            (
                tr!(
                    "correu {meters} m na Corrida do Jumento",
                    "ran {meters} m in Donkey Dash"
                ),
                tr!(
                    "{meters} m · {carrots} cenouras",
                    "{meters} m · {carrots} carrots"
                ),
            )
        }
        "pet_release" => {
            st["released"] = json!(true);
            (
                tr!("devolveu o {name}", "released {name}"),
                tr!(
                    "{name} foi morar num sítio. Tchau, {name}!",
                    "{name} moved to a farm. Bye, {name}!"
                ),
            )
        }
        _ => {
            return Err(tr!(
                "ferramenta desconhecida: {tool}",
                "unknown tool: {tool}"
            ))
        }
    };
    st["updated"] = json!(now);
    st["mood"] = json!(pet_mood(&st));
    push_log(&mut st, actor, &what, now);
    let mut a = Applied::new(st, format!("{actor} {what}"), text);
    a.title = title;
    a.comment = comment;
    Ok(a)
}
