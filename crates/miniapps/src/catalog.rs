//! Ordered built-in app resources and tool schemas.

use super::{deckcraft, json, ts, ActionClass, AppSpec, ToolSpec, Value};

mod schedule;
use ActionClass::{Irreversible, Reversible};

pub(super) fn tool(
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

#[must_use]
pub fn specs() -> Vec<AppSpec> {
    let mut specs = vec![
        excalidraw_spec(),
        pet_spec(),
        maptap_spec(),
        recipe_spec(),
        poll_spec(),
        list_spec(),
        countdown_spec(),
        hike_spec(),
        schedule::spec(),
    ];
    if let Some(deckcraft) = deckcraft::registration::spec() {
        specs.push(deckcraft);
    }
    if let Some(gridcraft) = crate::binary::gridcraft_registration::spec() {
        specs.push(gridcraft);
    }
    if let Some(wordcraft) = crate::binary::wordcraft_registration::spec() {
        specs.push(wordcraft);
    }
    if let Some(photo) = crate::binary::photocraft_registration::spec() {
        specs.push(photo);
    }
    if let Some(pdf) = crate::binary::pdfcraft_registration::spec() {
        specs.push(pdf);
    }
    specs
}

#[must_use]
pub fn spec(app: &str) -> Option<AppSpec> {
    specs().into_iter().find(|s| s.id == app)
}

/// O mini-app cujo tool `model` cria uma instância.
#[must_use]
pub fn spec_for_start_tool(tool_name: &str) -> Option<AppSpec> {
    specs().into_iter().find(|s| {
        s.tools
            .iter()
            .any(|t| t.name == tool_name && t.visibility.contains(&"model"))
    })
}

#[must_use]
pub fn spec_for_uri(uri: &str) -> Option<AppSpec> {
    specs().into_iter().find(|s| s.resource_uri == uri)
}

fn hike_spec() -> AppSpec {
    AppSpec {
            id: "hike",
            name: ts("Trilha de sábado", "Saturday hike"),
            description: ts("Três trilhas com mapa e fotos: o grupo compara, vota e o agente monta o dia (carona, horários).", "Three trails with maps and photos: the group compares, votes and the agent plans the day (rides, times)."),
            resource_uri: "ui://builtin/hike",
            html: Some(include_str!("../resources/hike.html")),
            manifest: Some(include_str!("../resources/hike.manifest.json")),
            tools: vec![
                tool("start_hike", ts("Planejar uma trilha", "Plan a hike"), ts("Abre a trilha do grupo com três opções.", "Opens the group's hike with three options."), json!({"type":"object","properties":{"day":{"type":"string","maxLength":16},"area":{"type":"string","maxLength":24}}}), &["model"], Reversible, false),
                tool("hike_vote", ts("Votar", "Vote"), ts("Vota numa trilha (troca o voto se já votou).", "Votes for a trail (changes your vote if you already voted)."), json!({"type":"object","properties":{"trail":{"type":"string","enum":["tomales","steep","lands"]}},"required":["trail"]}), &["app"], Reversible, false),
                tool("hike_decide", ts("Fechar a escolha", "Lock it in"), ts("Fecha a votação na trilha mais votada.", "Closes the vote on the most voted trail."), empty(), &["app"], Reversible, false),
                tool("hike_set_itinerary", ts("Montar o roteiro", "Plan the day"), ts("Monta os horários do dia a partir de quem dirige e quem vai.", "Builds the day's times from who drives and who's in."), json!({"type":"object","properties":{"driver":{"type":"string"},"pickups":{"type":"array","items":{"type":"object","properties":{"names":{"type":"array","items":{"type":"string"}},"place":{"type":"string"}}}}},"required":["driver","pickups"]}), &["model", "app"], Reversible, false),
                tool("hike_add_photos", ts("Pôr fotos no álbum", "Add photos to the album"), ts("Põe no álbum do grupo as fotos que você escolheu e confirmou.", "Adds the photos you picked and confirmed to the group's album."), json!({"type":"object","properties":{"photos":{"type":"array","items":{"type":"string"},"maxItems":6}},"required":["photos"]}), &["app"], Reversible, false),
                tool("hike_get_state", ts("Ver estado", "Get state"), ts("Lê votos, escolha e roteiro (sem mudar nada).", "Reads votes, choice and plan (changes nothing)."), empty(), &["app"], Reversible, true),
            ],
        }
}

fn countdown_spec() -> AppSpec {
    AppSpec {
        id: "countdown",
        name: ts("Contagem", "Countdown"),
        description: ts(
            "Contagem regressiva do grupo para uma data (viagem, show, aniversário).",
            "The group’s countdown to a date (a trip, a show, a birthday).",
        ),
        resource_uri: "ui://builtin/countdown",
        html: Some(include_str!("../resources/countdown.html")),
        manifest: Some(include_str!("../resources/countdown.manifest.json")),
        tools: vec![
            tool(
                "start_countdown",
                ts("Criar contagem", "Start countdown"),
                ts(
                    "Cria uma contagem regressiva no grupo.",
                    "Creates a countdown in the group.",
                ),
                json!({"type":"object","properties":{"title":{"type":"string","maxLength":32},"place":{"type":"string","maxLength":32},"target_ms":{"type":"integer"}},"required":["title","target_ms"]}),
                &["model"],
                Reversible,
                false,
            ),
            tool(
                "countdown_get_state",
                ts("Ver estado", "Get state"),
                ts(
                    "Lê a contagem (sem mudar nada).",
                    "Reads the countdown (changes nothing).",
                ),
                empty(),
                &["app"],
                Reversible,
                true,
            ),
        ],
    }
}

fn list_spec() -> AppSpec {
    AppSpec {
        id: "list",
        name: ts("Lista do grupo", "Group list"),
        description: ts(
            "Lista compartilhada: quem marcou cada coisa fica registrado.",
            "Shared list: who checked off each thing is recorded.",
        ),
        resource_uri: "ui://builtin/list",
        html: Some(include_str!("../resources/list.html")),
        manifest: Some(include_str!("../resources/list.manifest.json")),
        tools: vec![
            tool(
                "start_list",
                ts("Criar lista", "Create list"),
                ts("Cria uma lista compartilhada.", "Creates a shared list."),
                json!({"type":"object","properties":{"title":{"type":"string"},"items":{"type":"array","items":{"type":"string"}}},"required":["title"]}),
                &["model"],
                Reversible,
                false,
            ),
            tool(
                "list_add",
                ts("Adicionar", "Add"),
                ts("Adiciona um item.", "Adds an item."),
                json!({"type":"object","properties":{"text":{"type":"string"}},"required":["text"]}),
                &["app"],
                Reversible,
                false,
            ),
            tool(
                "list_toggle",
                ts("Marcar", "Check"),
                ts("Marca ou desmarca um item.", "Checks or unchecks an item."),
                json!({"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}),
                &["app"],
                Reversible,
                false,
            ),
            tool(
                "list_get_state",
                ts("Ver estado", "Get state"),
                ts(
                    "Lê a lista (sem mudar nada).",
                    "Reads the list (changes nothing).",
                ),
                empty(),
                &["app"],
                Reversible,
                true,
            ),
        ],
    }
}

fn poll_spec() -> AppSpec {
    AppSpec {
        id: "poll",
        name: ts("Enquete", "Poll"),
        description: ts(
            "Votação rápida no grupo. Ao fechar, a opção vencedora entra no plano do Espaço.",
            "Quick group vote. When it closes, the winning option goes into the Space’s plan.",
        ),
        resource_uri: "ui://builtin/poll",
        html: Some(include_str!("../resources/poll.html")),
        manifest: Some(include_str!("../resources/poll.manifest.json")),
        tools: vec![
            tool(
                "start_poll",
                ts("Abrir enquete", "Open poll"),
                ts("Abre uma enquete no grupo.", "Opens a poll in the group."),
                json!({"type":"object","properties":{"question":{"type":"string"},"options":{"type":"array","items":{"type":"string"}}},"required":["question","options"]}),
                &["model"],
                Reversible,
                false,
            ),
            tool(
                "poll_vote",
                ts("Votar", "Vote"),
                ts(
                    "Vota numa opção (troca o voto se já votou).",
                    "Votes for an option (changes the vote if you already voted).",
                ),
                json!({"type":"object","properties":{"option":{"type":"string"}},"required":["option"]}),
                &["app"],
                Reversible,
                false,
            ),
            tool(
                "poll_add_option",
                ts("Sugerir opção", "Suggest option"),
                ts("Adiciona uma opção.", "Adds an option."),
                json!({"type":"object","properties":{"text":{"type":"string"}},"required":["text"]}),
                &["app"],
                Reversible,
                false,
            ),
            tool(
                "poll_get_state",
                ts("Ver estado", "Get state"),
                ts(
                    "Lê os votos (sem mudar nada).",
                    "Reads the votes (changes nothing).",
                ),
                empty(),
                &["app"],
                Reversible,
                true,
            ),
            tool(
                "poll_close",
                ts("Fechar e levar ao plano", "Close and add to plan"),
                ts(
                    "Fecha a enquete e põe a vencedora no plano do Espaço.",
                    "Closes the poll and puts the winner in the Space’s plan.",
                ),
                empty(),
                &["app"],
                Reversible,
                false,
            ),
        ],
    }
}

fn recipe_spec() -> AppSpec {
    AppSpec {
            id: "recipe",
            name: ts("Jantar", "Dinner"),
            description: ts("Receita compartilhada: porções que recalculam os ingredientes, checklist e modo cozinhar.", "Shared recipe: servings that rescale the ingredients, a checklist and cooking mode."),
            resource_uri: "ui://builtin/recipe",
            html: Some(include_str!("../resources/recipe.html")),
            manifest: Some(include_str!("../resources/recipe.manifest.json")),
            tools: vec![
                tool("start_recipe", ts("Salvar receita", "Save recipe"), ts("Salva uma receita no grupo.", "Saves a recipe in the group."), json!({"type":"object","properties":{"dish":{"type":"string"},"servings":{"type":"integer"}}}), &["model"], Reversible, false),
                tool("recipe_servings", ts("Porções", "Servings"), ts("Muda o número de porções.", "Changes the number of servings."), json!({"type":"object","properties":{"servings":{"type":"integer","minimum":1,"maximum":12}},"required":["servings"]}), &["app"], Reversible, false),
                tool("recipe_check", ts("Marcar ingrediente", "Check ingredient"), ts("Marca ou desmarca um ingrediente.", "Checks or unchecks an ingredient."), json!({"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}), &["app"], Reversible, false),
                tool("recipe_cook", ts("Cozinhar", "Cook"), ts("Registra em que passo do modo cozinhar alguém está.", "Records which cooking-mode step someone is on."), json!({"type":"object","properties":{"step":{"type":"integer"}},"required":["step"]}), &["app"], Reversible, false),
                tool("recipe_get_state", ts("Ver estado", "Get state"), ts("Lê a receita (sem mudar nada).", "Reads the recipe (changes nothing)."), empty(), &["app"], Reversible, true),
            ],
        }
}

fn maptap_spec() -> AppSpec {
    AppSpec {
            id: "maptap",
            name: "MapTap",
            description: ts("Jogo de geografia por turnos: os mesmos 5 lugares para todo mundo, um toque no globo por rodada.", "Turn-based geography game: the same 5 places for everyone, one tap on the globe per round."),
            resource_uri: "ui://builtin/maptap",
            html: Some(include_str!("../resources/maptap.html")),
            manifest: Some(include_str!("../resources/maptap.manifest.json")),
            tools: vec![
                tool("start_maptap", ts("Começar MapTap", "Start MapTap"), ts("Abre uma partida de MapTap no grupo.", "Opens a MapTap game in the group."), empty(), &["model"], Reversible, false),
                tool("maptap_guess", ts("Cravar palpite", "Lock in guess"), ts("Crava o palpite da rodada (latitude/longitude) e calcula a distância.", "Locks in the round’s guess (latitude/longitude) and computes the distance."), json!({"type":"object","properties":{"round":{"type":"integer"},"lat":{"type":"number"},"lon":{"type":"number"}},"required":["round","lat","lon"]}), &["app"], Reversible, false),
                tool("maptap_get_state", ts("Ver estado", "Get state"), ts("Lê a partida (sem mudar nada).", "Reads the game (changes nothing)."), empty(), &["app"], Reversible, true),
            ],
        }
}

fn pet_spec() -> AppSpec {
    AppSpec {
            id: "pet",
            name: ts("Jumento do grupo", "Group donkey"),
            description: ts("Um jumento em pixel art que mora no grupo: comer, cochilar, brincar e a Corrida do Jumento, com placar do grupo. Todo mundo vê o mesmo bicho e quem fez o quê.", "A pixel-art donkey that lives in the group: eat, nap, play and Donkey Dash, with a group leaderboard. Everyone sees the same pet and who did what."),
            resource_uri: "ui://builtin/pet",
            html: Some(include_str!("../resources/pet.html")),
            manifest: Some(include_str!("../resources/pet.manifest.json")),
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
        }
}

fn excalidraw_spec() -> AppSpec {
    AppSpec {
        id: "excalidraw",
        name: "Excalidraw",
        description: ts(
            "Desenho compartilhado e criptografado do grupo.",
            "The group's shared encrypted drawing.",
        ),
        resource_uri: "ui://zoen/excalidraw.html",
        html: Some(include_str!("../resources/excalidraw.html")),
        manifest: Some(include_str!("../resources/excalidraw.manifest.json")),
        tools: vec![
            tool(
                "create_excalidraw",
                ts("Criar desenho", "Create drawing"),
                ts(
                    "Cria um desenho neste Espaço.",
                    "Creates a drawing in this Space.",
                ),
                json!({"type":"object","properties":{"title":{"type":"string","maxLength":160}},"additionalProperties":false}),
                &["model"],
                Reversible,
                false,
            ),
            tool(
                "app_document_read",
                ts("Ler desenho", "Read drawing"),
                ts("Lê o documento deste item.", "Reads this item's document."),
                empty(),
                &["app"],
                Reversible,
                true,
            ),
            tool(
                "app_document_save",
                ts("Salvar desenho", "Save drawing"),
                ts(
                    "Salva uma versão observada deste item.",
                    "Saves an observed version of this item.",
                ),
                empty(),
                &["app"],
                Reversible,
                false,
            ),
        ],
    }
}
