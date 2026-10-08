//! # Servidor MCP local dos mini-apps embutidos
//!
//! Implementa o lado *servidor* do MCP Apps (SEP-1865, versão `2026-01-26`) para
//! três mini-apps que vêm dentro do app, sem rede nem chaves:
//!
//! - **pet**: um bichinho do grupo (o burro do Wabi, em pixel art). Comer, dormir, brincar.
//! - **poll**: enquete que, ao fechar, vira uma linha do plano do Espaço.
//! - **list**: lista compartilhada ("o que levar").
//! - **maptap**: jogo de geografia por turnos (os mesmos 5 lugares para todo mundo).
//! - **recipe**: receita do jantar com porções e modo cozinhar.
//! - **countdown**: contagem regressiva do grupo (viagem, show).
//!
//! pet, maptap e recipe são desenhados em SwiftUI nativo pelo Roda (o recurso `ui://`
//! do pet continua servido, para qualquer host MCP); poll e list rodam como View MCP
//! no WKWebView isolado.
//!
//! Cada mini-app declara um recurso `ui://roda/<app>` (`text/html;profile=mcp-app`)
//! e ferramentas com `_meta.ui.resourceUri` e `visibility` (`model` = o agente cria,
//! `app` = a interface chama). O estado compartilhado mora num Item versionado; quem
//! decide se uma chamada pode rodar é o `roda-grants` (no `engine`), não este módulo.

use roda_types::ActionClass;

use crate::i18n::{is_en, t, thousands, ts};
use crate::tr;
use serde_json::{json, Value};

pub const MIME: &str = "text/html;profile=mcp-app";

pub struct ToolSpec {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub schema: Value,
    pub visibility: Vec<&'static str>,
    pub action: ActionClass,
    pub read_only: bool,
}

pub struct AppSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub resource_uri: &'static str,
    /// View MCP (HTML). `None` = só interface nativa no Roda.
    pub html: Option<&'static str>,
    /// Built bundles (tools/miniapp-build) ship a manifest with their sha256.
    pub manifest: Option<&'static str>,
    pub tools: Vec<ToolSpec>,
}

/// Efeito fora do próprio Item (o engine executa, com o mesmo crivo de Concessão).
pub enum Effect {
    AddPlanLine { text: String, cost_cents: i64 },
}

pub struct Applied {
    pub state: Value,
    pub note: String,
    pub text: String,
    pub effect: Option<Effect>,
    /// Novo título do Item (ex.: o grupo renomeou o bichinho).
    pub title: Option<String>,
    /// O agente comenta o evento na conversa (ex.: novo recorde).
    pub comment: Option<String>,
}

impl Applied {
    fn new(state: Value, note: String, text: String) -> Self {
        Applied {
            state,
            note,
            text,
            effect: None,
            title: None,
            comment: None,
        }
    }
}

