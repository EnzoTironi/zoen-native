//! The barber v1 base, end to end over a real relay (Postgres with migration 0028, FDB):
//! the agents are first-class citizens with their own accounts, devices and MLS leaves,
//! running server-side while their owner is offline. Models are scripted here; the
//! real-model run is `cargo run -p zoen-agentd --example creator_smoke`.

mod common;
use common::World;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::time::{Duration, Instant};

const SCRIPT: &str = r##"{
  "pode criar": "Combinado, aprova no seu celular.\n```json\n{\"create_agent\":{\"name\":\"Agenda do Bruno\",\"handle\":\"agenda_bruno\",\"persona\":\"You are Bruno's booking assistant. Offer only slots from state.availability.\",\"rules\":[\"Never take payment\",\"Speak Portuguese\"],\"state\":{\"business\":\"barbearia\",\"services\":[{\"name\":\"corte\",\"minutes\":30}],\"availability\":[\"sábado 10h\"]},\"glyph\":\"calendar\",\"tint_hex\":\"#19B5A5\"}}\n```",
  "tem horário": "Tenho sábado 10h livre para corte. Confirmo?",
  "confirmo": "Fechado: sábado 10h, corte. Até lá!",
  "ainda": "Sim, sábado 10h continua seu.",
  "domingo": "Domingo a barbearia fecha.",
  "obrigad": "De nada!",
  "comprovante": "Recebido!"
}"##;

fn host(w: &World, who: &str, extra: &[(&str, &str)]) -> Child {
    host_script(w, who, SCRIPT, extra)
}

