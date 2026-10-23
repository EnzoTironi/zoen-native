//! Schedule tool authority and replay through the original real-relay world.
use super::*;
use client_core::{Client, SecretVault, TrustLevelDto};
use std::sync::Arc;

// These are the fixture's existing CLI keys. This adapter only loads them;
// missing keys cannot be manufactured or written by this owner-grant step.
struct ExistingVault(PathBuf);
impl SecretVault for ExistingVault {
    fn load(&self, key: String) -> Option<Vec<u8>> {
        std::fs::read(self.0.join(key)).ok()
    }
    fn save(&self, _key: String, _value: Vec<u8>) -> bool {
        false
    }
    fn delete(&self, _key: String) {
        panic!("owner grant must not delete fixture credentials");
    }
}

async fn grant_booking_actions(w: &World, agent: &str) {
    let home = w.dir.join("bruno");
    let owner = Client::open(
        home.join("zoen.sqlite").to_string_lossy().into_owned(),
        "en".into(),
    )
    .unwrap();
    assert!(owner
        .unlock(Arc::new(ExistingVault(home.join("vault"))))
        .unwrap());
    let space = owner
        .spaces()
        .into_iter()
        .find(|space| space.title == "Barbearia")
        .unwrap();
    owner
        .set_trust(agent.into(), space.id.clone(), TrustLevelDto::Act)
        .unwrap();
    assert_eq!(
        owner
            .agent_profile(agent.into())
            .spaces
            .into_iter()
            .find(|entry| entry.space_id == space.id)
            .unwrap()
            .level,
        TrustLevelDto::Act,
    );
    // Publish the signed GrantIssued over the ordinary MLS/outbox path. The
    // agent's next CLI sync reads that same canonical event before it starts.
    owner.start_sync(None).unwrap();
    owner.wait_until_settled(8_000).await.unwrap();
    owner.stop_sync();
}

/// The first local `weekday` (0 = Sunday) at `hour`:00 in São Paulo (UTC-3) at least a day
/// out, in ms.
fn next_local(weekday: i64, hour: i64) -> i64 {
    const DAY: i64 = 86_400_000;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let mut day = now / DAY + 1;
    // 1970-01-01 was a Thursday (4).
    while (day + 4) % 7 != weekday {
        day += 1;
    }
    day * DAY + (hour + 3) * 3_600_000
}

/// The schedule as `who` sees it in `chat`.
fn schedule_view(w: &World, who: &str, chat: &str) -> serde_json::Value {
    let out = w.zoen(who, &["app", chat, "view"]);
    let line = out
        .lines()
        .find(|l| l.split('\t').nth(1) == Some("schedule"))
        .unwrap_or_else(|| panic!("no schedule in {chat} for {who}:\n{out}"));
    serde_json::from_str(line.splitn(3, '\t').nth(2).unwrap()).unwrap()
}