fn tool(
    name: &'static str,
    title: &'static str,
    description: &'static str,
    schema: Value,
    visibility: &[&'static str],
    action: ActionClass,
    read_only: bool,
) -> ToolSpec {
    ToolSpec {
        name,
        title,
        description,
        schema,
        visibility: visibility.to_vec(),
        action,
        read_only,
    }
}

fn empty() -> Value {
    json!({ "type": "object", "properties": {} })
}

pub fn specs() -> Vec<AppSpec> {
    use ActionClass::*;
    vec![
        AppSpec {
            id: "pet",
            name: ts("Jumento do grupo", "Group donkey"),
            description: ts("Um jumento em pixel art que mora no grupo: comer, cochilar, brincar e a Corrida do Jumento, com placar do grupo. Todo mundo vê o mesmo bicho e quem fez o quê.", "A pixel-art donkey that lives in the group: eat, nap, play and Donkey Dash, with a group leaderboard. Everyone sees the same pet and who did what."),
            resource_uri: "ui://roda/pet",
            html: Some(include_str!("../apps/pet.html")),
            manifest: None,
            tools: vec![
                tool("adopt_pet", ts("Adotar um bichinho", "Adopt a pet"), ts("Cria o bichinho compartilhado do grupo.", "Creates the group’s shared pet."), json!({"type":"object","properties":{"name":{"type":"string"},"species":{"type":"string","enum":["jumento"]}}}), &["model"], Reversible, false),
                tool("pet_feed", ts("Dar comida", "Feed"), ts("Dá uma cenoura ao bichinho (não dá se ele estiver dormindo).", "Gives the pet a carrot (not while it’s asleep)."), empty(), &["app"], Reversible, false),
                tool("pet_nap", ts("Cochilar", "Nap"), ts("Põe o bichinho para dormir; o descanso sobe com o tempo.", "Puts the pet to sleep; rest goes up over time."), empty(), &["app"], Reversible, false),
                tool("pet_wake", ts("Acordar", "Wake"), ts("Acorda o bichinho.", "Wakes the pet up."), empty(), &["app"], Reversible, false),
                tool("pet_play", ts("Brincar", "Play"), ts("Brinca com o bichinho.", "Plays with the pet."), empty(), &["app"], Reversible, false),
                tool("pet_rename", ts("Renomear", "Rename"), ts("Dá um nome novo ao bichinho (muda o cartão para todos).", "Gives the pet a new name (updates the card for everyone)."), json!({"type":"object","properties":{"name":{"type":"string","maxLength":18}},"required":["name"]}), &["app", "model"], Reversible, false),
                tool("pet_dash_score", ts("Registrar corrida", "Record a run"), ts("Registra uma corrida da Corrida do Jumento no placar do grupo.", "Records a Donkey Dash run on the group leaderboard."), json!({"type":"object","properties":{"meters":{"type":"integer"},"carrots":{"type":"integer"}},"required":["meters"]}), &["app"], Reversible, false),
                tool("pet_get_state", ts("Ver estado", "Get state"), ts("Lê o estado atual (sem mudar nada).", "Reads the current state (changes nothing)."), empty(), &["app"], Reversible, true),
                tool("pet_release", ts("Devolver o bichinho", "Release the pet"), ts("Encerra o bichinho para todo mundo. Não dá para desfazer.", "Ends the pet for everyone. Can’t be undone."), empty(), &["app"], Irreversible, false),
            ],
        },
        AppSpec {
            id: "maptap",
            name: "MapTap",
            description: ts("Jogo de geografia por turnos: os mesmos 5 lugares para todo mundo, um toque no globo por rodada.", "Turn-based geography game: the same 5 places for everyone, one tap on the globe per round."),
            resource_uri: "ui://roda/maptap",
            html: None,
            manifest: None,
            tools: vec![
                tool("start_maptap", ts("Começar MapTap", "Start MapTap"), ts("Abre uma partida de MapTap no grupo.", "Opens a MapTap game in the group."), empty(), &["model"], Reversible, false),
                tool("maptap_guess", ts("Cravar palpite", "Lock in guess"), ts("Crava o palpite da rodada (latitude/longitude) e calcula a distância.", "Locks in the round’s guess (latitude/longitude) and computes the distance."), json!({"type":"object","properties":{"round":{"type":"integer"},"lat":{"type":"number"},"lon":{"type":"number"}},"required":["round","lat","lon"]}), &["app"], Reversible, false),
                tool("maptap_get_state", ts("Ver estado", "Get state"), ts("Lê a partida (sem mudar nada).", "Reads the game (changes nothing)."), empty(), &["app"], Reversible, true),
            ],
        },
        AppSpec {
            id: "recipe",
            name: ts("Jantar", "Dinner"),
            description: ts("Receita compartilhada: porções que recalculam os ingredientes, checklist e modo cozinhar.", "Shared recipe: servings that rescale the ingredients, a checklist and cooking mode."),
            resource_uri: "ui://roda/recipe",
            html: None,
            manifest: None,
            tools: vec![
                tool("start_recipe", ts("Salvar receita", "Save recipe"), ts("Salva uma receita no grupo.", "Saves a recipe in the group."), json!({"type":"object","properties":{"dish":{"type":"string"},"servings":{"type":"integer"}}}), &["model"], Reversible, false),
                tool("recipe_servings", ts("Porções", "Servings"), ts("Muda o número de porções.", "Changes the number of servings."), json!({"type":"object","properties":{"servings":{"type":"integer","minimum":1,"maximum":12}},"required":["servings"]}), &["app"], Reversible, false),
                tool("recipe_check", ts("Marcar ingrediente", "Check ingredient"), ts("Marca ou desmarca um ingrediente.", "Checks or unchecks an ingredient."), json!({"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}), &["app"], Reversible, false),
                tool("recipe_cook", ts("Cozinhar", "Cook"), ts("Registra em que passo do modo cozinhar alguém está.", "Records which cooking-mode step someone is on."), json!({"type":"object","properties":{"step":{"type":"integer"}},"required":["step"]}), &["app"], Reversible, false),
                tool("recipe_get_state", ts("Ver estado", "Get state"), ts("Lê a receita (sem mudar nada).", "Reads the recipe (changes nothing)."), empty(), &["app"], Reversible, true),
            ],
        },
        AppSpec {
            id: "poll",
            name: ts("Enquete", "Poll"),
            description: ts("Votação rápida no grupo. Ao fechar, a opção vencedora entra no plano do Espaço.", "Quick group vote. When it closes, the winning option goes into the Space’s plan."),
            resource_uri: "ui://roda/poll",
            html: Some(include_str!("../apps/poll.html")),
            manifest: None,
            tools: vec![
                tool("start_poll", ts("Abrir enquete", "Open poll"), ts("Abre uma enquete no grupo.", "Opens a poll in the group."), json!({"type":"object","properties":{"question":{"type":"string"},"options":{"type":"array","items":{"type":"string"}}},"required":["question","options"]}), &["model"], Reversible, false),
                tool("poll_vote", ts("Votar", "Vote"), ts("Vota numa opção (troca o voto se já votou).", "Votes for an option (changes the vote if you already voted)."), json!({"type":"object","properties":{"option":{"type":"string"}},"required":["option"]}), &["app"], Reversible, false),
                tool("poll_add_option", ts("Sugerir opção", "Suggest option"), ts("Adiciona uma opção.", "Adds an option."), json!({"type":"object","properties":{"text":{"type":"string"}},"required":["text"]}), &["app"], Reversible, false),
                tool("poll_get_state", ts("Ver estado", "Get state"), ts("Lê os votos (sem mudar nada).", "Reads the votes (changes nothing)."), empty(), &["app"], Reversible, true),
                tool("poll_close", ts("Fechar e levar ao plano", "Close and add to plan"), ts("Fecha a enquete e põe a vencedora no plano do Espaço.", "Closes the poll and puts the winner in the Space’s plan."), empty(), &["app"], Reversible, false),
            ],
        },
        AppSpec {
            id: "list",
            name: ts("Lista do grupo", "Group list"),
            description: ts("Lista compartilhada: quem marcou cada coisa fica registrado.", "Shared list: who checked off each thing is recorded."),
            resource_uri: "ui://roda/list",
            html: Some(include_str!("../apps/list.html")),
            manifest: None,
            tools: vec![
                tool("start_list", ts("Criar lista", "Create list"), ts("Cria uma lista compartilhada.", "Creates a shared list."), json!({"type":"object","properties":{"title":{"type":"string"},"items":{"type":"array","items":{"type":"string"}}},"required":["title"]}), &["model"], Reversible, false),
                tool("list_add", ts("Adicionar", "Add"), ts("Adiciona um item.", "Adds an item."), json!({"type":"object","properties":{"text":{"type":"string"}},"required":["text"]}), &["app"], Reversible, false),
                tool("list_toggle", ts("Marcar", "Check"), ts("Marca ou desmarca um item.", "Checks or unchecks an item."), json!({"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}), &["app"], Reversible, false),
                tool("list_get_state", ts("Ver estado", "Get state"), ts("Lê a lista (sem mudar nada).", "Reads the list (changes nothing)."), empty(), &["app"], Reversible, true),
                tool("list_send_whatsapp", ts("Mandar no WhatsApp", "Send on WhatsApp"), ts("Manda a lista para fora do Zoen, pelo WhatsApp.", "Sends the list outside Zoen, via WhatsApp."), empty(), &["app"], External, false),
            ],
        },
        AppSpec {
            id: "countdown",
            name: ts("Contagem", "Countdown"),
            description: ts("Contagem regressiva do grupo para uma data (viagem, show, aniversário).", "The group’s countdown to a date (a trip, a show, a birthday)."),
            resource_uri: "ui://roda/countdown",
            html: None,
            manifest: None,
            tools: vec![
                tool("start_countdown", ts("Criar contagem", "Start countdown"), ts("Cria uma contagem regressiva no grupo.", "Creates a countdown in the group."), json!({"type":"object","properties":{"title":{"type":"string","maxLength":32},"place":{"type":"string","maxLength":32},"target_ms":{"type":"integer"}},"required":["title","target_ms"]}), &["model"], Reversible, false),
                tool("countdown_get_state", ts("Ver estado", "Get state"), ts("Lê a contagem (sem mudar nada).", "Reads the countdown (changes nothing)."), empty(), &["app"], Reversible, true),
            ],
        },
        AppSpec {
            id: "hike",
            name: ts("Trilha de sábado", "Saturday hike"),
            description: ts("Três trilhas com mapa e fotos: o grupo compara, vota e o agente monta o dia (carona, horários).", "Three trails with maps and photos: the group compares, votes and the agent plans the day (rides, times)."),
            resource_uri: "ui://roda/hike",
            html: Some(include_str!("../apps/hike.html")),
            manifest: Some(include_str!("../apps/hike.manifest.json")),
            tools: vec![
                tool("start_hike", ts("Planejar uma trilha", "Plan a hike"), ts("Abre a trilha do grupo com três opções.", "Opens the group's hike with three options."), json!({"type":"object","properties":{"day":{"type":"string","maxLength":16},"area":{"type":"string","maxLength":24}}}), &["model"], Reversible, false),
                tool("hike_vote", ts("Votar", "Vote"), ts("Vota numa trilha (troca o voto se já votou).", "Votes for a trail (changes your vote if you already voted)."), json!({"type":"object","properties":{"trail":{"type":"string","enum":["tomales","steep","lands"]}},"required":["trail"]}), &["app"], Reversible, false),
                tool("hike_decide", ts("Fechar a escolha", "Lock it in"), ts("Fecha a votação na trilha mais votada.", "Closes the vote on the most voted trail."), empty(), &["app"], Reversible, false),
                tool("hike_set_itinerary", ts("Montar o roteiro", "Plan the day"), ts("Monta os horários do dia a partir de quem dirige e quem vai.", "Builds the day's times from who drives and who's in."), json!({"type":"object","properties":{"driver":{"type":"string"},"pickups":{"type":"array","items":{"type":"object","properties":{"names":{"type":"array","items":{"type":"string"}},"place":{"type":"string"}}}}},"required":["driver","pickups"]}), &["model", "app"], Reversible, false),
                tool("hike_add_photos", ts("Pôr fotos no álbum", "Add photos to the album"), ts("Põe no álbum do grupo as fotos que você escolheu e confirmou.", "Adds the photos you picked and confirmed to the group's album."), json!({"type":"object","properties":{"photos":{"type":"array","items":{"type":"string"},"maxItems":6}},"required":["photos"]}), &["app"], Reversible, false),
                tool("hike_get_state", ts("Ver estado", "Get state"), ts("Lê votos, escolha e roteiro (sem mudar nada).", "Reads votes, choice and plan (changes nothing)."), empty(), &["app"], Reversible, true),
            ],
        },
    ]
}

pub fn spec(app: &str) -> Option<AppSpec> {
    specs().into_iter().find(|s| s.id == app)
}

/// O mini-app cujo tool `model` cria uma instância.
pub fn spec_for_start_tool(tool_name: &str) -> Option<AppSpec> {
    specs().into_iter().find(|s| {
        s.tools
            .iter()
            .any(|t| t.name == tool_name && t.visibility.contains(&"model"))
    })
}

pub fn spec_for_uri(uri: &str) -> Option<AppSpec> {
    specs().into_iter().find(|s| s.resource_uri == uri)
}

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k)
        .and_then(|x| x.as_str())
        .map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty())
}

fn clamp(x: f64) -> f64 {
    x.clamp(0.0, 100.0)
}

fn push_log(state: &mut Value, who: &str, what: &str, now: i64) {
    let log = state["log"]
        .as_array_mut()
        .map(std::mem::take)
        .unwrap_or_default();
    let mut log: Vec<Value> = log;
    log.insert(0, json!({ "who": who, "what": what, "at": now }));
    log.truncate(8);
    state["log"] = Value::Array(log);
}

// ── criação ──

pub fn create(app: &str, args: &Value, actor: &str, now: i64) -> Result<(String, Value), String> {
    match app {
        "pet" => {
            let name = s(args, "name")
                .map(|n| n.chars().take(18).collect())
                .unwrap_or_else(|| t("Jumento", "Donkey"));
            let mut st = json!({
                "name": name, "species": "jumento",
                "fullness": 72.0, "energy": 72.0, "joy": 76.0,
                "asleep": false, "released": false,
                "updated": now, "born": now, "log": [],
                "dash": { "best": {}, "runs": 0 }
            });
            push_log(&mut st, actor, &t("adotou", "adopted"), now);
            Ok((name, st))
        }
        "maptap" => {
            let places = maptap_places(now);
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
        "poll" => {
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
        "list" => {
            let title = s(args, "title").unwrap_or_else(|| t("O que levar", "What to bring"));
            let items: Vec<Value> = args["items"].as_array().map(|a| a.iter().filter_map(|x| x.as_str()).enumerate().map(|(i, t)| json!({ "id": format!("l{}", i + 1), "text": t, "done": false, "by": null })).collect()).unwrap_or_default();
            let mut st =
                json!({ "title": title, "items": items, "next": 100, "sent": false, "log": [] });
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
        _ => Err(tr!(
            "mini-app desconhecido: {app}",
            "unknown mini-app: {app}"
        )),
    }
}

// ── leitura (com o tempo passando, no caso do bichinho) ──

/// O estado como a interface deve ver agora (o bichinho sente fome com o tempo).
pub fn view(app: &str, state: &Value, now: i64) -> Value {
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
        v["mood"] = json!(pet_mood(&v));
    }
    v
}

fn pet_mood(v: &Value) -> &'static str {
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
pub fn headline(app: &str, state: &Value, now: i64) -> (String, Vec<(String, f64)>) {
    let v = view(app, state, now);
    match app {
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
        "countdown" => {
            let left = (v["target"].as_i64().unwrap_or(now) - now).max(0);
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

// ── ações ──

pub fn apply(
    app: &str,
    tool_name: &str,
    args: &Value,
    state: &Value,
    actor: &str,
    now: i64,
) -> Result<Applied, String> {
    // Materializa o tempo passado antes de mudar (o bichinho sentiu fome enquanto isso).
    let mut st = view(app, state, now);
    match (app, tool_name) {
        ("pet", tool) => {
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
        ("maptap", "maptap_guess") => {
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
        ("recipe", "recipe_servings") => {
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
        ("recipe", "recipe_check") => {
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
        ("recipe", "recipe_cook") => {
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
        ("poll", "poll_vote") => {
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
        ("poll", "poll_add_option") => {
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
        ("poll", "poll_close") => {
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
        ("list", "list_add") => {
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
        ("list", "list_toggle") => {
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
        ("list", "list_send_whatsapp") => {
            st["sent"] = json!(true);
            push_log(
                &mut st,
                actor,
                &t(
                    "mandou no WhatsApp (simulação)",
                    "sent it on WhatsApp (simulated)",
                ),
                now,
            );
            Ok(Applied::new(st, tr!("{actor} mandou a lista no WhatsApp (simulação)", "{actor} sent the list on WhatsApp (simulated)"), t("Simulação: nada saiu do aparelho. No produto, isto abriria o WhatsApp com a lista.", "Simulated: nothing left the device. In the product, this would open WhatsApp with the list.")))
        }
        ("hike", "hike_vote") => {
            if st["decided"].as_str().is_some() {
                return Err(t(
                    "a escolha já foi fechada",
                    "the choice is already locked in",
                ));
            }
            let trail = s(args, "trail").ok_or_else(|| t("qual trilha?", "which trail?"))?;
            let name =
                hike_name(&trail).ok_or_else(|| t("trilha desconhecida", "unknown trail"))?;
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
        ("hike", "hike_decide") => {
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
        ("hike", "hike_set_itinerary") => {
            let trail = st["decided"]
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| t("escolham a trilha primeiro", "pick the trail first"))?;
            let driver = s(args, "driver").ok_or_else(|| t("quem dirige?", "who drives?"))?;
            let (_, name, drive_min, hike_h) =
                HIKE_TRAILS
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
        ("hike", "hike_add_photos") => {
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
        ("countdown", "countdown_get_state") => Ok(Applied::new(st, String::new(), String::new())),
        (a, tool) => Err(tr!(
            "{a} não tem a ferramenta {tool}",
            "{a} has no tool {tool}"
        )),
    }
}

/// Resultado no formato `CallToolResult` do MCP.
pub fn call_tool_result(text: &str, structured: &Value, is_error: bool) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "structuredContent": structured, "isError": is_error })
}

// ── MapTap ──

/// Lugares do MapTap: os mesmos para todo mundo no grupo (sorteados pelo dia).
/// (name, country, hint) in pt-BR and en, then lat/lon.
type Place = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    f64,
    f64,
);
const PLACES: &[Place] = &[
    (
        "Liubliana",
        "Ljubljana",
        "Eslovênia",
        "Slovenia",
        "Um pouco de energia de dragão. Ache no globo.",
        "A little dragon energy. Find it on the globe.",
        46.0569,
        14.5058,
    ),
    (
        "Ushuaia",
        "Ushuaia",
        "Argentina",
        "Argentina",
        "A cidade mais ao sul do mundo.",
        "The southernmost city in the world.",
        -54.8019,
        -68.3030,
    ),
    (
        "Fernando de Noronha",
        "Fernando de Noronha",
        "Brasil",
        "Brazil",
        "Ilha no meio do Atlântico, a 350 km da costa.",
        "An island in the middle of the Atlantic, 350 km off the coast.",
        -3.8547,
        -32.4247,
    ),
    (
        "Reykjavík",
        "Reykjavík",
        "Islândia",
        "Iceland",
        "Capital mais ao norte do planeta.",
        "The northernmost capital on the planet.",
        64.1466,
        -21.9426,
    ),
    (
        "Ulan Bator",
        "Ulaanbaatar",
        "Mongólia",
        "Mongolia",
        "Capital mais fria do mundo.",
        "The coldest capital in the world.",
        47.8864,
        106.9057,
    ),
    (
        "Lençóis Maranhenses",
        "Lençóis Maranhenses",
        "Brasil",
        "Brazil",
        "Dunas brancas com lagoas de chuva.",
        "White dunes with rainwater lagoons.",
        -2.4870,
        -43.1280,
    ),
    (
        "Tbilisi",
        "Tbilisi",
        "Geórgia",
        "Georgia",
        "Entre o Mar Negro e o Cáspio.",
        "Between the Black Sea and the Caspian.",
        41.7151,
        44.8271,
    ),
    (
        "Zanzibar",
        "Zanzibar",
        "Tanzânia",
        "Tanzania",
        "Ilha das especiarias no Índico.",
        "The spice island in the Indian Ocean.",
        -6.1659,
        39.2026,
    ),
    (
        "Valparaíso",
        "Valparaíso",
        "Chile",
        "Chile",
        "Morros coloridos de frente para o Pacífico.",
        "Colorful hills facing the Pacific.",
        -33.0472,
        -71.6127,
    ),
    (
        "Hanói",
        "Hanoi",
        "Vietnã",
        "Vietnam",
        "Lagos no meio da cidade e café com ovo.",
        "Lakes in the middle of the city and egg coffee.",
        21.0278,
        105.8342,
    ),
];

fn maptap_places(now: i64) -> Value {
    let day = (now / 86_400_000) as usize;
    let en = is_en();
    let picks: Vec<Value> = (0..5)
        .map(|i| {
            let (name_pt, name_en, country_pt, country_en, hint_pt, hint_en, lat, lon) =
                PLACES[(day * 3 + i * 7) % PLACES.len()];
            let (name, country, hint) = if en {
                (name_en, country_en, hint_en)
            } else {
                (name_pt, country_pt, hint_pt)
            };
            json!({ "name": name, "country": country, "hint": hint, "lat": lat, "lon": lon })
        })
        .collect();
    Value::Array(picks)
}

pub fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6371.0_f64;
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dp = (lat2 - lat1).to_radians();
    let dl = (lon2 - lon1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

/// (quem, pontos, rodadas jogadas), do maior para o menor.
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

// ── Receita ──

fn recipe_default() -> Value {
    if is_en() {
        return json!({
            "title": "Plantain moqueca",
            "subtitle": "Vegetarian · 35 min · from Zoen",
            "baseServings": 2,
            "servings": 3,
            "ingredients": [
                { "id": "i1", "name": "Ripe plantains", "qty": 2.0, "unit": "", "done": false },
                { "id": "i2", "name": "Coconut milk", "qty": 200.0, "unit": "ml", "done": false },
                { "id": "i3", "name": "Red palm oil", "qty": 1.0, "unit": "tbsp", "done": false },
                { "id": "i4", "name": "Tomatoes", "qty": 2.0, "unit": "", "done": false },
                { "id": "i5", "name": "Bell pepper", "qty": 1.0, "unit": "", "done": false },
                { "id": "i6", "name": "Red onion", "qty": 1.0, "unit": "", "done": false },
                { "id": "i7", "name": "Cilantro", "qty": 0.5, "unit": "bunch", "done": false },
                { "id": "i8", "name": "Lime", "qty": 1.0, "unit": "", "done": false }
            ],
            "steps": [
                { "text": "Slice the plantains into thick rounds and season with lime and salt.", "minutes": 0 },
                { "text": "Sauté the onion, pepper and tomato in the palm oil until soft.", "minutes": 8 },
                { "text": "Add the plantain and coconut milk. Cover and simmer on low heat.", "minutes": 12 },
                { "text": "Finish with chopped cilantro and serve with rice and farofa.", "minutes": 0 }
            ],
            "cooking": null,
            "log": []
        });
    }
    json!({
        "title": "Moqueca de banana-da-terra",
        "subtitle": "Vegetariana · 35 min · do Zoen",
        "baseServings": 2,
        "servings": 3,
        "ingredients": [
            { "id": "i1", "name": "Banana-da-terra madura", "qty": 2.0, "unit": "un", "done": false },
            { "id": "i2", "name": "Leite de coco", "qty": 200.0, "unit": "ml", "done": false },
            { "id": "i3", "name": "Azeite de dendê", "qty": 1.0, "unit": "colher", "done": false },
            { "id": "i4", "name": "Tomate", "qty": 2.0, "unit": "un", "done": false },
            { "id": "i5", "name": "Pimentão colorido", "qty": 1.0, "unit": "un", "done": false },
            { "id": "i6", "name": "Cebola roxa", "qty": 1.0, "unit": "un", "done": false },
            { "id": "i7", "name": "Coentro", "qty": 0.5, "unit": "maço", "done": false },
            { "id": "i8", "name": "Limão", "qty": 1.0, "unit": "un", "done": false }
        ],
        "steps": [
            { "text": "Corte a banana em rodelas grossas e tempere com limão e sal.", "minutes": 0 },
            { "text": "Refogue cebola, pimentão e tomate no dendê até murchar.", "minutes": 8 },
            { "text": "Junte a banana e o leite de coco. Tampe e cozinhe em fogo baixo.", "minutes": 12 },
            { "text": "Finalize com coentro picado e sirva com arroz e farofa.", "minutes": 0 }
        ],
        "cooking": null,
        "log": []
    })
}

/// The three Bay Area trails of the hike demo: id, name, drive minutes from SF, hike hours.
/// Routes, photos and copy live in the mini-app bundle (miniapps/hike).
pub const HIKE_TRAILS: [(&str, &str, i64, i64); 3] = [
    ("tomales", "Tomales Point", 90, 5),
    ("steep", "Steep Ravine", 45, 3),
    ("lands", "Lands End", 25, 2),
];

fn hike_name(id: &str) -> Option<&'static str> {
    // English ids stay stable; labels follow the core language.
    match id {
        "tomales" => Some(if is_en() {
            "Tomales Point"
        } else {
            "Praia do Sono"
        }),
        "steep" => Some(if is_en() {
            "Steep Ravine"
        } else {
            "Pedra Grande"
        }),
        "lands" => Some(if is_en() {
            "Lands End"
        } else {
            "Pico do Jaraguá"
        }),
        _ => None,
    }
}

fn st_day(st: &Value) -> String {
    st["day"].as_str().unwrap_or("").to_lowercase()
}

/// 8:15, 10:00, 3:00 (12-hour in English, 24-hour in Portuguese).
fn clock(minutes: i64) -> String {
    let (h, m) = (minutes / 60 % 24, minutes % 60);
    if is_en() {
        format!("{}:{m:02}", if h % 12 == 0 { 12 } else { h % 12 })
    } else {
        format!("{h}:{m:02}")
    }
}
