//! Server-side agent citizen runtime (`zoen agent-*`).
//!
//! The agent is an ordinary Zoen account of kind Agent: its own root, certified device,
//! MLS leaf and relay session. `agent-host` keeps that session open; the relay's live
//! fan-out delivers (and MLS-decrypts) new messages, and each one wakes a turn: gate
//! inbound → brain (prompt + free JSON state) → gate outbound → send as the agent.
//! A per-Space cursor in the agent's home makes turns survive restarts.

use roda_ffi::{EntryKind, RodaEngine};
use roda_gate::{Check, Clef, ClefModel, Gate, HttpTransport, Outcome, Point};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use zoen_agentd::citizen::{BrainOut, ModelBrain};
use zoen_agentd::{Brain, Definition, ScriptedBrain, Turn};

struct ReqwestTransport(reqwest::Client);
impl HttpTransport for ReqwestTransport {
    fn post_json(&self, url: &str, bearer: Option<&str>, body: &str) -> Result<String, String> {
        let mut rq = self
            .0
            .post(url)
            .header("content-type", "application/json")
            .body(body.to_string());
        if let Some(b) = bearer {
            rq = rq.bearer_auth(b);
        }
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let r = rq.send().await.map_err(|_| "transport".to_string())?;
                r.text().await.map_err(|_| "body".to_string())
            })
        })
    }
}

/// Clef (clef-flash) when Workers AI is configured, else hard rules + offline floor.
fn gate() -> Gate {
    let token = std::env::var("CLOUDFLARE_WORKERS_AI_TOKEN")
        .ok()
        .filter(|t| !t.is_empty());
    let account = std::env::var("CLOUDFLARE_ACCOUNT_ID")
        .ok()
        .filter(|t| !t.is_empty());
    match (token, account, std::env::var("ZOEN_AGENT_GATE").as_deref()) {
        (Some(token), Some(account), r) if r != Ok("offline") => Gate {
            remote: Some(Box::new(Clef {
                transport: Arc::new(ReqwestTransport(reqwest::Client::new())),
                account,
                token,
                model: ClefModel::Flash,
            })),
            ..Gate::default()
        },
        _ => Gate::default(),
    }
}

#[allow(clippy::large_enum_variant)]
enum AnyBrain {
    Scripted(ScriptedBrain),
    Model(ModelBrain),
}

fn brain() -> Result<AnyBrain, String> {
    if let Ok(path) = std::env::var("ZOEN_AGENT_SCRIPT") {
        let text = std::fs::read_to_string(&path).map_err(|e| format!("script: {e}"))?;
        let replies: BTreeMap<String, String> =
            serde_json::from_str(&text).map_err(|e| format!("script: {e}"))?;
        return Ok(AnyBrain::Scripted(ScriptedBrain { replies }));
    }
    ModelBrain::workers_ai_from_env().map(AnyBrain::Model).ok_or_else(|| {
        "no model: set CLOUDFLARE_WORKERS_AI_TOKEN and CLOUDFLARE_ACCOUNT_ID (or ZOEN_AGENT_SCRIPT)".into()
    })
}