fn bookings_at(v: &serde_json::Value, start: i64) -> Vec<serde_json::Value> {
    v["bookings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|b| b["startMs"].as_i64() == Some(start))
        .cloned()
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_client_asks_in_chat_and_the_agent_books_with_the_schedule_tool() {
    let w = World::new("agent_tools").await;
    w.init("bruno", "Bruno");
    w.init("carla", "Carla");

    // Prompt + free JSON state; the schedule mini-app is the only booking code.
    let def = zoen_agentd::Definition {
        version: 1,
        kind: zoen_agentd::AgentKind::User,
        persona: "You are Bruno's booking assistant. Book with the schedule tools.".into(),
        rules: vec!["Speak Portuguese".into()],
        state: serde_json::json!({}),
        model_tier: zoen_agentd::ModelTier::Cheap,
    };
    let bot_def = definition_file(&w, "bot.json", &def);
    let agent = citizen(
        &w,
        "bruno",
        "bot",
        "Agenda do Bruno",
        "agenda_bruno",
        &bot_def,
    );
    let mut bot = host(&w, "bot", &[]);
    w.zoen("bruno", &["group", "Barbearia", "@carla"]);
    w.zoen("bruno", &["add", "Barbearia", "@agenda_bruno"]);
    bot.kill().unwrap();
    let _ = bot.wait();

    // The agent opens the shop's schedule in the group: open every day 08:00–20:00.
    w.zoen("bot", &["sync"]);
    let hours: serde_json::Map<String, serde_json::Value> = (0..7)
        .map(|d| (d.to_string(), serde_json::json!([["08:00", "20:00"]])))
        .collect();
    let args = serde_json::json!({"business": "Barbearia do Bruno", "timezone": "America/Sao_Paulo",
        "services": [{"id": "corte", "name": "Corte", "minutes": 30, "priceCents": 5000}],
        "weeklyHours": hours});
    w.zoen(
        "bot",
        &["app", "Barbearia", "install", "schedule", &args.to_string()],
    );

    // Installing an App grants only that Item. The human owner must separately
    // authorize this agent's reversible actions in this Space; no App grant can
    // raise the caller agent's trust or revive an old/revoked grant.
    w.zoen("bruno", &["sync"]);
    grant_booking_actions(&w, &agent).await;
    w.zoen("bot", &["sync"]);

    // A deterministic model that calls tools: it names the slot (and notes what the client
    // asked for, which the gate checks the call against), never the client.
    let sat = next_local(6, 10);
    let sun = next_local(0, 10);
    let book = |start: i64, note: &str, words: &str| {
        format!(
            "{words}\n```tools\n[{{\"name\":\"schedule_book\",\"arguments\":{{\"serviceId\":\"corte\",\"startMs\":{start},\"note\":\"{note}\"}}}}]\n```"
        )
    };
    let script = serde_json::json!({
        "cortar sábado": book(sat, "cortar", "Fechado: sábado 10h, corte!"),
        "domingo": book(sun, "domingo", "Marquei domingo 10h também."),
        "sexta": "Sexta já está cheia.\n```tools\n[{\"name\":\"schedule_book\",\"arguments\":{\"serviceId\":\"corte\",\"startMs\":1,\"forId\":\"someone-else\",\"note\":\"sexta\"}}]\n```",
    })
    .to_string();
    let mut bot = host_script(&w, "bot", &script, &[]);

    w.zoen(
        "carla",
        &["send", "Barbearia", "Oi! quero cortar sábado 10h"],
    );
    let chat = read_until(&w, "carla", "Barbearia", "Fechado: sábado 10h", &mut bot);
    assert_eq!(chat.matches("Fechado: sábado 10h").count(), 1, "{chat}");

    // Carla sees her booking in her own view of the schedule, made out to her.
    let deadline = Instant::now() + Duration::from_secs(60);
    let mine = loop {
        let b = bookings_at(&schedule_view(&w, "carla", "Barbearia"), sat);
        if !b.is_empty() || Instant::now() > deadline {
            break b;
        }
        std::thread::sleep(Duration::from_millis(500));
    };
    assert_eq!(mine.len(), 1, "{mine:?}");
    // Made out to her id. People don't share profile keys with agents, so the agent's
    // device knows her by her @handle until that policy changes.
    assert_eq!(mine[0]["forId"], full_id(&w, "carla"), "{mine:?}");
    let her = |n: &serde_json::Value| n == "Carla" || n == "@carla";
    assert!(her(&mine[0]["forName"]), "{mine:?}");
    assert_eq!(mine[0]["serviceName"], "Corte", "{mine:?}");

    // Bruno gets the "Novo agendamento" note from his own agent.
    let dm = read_until(&w, "bruno", "@agenda_bruno", "\"booking\"", &mut bot);
    let n = note(&dm, "booking").expect("booking note")["booking"].clone();
    assert_eq!(n["kind"], "new", "{n}");
    assert!(her(&n["client"]), "{n}");

    // The model can't book for someone else: the call is refused and nothing is booked.
    w.zoen("carla", &["send", "Barbearia", "E sexta?"]);
    let chat = read_until(&w, "carla", "Barbearia", "Não deu", &mut bot);
    assert!(!chat.contains("Sexta já está cheia"), "{chat}");

    // A crash right after the booking, before the host journals it: the rerun recognises
    // the booking made for this message and confirms it, without booking twice.
    bot.kill().unwrap();
    let _ = bot.wait();
    let marker = w.dir.join("fault-tool");
    let mut bot = host_script(
        &w,
        "bot",
        &script,
        &[
            ("ZOEN_AGENT_FAULT", "before-journal"),
            ("ZOEN_AGENT_FAULT_MARKER", marker.to_str().unwrap()),
        ],
    );
    w.zoen(
        "carla",
        &["send", "Barbearia", "Pode marcar domingo 10h também?"],
    );
    assert_eq!(exits(&mut bot, 60), Some(86), "{}", drain(&mut bot));
    let mut bot = host_script(&w, "bot", &script, &[]);
    let chat = read_until(&w, "carla", "Barbearia", "Marquei domingo 10h", &mut bot);
    assert_eq!(chat.matches("Marquei domingo 10h").count(), 1, "{chat}");
    let view = schedule_view(&w, "carla", "Barbearia");
    assert_eq!(bookings_at(&view, sun).len(), 1, "{view}");
    assert_eq!(bookings_at(&view, sat).len(), 1, "{view}");

    let _ = bot.kill();
}