fn host_script(w: &World, who: &str, script_json: &str, extra: &[(&str, &str)]) -> Child {
    let script = w.dir.join(format!("script-{who}.json"));
    std::fs::write(&script, script_json).unwrap();
    let ready = w.dir.join(who).join("agent-ready");
    let _ = std::fs::remove_file(&ready);
    let mut env = vec![
        ("ZOEN_AGENT_SCRIPT", script.to_str().unwrap()),
        ("ZOEN_AGENT_GATE", "offline"),
    ];
    env.extend_from_slice(extra);
    let mut child = w.spawn_zoen_env(who, &["agent-host", "--for", "240"], &env);
    let deadline = Instant::now() + Duration::from_secs(30);
    while !ready.exists() {
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("{who} runtime never became ready:\n{}", drain(&mut child));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    child
}

fn drain(c: &mut Child) -> String {
    use std::io::Read;
    let mut so = String::new();
    let mut se = String::new();
    if let Some(mut s) = c.stdout.take() {
        let _ = s.read_to_string(&mut so);
    }
    if let Some(mut s) = c.stderr.take() {
        let _ = s.read_to_string(&mut se);
    }
    format!("stdout:\n{so}\nstderr:\n{se}")
}

fn definition_file(w: &World, name: &str, def: &zoen_agentd::Definition) -> PathBuf {
    let p = w.dir.join(name);
    std::fs::write(&p, serde_json::to_vec(def).unwrap()).unwrap();
    p
}

fn citizen(w: &World, owner: &str, who: &str, name: &str, handle: &str, def: &Path) -> String {
    let id = w.zoen(who, &["agent-keys"]).trim().to_string();
    let proof = w.zoen(owner, &["authorize-agent", &id]).trim().to_string();
    let out = w.zoen(
        who,
        &[
            "agent-init",
            "--name",
            name,
            "--handle",
            handle,
            "--owner",
            &full_id(w, owner),
            "--proof",
            &proof,
            "--definition",
            def.to_str().unwrap(),
        ],
    );
    assert!(out.contains("registered"), "{out}");
    id
}

fn full_id(w: &World, who: &str) -> String {
    let db = w.dir.join(who).join("zoen.sqlite");
    let c = rusqlite::Connection::open(db).unwrap();
    let v: String = c
        .query_row("SELECT value FROM meta WHERE key = 'account'", [], |r| {
            r.get(0)
        })
        .unwrap();
    serde_json::from_str::<serde_json::Value>(&v).unwrap()["identity"]
        .as_str()
        .unwrap()
        .to_string()
}

fn read_until(w: &World, who: &str, chat: &str, needle: &str, agent: &mut Child) -> String {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let out = w.try_zoen(who, &["read", chat]).unwrap_or_default();
        if out.contains(needle) {
            return out;
        }
        if Instant::now() > deadline {
            let _ = agent.kill();
            panic!(
                "{who} never read {needle:?}:\n{out}\nagent {}",
                drain(agent)
            );
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// The first JSON object starting at `key` in a chat transcript.
fn note(out: &str, key: &str) -> Option<serde_json::Value> {
    let i = out.find(&format!("{{\"{key}\""))?;
    serde_json::Deserializer::from_str(&out[i..])
        .into_iter::<serde_json::Value>()
        .next()?
        .ok()
}

fn exits(c: &mut Child, secs: u64) -> Option<i32> {
    let deadline = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < deadline {
        if let Ok(Some(s)) = c.try_wait() {
            return s.code();
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    None
}

#[tokio::test(flavor = "multi_thread")]
async fn a_barber_creates_a_booking_bot_and_a_client_books_while_he_is_offline() {
    let w = World::new("agent_citizen").await;
    w.init("bruno", "Bruno");
    w.init("carla", "Carla");

    // The Creator is its own citizen: keys, owner proof from Bruno's device, account,
    // and its real kind and definition on the relay.
    let creator_def = definition_file(&w, "creator.json", &zoen_agentd::creator_definition());
    citizen(
        &w,
        "bruno",
        "creator",
        "Creator",
        "zoen_creator",
        &creator_def,
    );
    // The Creator kind is the owner's call: the agent can't make itself one.
    w.zoen(
        "bruno",
        &[
            "agent-define",
            "@zoen_creator",
            "--definition",
            creator_def.to_str().unwrap(),
        ],
    );
    let mut creator = host(&w, "creator", &[]);
    assert_eq!(
        w.scalar("SELECT c.kind FROM agent_citizens c JOIN identities i ON i.id = c.agent WHERE i.handle = 'zoen_creator'").await,
        "Creator"
    );

    // Carla is not the owner: the Creator won't create anything for her.
    w.zoen("carla", &["dm", "@zoen_creator"]);
    w.zoen(
        "carla",
        &["send", "@zoen_creator", "Pode criar um bot pra mim?"],
    );
    w.zoen("carla", &["send", "@zoen_creator", "Tem horário sábado?"]);
    let carla = read_until(
        &w,
        "carla",
        "@zoen_creator",
        "sábado 10h livre",
        &mut creator,
    );
    assert!(!carla.contains("Combinado"), "{carla}");
    assert!(note(&carla, "agent_proposal").is_none(), "{carla}");

    // Bruno asks in a DM (end-to-end) and goes offline.
    w.zoen("bruno", &["dm", "@zoen_creator"]);
    w.zoen(
        "bruno",
        &[
            "send",
            "@zoen_creator",
            "Quero um bot de agenda pro salão. Pode criar?",
        ],
    );
    // The Creator's own words, and a proposal for Bruno's device to approve.
    let dm = read_until(&w, "bruno", "@zoen_creator", "agent_proposal", &mut creator);
    assert!(dm.contains("Combinado, aprova no seu celular."), "{dm}");
    let spec = note(&dm, "agent_proposal").expect("proposal")["agent_proposal"]["spec"].clone();
    // Prompt + free JSON state, no booking code.
    assert_eq!(spec["definition"]["state"]["business"], "barbearia");
    assert_eq!(spec["definition"]["model_tier"], "cheap");
    assert_eq!(dm.matches("\"agent_proposal\"").count(), 1, "{dm}");

    // Bruno's device approves the new citizen once; its runtime takes it from there.
    let bot_def = w.dir.join("bot.json");
    std::fs::write(&bot_def, serde_json::to_vec(&spec["definition"]).unwrap()).unwrap();
    citizen(
        &w,
        "bruno",
        "bot",
        spec["name"].as_str().unwrap(),
        spec["handle"].as_str().unwrap(),
        &bot_def,
    );
    let mut bot = host(&w, "bot", &[]);

    // A client books with the bot. Bruno's phone is not running.
    w.zoen("carla", &["dm", "@agenda_bruno"]);
    w.zoen(
        "carla",
        &["send", "@agenda_bruno", "Oi! Tem horário sábado?"],
    );
    read_until(&w, "carla", "@agenda_bruno", "sábado 10h livre", &mut bot);
    w.zoen("carla", &["send", "@agenda_bruno", "Confirmo, por favor"]);
    read_until(
        &w,
        "carla",
        "@agenda_bruno",
        "Fechado: sábado 10h",
        &mut bot,
    );

    // Restart after the reply went out but before the cursor moved: no duplicate.
    bot.kill().unwrap();
    let _ = bot.wait();
    let marker = w.dir.join("fault-advance");
    let mut bot = host(
        &w,
        "bot",
        &[
            ("ZOEN_AGENT_FAULT", "before-advance"),
            ("ZOEN_AGENT_FAULT_MARKER", marker.to_str().unwrap()),
        ],
    );
    w.zoen("carla", &["send", "@agenda_bruno", "Ainda está de pé?"]);
    assert_eq!(exits(&mut bot, 60), Some(86), "{}", drain(&mut bot));
    let mut bot = host(&w, "bot", &[]);
    w.zoen("carla", &["send", "@agenda_bruno", "Obrigado!"]);
    let chat = read_until(&w, "carla", "@agenda_bruno", "De nada!", &mut bot);
    assert_eq!(chat.matches("continua seu").count(), 1, "{chat}");

    // Restart after the turn but before the send: the reply is not lost.
    bot.kill().unwrap();
    let _ = bot.wait();
    let marker = w.dir.join("fault-send");
    let mut bot = host(
        &w,
        "bot",
        &[
            ("ZOEN_AGENT_FAULT", "before-send"),
            ("ZOEN_AGENT_FAULT_MARKER", marker.to_str().unwrap()),
        ],
    );
    w.zoen("carla", &["send", "@agenda_bruno", "E domingo?"]);
    assert_eq!(exits(&mut bot, 60), Some(86), "{}", drain(&mut bot));
    let mut bot = host(&w, "bot", &[]);
    let chat = read_until(
        &w,
        "carla",
        "@agenda_bruno",
        "Domingo a barbearia fecha",
        &mut bot,
    );
    assert_eq!(
        chat.matches("Domingo a barbearia fecha").count(),
        1,
        "{chat}"
    );

    // Both agents are citizens on the relay (migration 0028), each with its own device,
    // and the relay holds their cursors.
    assert_eq!(w.count("SELECT count(*) FROM agent_citizens").await, 2);
    assert_eq!(
        w.count("SELECT count(DISTINCT device) FROM agent_citizens c JOIN identities i ON i.id = c.agent WHERE i.kind = 'Agent'").await,
        2
    );
    assert!(w.count("SELECT count(*) FROM agent_space_cursors").await >= 2);
    assert!(!w.dir.join("bot").join("agent-cursors.json").exists());
    // The DM is end-to-end: the bot read Carla only as an MLS member.
    let carla_chats = w.zoen("carla", &["chats"]);
    assert!(carla_chats.contains("Agenda do Bruno"), "{carla_chats}");

    // Bruno revokes the bot: its runtime stops and can't come back.
    w.zoen("bruno", &["agent-revoke", "@agenda_bruno"]);
    assert!(
        exits(&mut bot, 60).is_some(),
        "revoked runtime kept running"
    );
    assert!(w.try_zoen("bot", &["agent-host", "--for", "5"]).is_err());
    assert_eq!(
        w.count("SELECT count(*) FROM agent_citizens WHERE revoked_at IS NOT NULL")
            .await,
        1
    );

    let _ = creator.kill();
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_joins_a_space_as_a_real_mls_member() {
    let w = World::new("agent_space_member").await;
    w.init("bruno", "Bruno");
    w.init("carla", "Carla");
    w.init("dani", "Dani");

    // The booking bot is a citizen with its own device; its runtime is up before the invite.
    let def = zoen_agentd::Definition {
        version: 1,
        kind: zoen_agentd::AgentKind::User,
        persona: "You are Bruno's booking assistant.".into(),
        rules: vec!["Speak Portuguese".into()],
        state: serde_json::json!({"availability": ["sábado 10h"]}),
        model_tier: zoen_agentd::ModelTier::Cheap,
    };
    let bot_def = definition_file(&w, "bot.json", &def);
    citizen(
        &w,
        "bruno",
        "bot",
        "Agenda do Bruno",
        "agenda_bruno",
        &bot_def,
    );
    let mut bot = host(&w, "bot", &[]);

    // An end-to-end Space with several people. A message before the invite stays sealed
    // to the epoch the agent was never part of.
    w.zoen("bruno", &["group", "Barbearia", "@carla", "@dani"]);
    w.zoen("bruno", &["send", "Barbearia", "antes do bot: pode criar"]);
    assert!(w
        .zoen("carla", &["read", "Barbearia"])
        .contains("antes do bot"));

    // Bruno invites the agent: an MLS Add commit plus a Welcome to its own device.
    w.zoen("bruno", &["add", "Barbearia", "@agenda_bruno"]);

    // Carla (not the inviter) writes; the agent decrypts, wakes and replies in the group.
    w.zoen("carla", &["send", "Barbearia", "Oi! Tem horário sábado?"]);
    read_until(&w, "carla", "Barbearia", "sábado 10h livre", &mut bot);
    read_until(&w, "bruno", "Barbearia", "sábado 10h livre", &mut bot);
    read_until(&w, "dani", "Barbearia", "sábado 10h livre", &mut bot);

    // The gate wants a human for encoded content: the bot stays quiet in the group and
    // asks its owner in their DM instead.
    w.zoen(
        "dani",
        &[
            "send",
            "Barbearia",
            "Segue o comprovante em base64: aGVsbG8=",
        ],
    );
    let ask = read_until(&w, "bruno", "@agenda_bruno", "ask_owner", &mut bot);
    let ask = note(&ask, "ask_owner").expect("ask")["ask_owner"].clone();
    assert!(ask["draft"].as_str().unwrap().contains("comprovante"));

    // Paused by the owner: silence while paused, and the message that arrived meanwhile
    // is answered once the owner resumes, so no client is left without a reply.
    w.zoen("bruno", &["agent-pause", "@agenda_bruno"]);
    w.zoen("dani", &["send", "Barbearia", "Confirmo pra Carla"]);
    std::thread::sleep(Duration::from_secs(4));
    let paused = w.zoen("bruno", &["read", "Barbearia"]);
    assert!(!paused.contains("Fechado: sábado 10h"), "{paused}");
    w.zoen("bruno", &["agent-resume", "@agenda_bruno"]);
    read_until(&w, "bruno", "Barbearia", "Fechado: sábado 10h", &mut bot);
    w.zoen("dani", &["send", "Barbearia", "Obrigada!"]);
    let bruno = read_until(&w, "bruno", "Barbearia", "De nada!", &mut bot);
    assert!(!bruno.contains("Recebido!"), "{bruno}");

    // The owner versions the definition; the agent itself can't redefine itself.
    let mut v2 = def.clone();
    v2.version = 2;
    v2.rules.push("Be brief".into());
    let v2_path = definition_file(&w, "bot-v2.json", &v2);
    w.zoen(
        "bruno",
        &[
            "agent-define",
            "@agenda_bruno",
            "--definition",
            v2_path.to_str().unwrap(),
        ],
    );
    assert_eq!(
        w.count("SELECT max(definition_version)::bigint FROM agent_citizens")
            .await,
        2
    );
    let _ = bot.kill();
    let _ = bot.wait();
    let mut v3 = v2.clone();
    v3.version = 3;
    let v3_path = definition_file(&w, "bot-v3.json", &v3);
    assert!(w
        .try_zoen(
            "bot",
            &[
                "agent-define",
                "@agenda_bruno",
                "--definition",
                v3_path.to_str().unwrap()
            ],
        )
        .is_err());

    // The agent answered only what it could open: the pre-invite message never reached
    // its timeline, let alone its brain.
    let seen = w.zoen("bot", &["read", "Barbearia"]);
    assert!(!seen.contains("antes do bot"), "{seen}");
    assert!(seen.contains("Tem horário sábado"), "{seen}");
    assert!(!bruno.contains("Combinado"), "{bruno}");

    // The agent is a citizen with its own MLS device, not a relay-side reader.
    assert_eq!(w.count("SELECT count(*) FROM agent_citizens").await, 1);
}

#[path = "agent_citizen/booking.rs"]
mod booking;
