//! The barber v1 base, end to end over a real relay (Postgres with migration 0028, FDB):
//! the agents are first-class citizens with their own accounts, devices and MLS leaves,
//! running server-side while their owner is offline. Models are scripted here; the
//! real-model run is `cargo run -p zoen-agentd --example creator_smoke`.

mod common;
use common::World;
use std::process::Child;
use std::time::{Duration, Instant};

const SCRIPT: &str = r##"{
  "pode criar": "Combinado.\n```json\n{\"create_agent\":{\"name\":\"Agenda do Bruno\",\"handle\":\"agenda_bruno\",\"persona\":\"You are Bruno's booking assistant. Offer only slots from state.availability.\",\"rules\":[\"Never take payment\",\"Speak Portuguese\"],\"state\":{\"business\":\"barbearia\",\"services\":[{\"name\":\"corte\",\"minutes\":30}],\"availability\":[\"sábado 10h\"]},\"glyph\":\"calendar\",\"tint_hex\":\"#19B5A5\"}}\n```",
  "tem horário": "Tenho sábado 10h livre para corte. Confirmo?",
  "confirmo": "Fechado: sábado 10h, corte. Até lá!",
  "ainda": "Sim, sábado 10h continua seu."
}"##;

fn host(w: &World, who: &str, def: &std::path::Path) -> Child {
    let script = w.dir.join("script.json");
    std::fs::write(&script, SCRIPT).unwrap();
    let ready = w.dir.join(who).join("agent-ready");
    let _ = std::fs::remove_file(&ready);
    let child = w.spawn_zoen_env(
        who,
        &[
            "agent-host",
            "--definition",
            def.to_str().unwrap(),
            "--for",
            "240",
        ],
        &[
            ("ZOEN_AGENT_SCRIPT", script.to_str().unwrap()),
            ("ZOEN_AGENT_GATE", "offline"),
        ],
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    while !ready.exists() {
        assert!(
            Instant::now() < deadline,
            "{who} runtime never became ready"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    child
}

fn citizen(w: &World, owner: &str, who: &str, name: &str, handle: &str) -> String {
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
            let mut so = String::new();
            let mut se = String::new();
            use std::io::Read;
            if let Some(mut s) = agent.stdout.take() {
                let _ = s.read_to_string(&mut so);
            }
            if let Some(mut s) = agent.stderr.take() {
                let _ = s.read_to_string(&mut se);
            }
            panic!("{who} never read {needle:?}:\n{out}\nagent stdout:\n{so}\nagent stderr:\n{se}");
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_barber_creates_a_booking_bot_and_a_client_books_while_he_is_offline() {
    let w = World::new("agent_citizen").await;
    w.init("bruno", "Bruno");

    // The Creator is its own citizen: keys, owner proof from Bruno's device, account.
    citizen(&w, "bruno", "creator", "Creator", "zoen_creator");
    let creator_def = w.dir.join("creator.json");
    std::fs::write(
        &creator_def,
        serde_json::to_vec(&zoen_agentd::creator_definition()).unwrap(),
    )
    .unwrap();
    let mut creator = host(&w, "creator", &creator_def);

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
    let spec_path = w
        .dir
        .join("creator")
        .join("created")
        .join("agenda_bruno.json");
    let deadline = Instant::now() + Duration::from_secs(60);
    while !spec_path.exists() {
        if Instant::now() > deadline {
            let _ = creator.kill();
            let o = creator.wait_with_output().unwrap();
            panic!(
                "creator never woke on the DM\nstdout:\n{}\nstderr:\n{}\nbruno:\n{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr),
                w.try_zoen("bruno", &["read", "@zoen_creator"])
                    .unwrap_or_else(|e| e)
            );
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    read_until(
        &w,
        "bruno",
        "@zoen_creator",
        "Agenda do Bruno",
        &mut creator,
    );
    let spec: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&spec_path).unwrap()).unwrap();
    // Prompt + free JSON state, no booking code.
    assert_eq!(spec["definition"]["state"]["business"], "barbearia");
    assert_eq!(spec["definition"]["model_tier"], "cheap");

    // Bruno's device authorizes the new citizen once; its runtime takes it from there.
    citizen(
        &w,
        "bruno",
        "bot",
        spec["name"].as_str().unwrap(),
        spec["handle"].as_str().unwrap(),
    );
    let bot_def = w.dir.join("bot.json");
    std::fs::write(&bot_def, serde_json::to_vec(&spec["definition"]).unwrap()).unwrap();
    let mut bot = host(&w, "bot", &bot_def);

    // A client books with the bot. Bruno's phone is not running.
    w.init("carla", "Carla");
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

    // Durable: the bot's runtime restarts and still answers what arrived meanwhile.
    bot.kill().unwrap();
    let _ = bot.wait();
    w.zoen("carla", &["send", "@agenda_bruno", "Ainda está de pé?"]);
    let mut bot = host(&w, "bot", &bot_def);
    read_until(&w, "carla", "@agenda_bruno", "continua seu", &mut bot);

    // Both agents are citizens on the relay (migration 0028), each with its own device.
    assert_eq!(w.count("SELECT count(*) FROM agent_citizens").await, 2);
    assert_eq!(
        w.count("SELECT count(DISTINCT device) FROM agent_citizens c JOIN identities i ON i.id = c.agent WHERE i.kind = 'Agent'").await,
        2
    );
    // The DM is end-to-end: the bot read Carla only as an MLS member.
    let carla_chats = w.zoen("carla", &["chats"]);
    assert!(carla_chats.contains("Agenda do Bruno"), "{carla_chats}");

    let _ = creator.kill();
    let _ = bot.kill();
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_joins_a_space_as_a_real_mls_member() {
    let w = World::new("agent_space_member").await;
    w.init("bruno", "Bruno");
    w.init("carla", "Carla");
    w.init("dani", "Dani");

    // The booking bot is a citizen with its own device; its runtime is up before the invite.
    citizen(&w, "bruno", "bot", "Agenda do Bruno", "agenda_bruno");
    let def = zoen_agentd::Definition {
        version: 1,
        kind: zoen_agentd::AgentKind::User,
        persona: "You are Bruno's booking assistant.".into(),
        rules: vec!["Speak Portuguese".into()],
        state: serde_json::json!({"availability": ["sábado 10h"], "greeting": "Oi!"}),
        model_tier: zoen_agentd::ModelTier::Cheap,
    };
    let bot_def = w.dir.join("bot.json");
    std::fs::write(&bot_def, serde_json::to_vec(&def).unwrap()).unwrap();
    let mut bot = host(&w, "bot", &bot_def);

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
    // Every other member opens the agent's ciphertext too.
    read_until(&w, "bruno", "Barbearia", "sábado 10h livre", &mut bot);
    read_until(&w, "dani", "Barbearia", "sábado 10h livre", &mut bot);

    // A second turn from another member keeps working in the same epoch chain.
    w.zoen("dani", &["send", "Barbearia", "Confirmo pra Carla"]);
    read_until(&w, "carla", "Barbearia", "Fechado: sábado 10h", &mut bot);
    let bruno = read_until(&w, "bruno", "Barbearia", "Fechado: sábado 10h", &mut bot);

    // The agent answered only what it could open: the pre-invite "pode criar" never
    // reached its brain (the Creator-style JSON reply would have shown up here).
    assert!(!bruno.contains("Combinado"), "{bruno}");
    let _ = bot.kill();
    let _ = bot.wait();

    // The agent is a citizen with its own MLS device, not a relay-side reader.
    assert_eq!(w.count("SELECT count(*) FROM agent_citizens").await, 1);
}