fn load_cursors(home: &Path) -> BTreeMap<String, u64> {
    std::fs::read(home.join("agent-cursors.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save_cursors(home: &Path, c: &BTreeMap<String, u64>) {
    let tmp = home.join("agent-cursors.json.tmp");
    if std::fs::write(&tmp, serde_json::to_vec(c).unwrap_or_default()).is_ok() {
        let _ = std::fs::rename(tmp, home.join("agent-cursors.json"));
    }
}

/// Runs the wake loop for `for_secs` (or forever).
pub async fn host(e: &Arc<RodaEngine>, home: &Path, def_path: &str, for_secs: Option<u64>) {
    let def: Definition = serde_json::from_slice(
        &std::fs::read(def_path).unwrap_or_else(|err| crate::die(format!("definition: {err}"))),
    )
    .unwrap_or_else(|err| crate::die(format!("definition: {err}")));
    let brain = brain().unwrap_or_else(|err| crate::die(err));
    let gate = gate();
    let acct = e.account().unwrap_or_else(|| crate::die("no account"));
    let me = acct.identity_id.clone();
    let mut cursors = load_cursors(home);
    let mut history: BTreeMap<String, Vec<Turn>> = BTreeMap::new();
    let deadline = for_secs.map(|s| Instant::now() + Duration::from_secs(s));
    // Ready only once our key packages are on the relay, so a DM can add our device.
    e.wait_until_idle(15000).await;
    let _ = std::fs::write(home.join("agent-ready"), b"");
    println!("agent @{} hosting ({:?})", acct.handle, def.kind);
    use std::io::Write;
    let _ = std::io::stdout().flush();
    // The live push wakes us; a quiet reconnect every 10s catches anything it missed
    // (e.g. a DM opened while the connection was still subscribing).
    let mut last_activity = Instant::now();
    while deadline.is_none_or(|d| Instant::now() < d) {
        if last_activity.elapsed() > Duration::from_secs(10) {
            e.stop_sync();
            let _ = e.start_sync(None);
            last_activity = Instant::now();
        }
        for s in e.spaces() {
            let Ok(tl) = e.timeline(s.id.clone()) else {
                continue;
            };
            let last = cursors.get(&s.id).copied().unwrap_or(0);
            for t in tl.iter().filter(|t| t.seq > last) {
                cursors.insert(s.id.clone(), t.seq);
                last_activity = Instant::now();
                let EntryKind::Message { text, .. } = &t.kind else {
                    continue;
                };
                let turn = Turn {
                    space: s.id.clone(),
                    from: t.author.id.clone(),
                    text: text.clone(),
                };
                let h = history.entry(s.id.clone()).or_default();
                if t.author.id == me {
                    h.push(turn);
                    continue;
                }
                let reply =
                    turn_reply(&gate, &brain, &def, &me, &acct.device_id, h, &turn, home).await;
                h.push(turn);
                if let Some(text) = reply {
                    match e.send_message(s.id.clone(), text.clone()) {
                        Ok(_) => println!("woke in {} -> replied", &s.id[..s.id.len().min(10)]),
                        Err(err) => println!("reply failed: {err}"),
                    }
                    h.push(Turn {
                        space: s.id.clone(),
                        from: me.clone(),
                        text,
                    });
                    let _ = std::io::stdout().flush();
                }
            }
            save_cursors(home, &cursors);
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    e.wait_until_idle(8000).await;
}

#[allow(clippy::too_many_arguments)]
async fn turn_reply(
    gate: &Gate,
    brain: &AnyBrain,
    def: &Definition,
    me: &str,
    device: &str,
    history: &[Turn],
    inbound: &Turn,
    home: &Path,
) -> Option<String> {
    if gate
        .check(&Check::new(Point::Inbound, &inbound.text))
        .outcome
        == Outcome::Block
    {
        println!("inbound blocked");
        return None;
    }
    let out = match brain {
        AnyBrain::Scripted(b) => b.think(def, history, inbound),
        AnyBrain::Model(m) => match m
            .think(&inbound.from, me, device, def, history, inbound)
            .await
        {
            Ok(o) => o,
            Err(err) => {
                println!("model: {err}");
                return None;
            }
        },
    };
    let text = match out {
        BrainOut::Say(t) => t,
        BrainOut::CreateAgent {
            name,
            handle,
            persona,
            rules,
            state,
            glyph,
            tint_hex,
        } => {
            // The owner's device must authorize the new citizen (ADR 0049); the
            // runtime hands over the prompt-only definition for that approval.
            let dir = home.join("created");
            let _ = std::fs::create_dir_all(&dir);
            let spec = serde_json::json!({
                "name": name, "handle": handle, "glyph": glyph, "tint_hex": tint_hex,
                "definition": Definition {
                    version: 1, kind: zoen_agentd::AgentKind::User, persona, rules, state,
                    model_tier: zoen_agentd::ModelTier::Cheap,
                },
            });
            let _ = std::fs::write(dir.join(format!("{handle}.json")), spec.to_string());
            println!("created spec @{handle}");
            format!("Pronto: {name} (@{handle}) está criado. Autorize no seu app para ele entrar no Zoen.")
        }
    };
    match gate.check(&Check::new(Point::Outbound, &text)).outcome {
        Outcome::Allow => Some(text),
        o => {
            println!("outbound {}", o.id());
            None
        }
    }
}
