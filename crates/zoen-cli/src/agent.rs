//! Server-side agent citizen runtime (`zoen agent-*`).
//!
//! The agent is an ordinary Zoen account of kind Agent: its own root, certified device,
//! MLS leaf and relay session. `agent-host` keeps that session open. The engine's change
//! events wake it; each new message from someone else is one [`Runtime::turn`] (the same
//! one the in-process host runs), answered as an inline reply to the message it read.
//!
//! The relay is the single source of truth: the definition, paused/revoked status and the
//! per-Space cursors come from it and go back to it, one message at a time, only once the
//! message is handled (sent, or deliberately silent or blocked). A reply is idempotent on
//! its source message, so a restart mid-turn neither repeats nor loses one.
//!
//! In a turn the model may call the tools of the mini-apps in that chat (a client asks for
//! Saturday 10h; the agent books it). Every call goes through `agent_call_tool` serving the
//! author of the message, so a booking is only ever for them. Each call that ran is
//! journaled under its source message before anything else happens: a restart mid-turn
//! hands the model what already ran instead of running it twice.

use client_core::{AppCallStatus, Client, ConnectionDto, CoreListener, EntryKind, TimelineEntry};
use gate::{Clef, ClefModel, Gate, HttpTransport};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::Notify;
use zoen_agentd::citizen::testing::{fault, scripted_brain_from_env};
use zoen_agentd::citizen::ModelBrain;
use zoen_agentd::{
    AgentSpec, Brain, Definition, Runtime, ToolCall, ToolDef, ToolHost, ToolStep, Turn,
    TurnContext, TurnOutcome,
};

/// History the brain sees: the Space's last messages before the one it answers.
const HISTORY: usize = 16;
/// A message whose brain call keeps failing is skipped after this many tries.
const MAX_TRIES: u32 = 3;
/// At most this many unanswered messages per Space are answered in one go (after a pause
/// or a long outage); older ones are skipped, never replayed in bulk.
const BACKLOG_CAP: usize = 20;
/// The runtime stops after this many refused or failed state reads in a row (~30 s).
const MAX_STATE_FAILURES: u32 = 6;
/// Source messages whose tool steps the journal keeps (older ones are long answered).
const JOURNAL_KEEP: usize = 200;

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

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

/// Clef (clef-flash) on Workers AI. Without credentials the runtime refuses to start
/// unless `ZOEN_AGENT_GATE=offline` explicitly accepts the hard rules + offline floor.
fn gate() -> Result<Gate, String> {
    if std::env::var("ZOEN_AGENT_GATE").as_deref() == Ok("offline") {
        return Ok(Gate::default());
    }
    match (env("CLOUDFLARE_WORKERS_AI_TOKEN"), env("CLOUDFLARE_ACCOUNT_ID")) {
        (Some(token), Some(account)) => Ok(Gate {
            remote: Some(Box::new(Clef {
                transport: Arc::new(ReqwestTransport(reqwest::Client::new())),
                account,
                token,
                model: ClefModel::Flash,
            })),
            ..Gate::default()
        }),
        _ => Err(
            "no gate: set CLOUDFLARE_WORKERS_AI_TOKEN and CLOUDFLARE_ACCOUNT_ID, or ZOEN_AGENT_GATE=offline"
                .into(),
        ),
    }
}

fn brain() -> Result<Box<dyn Brain>, String> {
    if let Some(scripted) = scripted_brain_from_env() {
        return scripted;
    }
    ModelBrain::workers_ai_from_env()
        .map(|b| Box::new(b) as Box<dyn Brain>)
        .ok_or_else(|| "no model: set CLOUDFLARE_WORKERS_AI_TOKEN and CLOUDFLARE_ACCOUNT_ID".into())
}

/// The engine's events, as a wake-up and a connection state.
#[derive(Default)]
struct Waker {
    notify: Notify,
    online: Mutex<Option<bool>>,
}

impl CoreListener for Waker {
    fn on_change(&self, _space_ids: Vec<String>) {
        self.notify.notify_one();
    }
    fn on_ephemeral(&self, _: String, _: String, _: String, _: String) {}
    fn on_presence(&self, _: String, _: bool) {}
    fn on_connection(&self, status: ConnectionDto) {
        *self.online.lock().unwrap_or_else(|p| p.into_inner()) = Some(status.state == "online");
        self.notify.notify_one();
    }
    fn on_error(&self, _: String) {}
    fn on_profile_changed(&self, _: String) {}
}

fn say(line: impl std::fmt::Display) {
    use std::io::Write;
    println!("{line}");
    let _ = std::io::stdout().flush();
}

/// The agent's first definition, at registration (`agent-init --definition`). Only a
/// plain User agent defines itself; a Creator's kind is the owner's call (`agent-define`).
pub async fn define_first(e: &Arc<Client>, def_path: &str) {
    let def: Definition = read_definition(def_path);
    if def.kind.relay_kind() != "User" {
        say(format_args!(
            "kind {} is set by the owner: agent-define",
            def.kind.relay_kind()
        ));
        return;
    }
    let me = e
        .account()
        .unwrap_or_else(|| crate::die("no account"))
        .identity_id;
    let bytes = def.encode().unwrap_or_else(|err| crate::die(err));
    e.agent_define(me, def.version.max(1), def.kind.relay_kind().into(), bytes)
        .await
        .unwrap_or_else(|err| crate::die(err));
}

pub fn read_definition(path: &str) -> Definition {
    serde_json::from_slice(
        &std::fs::read(path).unwrap_or_else(|err| crate::die(format!("definition: {err}"))),
    )
    .unwrap_or_else(|err| crate::die(format!("definition: {err}")))
}

struct Ctx {
    me: String,
    device: String,
    owner: String,
    runtime: Runtime,
    deckcraft: Option<Arc<zoen_agentd::deckcraft::DeckcraftWorker>>,
    /// Tool steps already run, per source message (`agent-tool-steps.json`).
    journal: std::path::PathBuf,
}

/// Runs the wake loop for `for_secs` (or until revoked).
pub async fn host(e: &Arc<Client>, home: &Path, for_secs: Option<u64>) {
    let runtime = Runtime::new(
        gate().unwrap_or_else(|err| crate::die(err)),
        brain().unwrap_or_else(|err| crate::die(err)),
    );
    let acct = e.account().unwrap_or_else(|| crate::die("no account"));
    let owner = e
        .agent_owner()
        .unwrap_or_else(|| crate::die("this account is not an agent with an owner"));
    let cx = Ctx {
        me: acct.identity_id.clone(),
        device: acct.device_id.clone(),
        owner,
        runtime,
        deckcraft: crate::deckcraft::configured().unwrap_or_else(|error| crate::die(error)),
        journal: home.join("agent-tool-steps.json"),
    };
    let waker = Arc::new(Waker::default());
    e.stop_sync();
    e.start_sync(Some(waker.clone() as Arc<dyn CoreListener>))
        .unwrap_or_else(|err| crate::die(err));
    // Ready only once our key packages are on the relay, so a DM can add our device.
    e.wait_until_idle(15000).await;

    let state = e.agent_state().await.unwrap_or_else(|err| crate::die(err));
    if state.status == "revoked" {
        crate::die("this agent was revoked by its owner");
    }
    let mut cursors: BTreeMap<String, u64> = state.cursors.into_iter().collect();
    // No cursor yet: start from the current head, never replay old traffic.
    let mut fresh = Vec::new();
    for s in e.spaces() {
        if cursors.contains_key(&s.id) {
            continue;
        }
        let head = e
            .timeline(s.id.clone())
            .ok()
            .and_then(|tl| tl.iter().map(|t| t.seq).max())
            .unwrap_or(0);
        cursors.insert(s.id.clone(), head);
        fresh.push((s.id, head));
    }
    for (space, head) in fresh {
        let _ = e.agent_advance(space, head).await;
    }
    let mut known: BTreeSet<String> = cursors.keys().cloned().collect();
    let _ = std::fs::write(home.join("agent-ready"), b"");
    say(format_args!("agent @{} hosting", acct.handle));

    let deadline = for_secs.map(|s| Instant::now() + Duration::from_secs(s));
    let mut tries: BTreeMap<(String, u64), u32> = BTreeMap::new();
    let mut backoff = Duration::from_secs(1);
    let mut offline_since: Option<Instant> = None;
    let mut last_activity = Instant::now();
    let mut failures = 0u32;
    loop {
        if deadline.is_some_and(|d| Instant::now() >= d) {
            break;
        }
        // Reconnect with backoff when the connection is down; a long-quiet session also
        // resubscribes once in a while so a Space opened mid-subscribe isn't missed.
        let online = *waker.online.lock().unwrap_or_else(|p| p.into_inner());
        if online == Some(false) {
            let since = *offline_since.get_or_insert_with(Instant::now);
            if since.elapsed() >= backoff {
                e.stop_sync();
                let _ = e.start_sync(Some(waker.clone() as Arc<dyn CoreListener>));
                offline_since = Some(Instant::now());
                backoff = (backoff * 2).min(Duration::from_secs(60));
            }
        } else {
            offline_since = None;
            backoff = Duration::from_secs(1);
            if last_activity.elapsed() > Duration::from_secs(10) {
                e.stop_sync();
                let _ = e.start_sync(Some(waker.clone() as Arc<dyn CoreListener>));
                last_activity = Instant::now();
            }
        }

        let status = match e.agent_state().await {
            Ok(s) => {
                failures = 0;
                s
            }
            Err(err) => {
                // The relay refuses a revoked runtime device; the engine retries the rest.
                failures += 1;
                if failures >= MAX_STATE_FAILURES {
                    say(format_args!("relay unreachable or refused: {err}"));
                    break;
                }
                wait(&waker, deadline).await;
                continue;
            }
        };
        if status.status == "revoked" {
            say("revoked by the owner");
            break;
        }
        let def = match Definition::decode(&status.definition) {
            Ok(d) => d,
            Err(_) => {
                say("no definition yet");
                wait(&waker, deadline).await;
                continue;
            }
        };
        for (space, seq) in status.cursors {
            let c = cursors.entry(space).or_insert(seq);
            *c = (*c).max(seq);
        }
        if status.status == "paused" {
            // Paused: answer nothing and keep the cursors where they are, so whatever
            // arrives now is answered (capped, through the gate) once the owner resumes.
            wait(&waker, deadline).await;
            continue;
        }

        for s in e.spaces() {
            if known.insert(s.id.clone()) {
                // Joined while running: everything we can read is after our join.
                cursors.entry(s.id.clone()).or_insert(0);
            }
            let Ok(tl) = e.timeline(s.id.clone()) else {
                continue;
            };
            let mut from = cursors.get(&s.id).copied().unwrap_or(0);
            // Our own messages are skipped, but not our own mini-app edits: a booking we
            // made for a client in chat still tells the owner.
            let skip = |t: &TimelineEntry| {
                t.author.id == cx.me && !matches!(t.kind, EntryKind::ItemEdited { .. })
            };
            let pending: Vec<u64> = tl
                .iter()
                .filter(|t| t.seq > from && !skip(t))
                .map(|t| t.seq)
                .collect();
            if pending.len() > BACKLOG_CAP {
                let skip_to = pending[pending.len() - BACKLOG_CAP - 1];
                say(format_args!(
                    "skipping {} old messages",
                    pending.len() - BACKLOG_CAP
                ));
                if e.agent_advance(s.id.clone(), skip_to).await.is_err() {
                    continue;
                }
                cursors.insert(s.id.clone(), skip_to);
                from = skip_to;
            }
            let mut caught_up = true;
            for (i, t) in tl.iter().enumerate() {
                if t.seq <= from || skip(t) {
                    continue;
                }
                last_activity = Instant::now();
                let handled = turn(e, &cx, &def, &s.id, &tl[..i], t).await;
                let key = (s.id.clone(), t.seq);
                if !handled {
                    let n = tries.entry(key).or_insert(0);
                    *n += 1;
                    if *n < MAX_TRIES {
                        caught_up = false;
                        break; // retry this message on the next wake, in order
                    }
                    say(format_args!("giving up on seq {}", t.seq));
                }
                fault("before-advance");
                if e.agent_advance(s.id.clone(), t.seq).await.is_err() {
                    caught_up = false;
                    break;
                }
                cursors.insert(s.id.clone(), t.seq);
            }
            // An agent is a member like any other: once it has processed the chat it says
            // so with the same "read up to" watermark, so groups with agents reach ✓✓ read.
            if caught_up && !pending.is_empty() {
                let _ = e.mark_read(s.id.clone());
            }
        }
        wait(&waker, deadline).await;
    }
    e.wait_until_idle(8000).await;
}

async fn wait(w: &Waker, deadline: Option<Instant>) {
    let mut d = Duration::from_secs(5);
    if let Some(end) = deadline {
        d = d.min(end.saturating_duration_since(Instant::now()));
    }
    let _ = tokio::time::timeout(d, w.notify.notified()).await;
}

fn text_of(t: &TimelineEntry) -> Option<&str> {
    match &t.kind {
        EntryKind::Message { text, .. } => Some(text),
        _ => None,
    }
}

/// One message. `true` once it is handled: replied, or deliberately silent or blocked.
async fn turn(
    e: &Arc<Client>,
    cx: &Ctx,
    def: &Definition,
    space: &str,
    before: &[TimelineEntry],
    t: &TimelineEntry,
) -> bool {
    if let EntryKind::ItemEdited {
        item_id, version, ..
    } = &t.kind
    {
        // A client booked or cancelled in a mini-app: tell the owner, once.
        return match e.booking_note(item_id.clone(), *version) {
            Some(n) if owner_note(e, cx, &t.id).is_none() => {
                let mut n: serde_json::Value = serde_json::from_str(&n).unwrap_or_default();
                n["booking"]["source"] = t.id.clone().into();
                tell_owner(e, cx, n)
            }
            _ => true,
        };
    }
    let Some(text) = text_of(t) else {
        return true;
    };
    // Idempotent on the source: did an earlier run already answer this message?
    let tl = e.timeline(space.to_string()).unwrap_or_default();
    if tl
        .iter()
        .any(|m| m.author.id == cx.me && m.reply_to.as_ref().is_some_and(|q| q.id == t.id))
    {
        return true;
    }
    if let Some(note) = owner_note(e, cx, &t.id) {
        // Told the owner already. A proposal's confirmation may still be owed to the chat
        // (the reply failed after the note went out): send it now, from the note.
        return match note["agent_proposal"]["confirmation"].as_str() {
            Some(c) => reply(e, space, c, &t.id),
            None => true,
        };
    }
    let history: Vec<Turn> = before
        .iter()
        .rev()
        .filter_map(|m| {
            text_of(m).map(|x| Turn {
                space: space.into(),
                from: m.author.id.clone(),
                text: x.to_string(),
            })
        })
        .take(HISTORY)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let inbound = Turn {
        space: space.into(),
        from: t.author.id.clone(),
        text: text.to_string(),
    };
    let tc = TurnContext {
        agent: &cx.me,
        device: &cx.device,
        owner: &cx.owner,
        definition: def,
    };
    let tools = SpaceTools::new(e, cx, space, &t.author.id, &t.id);
    let done = tools.done();
    let out = match cx
        .runtime
        .turn_with_tools(tc, &history, &inbound, &tools, done)
        .await
    {
        Ok(o) => o,
        Err(err) => {
            say(format_args!("turn failed: {err}"));
            return false;
        }
    };
    match out {
        TurnOutcome::Silence => true,
        TurnOutcome::Blocked { reason } => {
            say(format_args!("blocked: {reason}"));
            true
        }
        TurnOutcome::Say { text } => reply(e, space, &text, &t.id),
        TurnOutcome::AskOwner { draft, rule } => tell_owner(
            e,
            cx,
            json!({"ask_owner": {"space": space, "source": t.id, "from": t.author.id,
                                 "draft": draft, "rule": rule}}),
        ),
        TurnOutcome::ProposeAgent { spec, confirmation } => {
            propose(e, cx, space, t, &spec, confirmation.as_deref())
        }
    }
}

fn reply(e: &Arc<Client>, space: &str, text: &str, source: &str) -> bool {
    fault("before-send");
    match e.send_reply(space.into(), text.into(), source.into(), false) {
        Ok(_) => {
            say(format_args!(
                "woke in {} -> replied",
                space.chars().take(10).collect::<String>()
            ));
            true
        }
        Err(err) => {
            say(format_args!("reply failed: {err}"));
            false
        }
    }
}

/// The Creator's proposal goes to the owner's DM for approval on the owner's device; its
/// own words go to the chat it was asked in.
fn propose(
    e: &Arc<Client>,
    cx: &Ctx,
    space: &str,
    t: &TimelineEntry,
    spec: &AgentSpec,
    confirmation: Option<&str>,
) -> bool {
    if !tell_owner(
        e,
        cx,
        json!({"agent_proposal": {"source": t.id, "space": space, "spec": spec,
                                  "confirmation": confirmation}}),
    ) {
        return false;
    }
    say(format_args!("proposed @{}", spec.handle));
    match confirmation {
        Some(c) => reply(e, space, c, &t.id),
        None => true,
    }
}

/// A structured note to the owner (their app renders it), tagged with its source message.
fn tell_owner(e: &Arc<Client>, cx: &Ctx, body: serde_json::Value) -> bool {
    let Ok(dm) = e.start_direct(cx.owner.clone()) else {
        return false;
    };
    let text = format!("```json\n{body}\n```");
    match e.send_message(dm, text) {
        Ok(_) => {
            say("told the owner");
            true
        }
        Err(err) => {
            say(format_args!("owner note failed: {err}"));
            false
        }
    }
}

/// Our earlier note to the owner about `source`, if one went out.
fn owner_note(e: &Arc<Client>, cx: &Ctx, source: &str) -> Option<serde_json::Value> {
    let dm = e.start_direct(cx.owner.clone()).ok()?;
    let needle = format!("\"source\":\"{source}\"");
    e.timeline(dm).unwrap_or_default().iter().find_map(|m| {
        let x = text_of(m).filter(|x| m.author.id == cx.me && x.contains(&needle))?;
        let body = x
            .trim()
            .trim_start_matches("```json")
            .trim_end_matches("```");
        Some(serde_json::from_str(body.trim()).unwrap_or(serde_json::Value::Null))
    })
}

/// The mini-app tools of one chat, for one turn: calls run as the agent serving `serving`.
struct SpaceTools<'a> {
    e: &'a Arc<Client>,
    journal: &'a Path,
    source: &'a str,
    serving: &'a str,
    /// Model-facing name → (item, tool).
    names: Vec<(ToolDef, String, String)>,
    deckcraft: Option<&'a Arc<zoen_agentd::deckcraft::DeckcraftWorker>>,
    deckcraft_items: Vec<String>,
}

impl<'a> SpaceTools<'a> {
    fn new(
        e: &'a Arc<Client>,
        cx: &'a Ctx,
        space: &str,
        serving: &'a str,
        source: &'a str,
    ) -> Self {
        let mut names: Vec<(ToolDef, String, String)> = Vec::new();
        for t in e.agent_tools(space) {
            // Two mini-apps with the same tool: the second is `name_2`, and so on.
            let mut name = t.name.clone();
            let mut n = 1;
            while names.iter().any(|(d, _, _)| d.name == name) {
                n += 1;
                name = format!("{}_{n}", t.name);
            }
            let def = ToolDef {
                name,
                description: format!("{} (mini-app \"{}\")", t.description, t.item_title),
                parameters: t.schema,
            };
            names.push((def, t.item_id, t.name));
        }
        let mut deckcraft_items = Vec::new();
        if cx.deckcraft.is_some() {
            for item in e.items().into_iter().filter(|item| {
                item.space_id == space
                    && item
                        .app
                        .as_ref()
                        .is_some_and(|app| app.app_id == "deckcraft")
            }) {
                let mut offered = Vec::new();
                for mut definition in crate::deckcraft::tools() {
                    let tool = definition.name.clone();
                    let Some(name) = crate::deckcraft::model_name(&tool, &item.id) else {
                        offered.clear();
                        break;
                    };
                    if names.iter().any(|(existing, _, _)| existing.name == name) {
                        offered.clear();
                        break;
                    }
                    definition.name = name;
                    definition.parameters["properties"]["artifactId"]["enum"] = json!([item.id]);
                    definition.description =
                        format!("{} (presentation {:?})", definition.description, item.title);
                    offered.push((definition, item.id.clone(), tool));
                }
                // Keep the gateway's existing 64-tool limit. Offer an entire
                // artifact's set or none; never advertise an unusable partial set.
                if offered.is_empty() || names.len() + offered.len() > 64 {
                    continue;
                }
                deckcraft_items.push(item.id.clone());
                names.extend(offered);
            }
        }
        Self {
            e,
            journal: &cx.journal,
            source,
            serving,
            names,
            deckcraft: cx.deckcraft.as_ref(),
            deckcraft_items,
        }
    }

    fn load(&self) -> Vec<(String, Vec<ToolStep>)> {
        std::fs::read(self.journal)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    /// What an earlier run of this turn already did.
    fn done(&self) -> Vec<ToolStep> {
        self.load()
            .into_iter()
            .find(|(s, _)| s == self.source)
            .map(|(_, steps)| steps)
            .unwrap_or_default()
    }

    fn record(&self, step: ToolStep) {
        let mut all = self.load();
        match all.iter_mut().find(|(s, _)| s == self.source) {
            Some((_, steps)) => steps.push(step),
            None => all.push((self.source.to_string(), vec![step])),
        }
        let drop = all.len().saturating_sub(JOURNAL_KEEP);
        all.drain(..drop);
        let tmp = self.journal.with_extension("tmp");
        let ok = serde_json::to_vec(&all)
            .ok()
            .and_then(|b| std::fs::write(&tmp, b).ok())
            .and_then(|_| std::fs::rename(&tmp, self.journal).ok());
        if ok.is_none() {
            say("tool journal write failed");
        }
    }
}

impl ToolHost for SpaceTools<'_> {
    fn tools(&self) -> Vec<ToolDef> {
        self.names.iter().map(|(d, _, _)| d.clone()).collect()
    }

    fn gate_public_references(
        &self,
        call: &ToolCall,
    ) -> Option<zoen_agentd::PublicArtifactReferences> {
        let (_, item, tool) = self
            .names
            .iter()
            .find(|(definition, _, _)| definition.name == call.name)?;
        if !self.deckcraft_items.contains(item) || !crate::deckcraft::recognizes(tool) {
            return None;
        }
        crate::deckcraft::gate_public_references(
            self.e,
            item,
            self.source,
            self.serving,
            tool,
            &call.arguments,
        )
    }
    fn call(&self, call: &ToolCall) -> Result<String, String> {
        // Binary intents reconcile through the signed native store, never a journal
        // cache that could bypass current access or an ambiguous accepted result.
        if let Some((_, item, tool)) = self
            .names
            .iter()
            .find(|(definition, _, _)| definition.name == call.name)
        {
            if self.deckcraft_items.contains(item) && crate::deckcraft::recognizes(tool) {
                let worker = self
                    .deckcraft
                    .ok_or("The isolated presentation worker is unavailable")?;
                let result = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(crate::deckcraft::call(
                        self.e,
                        worker,
                        item,
                        self.source,
                        self.serving,
                        tool,
                        &call.arguments,
                    ))
                });
                if let Ok(text) = &result {
                    if !self.done().iter().any(|step| {
                        step.call.name == call.name && step.call.arguments == call.arguments
                    }) {
                        fault("before-journal");
                        self.record(ToolStep {
                            call: call.clone(),
                            result: text.clone(),
                            is_error: false,
                        });
                        fault("after-tool");
                    }
                }
                return result;
            }
        }
        // Missing/disabled binary bindings must not fall through to the generic
        // journal cache and report success without fresh native authority.
        if crate::deckcraft::is_model_name(&call.name) {
            return Err("This presentation tool is no longer offered".into());
        }
        // The same call already ran for this message (before a restart): its result, again.
        if let Some(s) = self
            .done()
            .into_iter()
            .find(|s| s.call.name == call.name && s.call.arguments == call.arguments)
        {
            return if s.is_error {
                Err(s.result)
            } else {
                Ok(s.result)
            };
        }
        let Some((_, item, tool)) = self.names.iter().find(|(d, _, _)| d.name == call.name) else {
            return Err(format!("unknown tool: {}", call.name));
        };
        // Stamped with the message it answers, so a call that landed just before a crash is
        // recognised on the rerun instead of being refused as a clash.
        let mut args = call.arguments.clone();
        if !args.is_object() {
            args = json!({});
        }
        args["ref"] = self.source.into();
        let args = args.to_string();
        let res = match self.e.agent_call_tool(item, tool, &args, self.serving) {
            Ok(o) => match o.status {
                AppCallStatus::Done => {
                    let v: serde_json::Value =
                        serde_json::from_str(&o.result_json).unwrap_or_default();
                    if v["isError"].as_bool() == Some(true) {
                        Err(o.result_json)
                    } else {
                        Ok(o.result_json)
                    }
                }
                AppCallStatus::NeedsConfirmation => {
                    Err(format!("needs the owner's confirmation: {}", o.message))
                }
                AppCallStatus::Denied => Err(format!("denied: {}", o.message)),
            },
            Err(err) => Err(err.to_string()),
        };
        // Only calls that changed something need to survive a restart; a failed or
        // read-only call is cheap and safe to run again.
        if res.is_ok() {
            fault("before-journal");
            self.record(ToolStep {
                call: call.clone(),
                result: res.clone().unwrap_or_default(),
                is_error: false,
            });
            fault("after-tool");
        }
        say(format_args!(
            "tool {} -> {}",
            call.name,
            if res.is_ok() { "ok" } else { "refused" }
        ));
        res
    }
}

#[cfg(test)]
pub(crate) async fn exercise_deckcraft_dispatch(
    e: &Arc<Client>,
    worker: Arc<zoen_agentd::deckcraft::DeckcraftWorker>,
    journal: &Path,
    binding: (&str, &str, &str),
    arguments: serde_json::Value,
) {
    use zoen_agentd::citizen::testing::scripted::ScriptedBrain;
    let (item, source, serving) = binding;
    let space = e.item(item.into()).unwrap().space_id;
    let account = e.account().unwrap();
    let call_name = crate::deckcraft::model_name("deckcraft_slide_notes", item).unwrap();
    let replay_arguments = arguments.clone();
    let response = format!(
        "```tools\n{}\n```\nApplied the reviewed notes.",
        json!([{"name":call_name,"arguments":arguments}])
    );
    let cx = Ctx {
        me: account.identity_id,
        device: account.device_id,
        owner: e.agent_owner().unwrap(),
        runtime: Runtime::new(
            Gate::default(),
            Box::new(ScriptedBrain {
                replies: [("reviewed edit".into(), response)].into(),
            }),
        ),
        deckcraft: Some(worker),
        journal: journal.into(),
    };
    assert_eq!(item.len(), 72); // actual deterministic app_binary_create format
    let first_host = SpaceTools::new(e, &cx, &space, serving, source);
    assert_eq!(
        first_host
            .names
            .iter()
            .find(|(tool, _, _)| tool.name == call_name)
            .unwrap()
            .1,
        item
    );
    let before_order: Vec<String> = e
        .items()
        .into_iter()
        .filter(|entry| {
            entry.space_id == space
                && entry
                    .app
                    .as_ref()
                    .is_some_and(|app| app.app_id == "deckcraft")
        })
        .map(|entry| entry.id)
        .collect();
    assert_eq!(before_order, vec![item.to_owned()]);
    let source_document = e.app_binary_read(item.into(), None, None).unwrap();
    let other = e
        .app_binary_create(
            space.clone(),
            "Other deck inserted during this turn".into(),
            source_document.data_base64,
            "other_deck".into(),
        )
        .unwrap();
    let after_order: Vec<String> = e
        .items()
        .into_iter()
        .filter(|entry| {
            entry.space_id == space
                && entry
                    .app
                    .as_ref()
                    .is_some_and(|app| app.app_id == "deckcraft")
        })
        .map(|entry| entry.id)
        .collect();
    assert_eq!(after_order.len(), 2);
    assert_eq!(after_order, vec![other.item_id.clone(), item.to_owned()]);
    let host = SpaceTools::new(e, &cx, &space, serving, source);
    assert_eq!(host.deckcraft_items.len(), 2);
    assert!(host
        .tools()
        .iter()
        .filter(|tool| tool.name == call_name)
        .all(|tool| tool.parameters["properties"]["artifactId"]["enum"] == json!([item])));
    assert_eq!(
        host.names
            .iter()
            .find(|(tool, _, _)| tool.name == call_name)
            .unwrap()
            .1,
        item
    );
    assert_ne!(
        crate::deckcraft::model_name("deckcraft_slide_notes", &other.item_id).unwrap(),
        call_name
    );
    assert!(host.tools().len() <= 64);
    let mut mismatched = replay_arguments.clone();
    mismatched["artifactId"] = other.item_id.clone().into();
    assert!(host
        .call(&ToolCall {
            id: "wrong_full_artifact".into(),
            name: call_name.clone(),
            arguments: mismatched
        })
        .is_err());
    assert!(host.done().is_empty());
    assert_eq!(
        e.app_binary_read(item.into(), None, None).unwrap().version,
        source_document.version
    );
    assert_eq!(
        e.app_binary_read(other.item_id.clone(), None, None)
            .unwrap(),
        other
    );
    let definition: Definition = serde_json::from_value(json!({"version":1,"kind":"user","persona":"Update only the reviewed presentation.","rules":[],"state":{}})).unwrap();
    let context = zoen_agentd::TurnContext {
        agent: &cx.me,
        device: &cx.device,
        owner: &cx.owner,
        definition: &definition,
    };
    let source_entry = e
        .timeline(space.clone())
        .unwrap()
        .into_iter()
        .find(|entry| entry.id == source)
        .unwrap();
    let EntryKind::Message { text, .. } = source_entry.kind else {
        panic!("signed message source")
    };
    let inbound = Turn {
        space,
        from: source_entry.author.id,
        text,
    };
    let outcome = cx
        .runtime
        .turn_with_tools(context, &[], &inbound, &host, vec![])
        .await
        .unwrap();
    assert!(
        matches!(&outcome, zoen_agentd::TurnOutcome::Say { text } if text == "Applied the reviewed notes."),
        "actual Runtime outcome: {outcome:?}; durable steps: {:?}",
        host.done()
    );
    let steps = host.done();
    assert_eq!(steps.len(), 1);
    assert!(!steps[0].is_error);
    assert_eq!(steps[0].call.name, call_name);
    let repeated = host
        .call(&ToolCall {
            id: "retained_call".into(),
            name: call_name,
            arguments: replay_arguments,
        })
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&repeated).unwrap()["replayed"],
        true
    );
    assert_eq!(host.done().len(), 1);
    assert_eq!(
        e.app_binary_read(other.item_id.clone(), None, None)
            .unwrap(),
        other
    );
    let accepted: serde_json::Value = serde_json::from_str(&steps[0].result).unwrap();
    assert_eq!(accepted["authorId"], cx.me);
    assert_eq!(accepted["replayed"], false);
}

#[cfg(test)]
mod deckcraft_binding_tests {
    use super::*;
    #[test]
    fn unoffered_binary_binding_cannot_replay_a_cached_success_without_authority() {
        let e = Client::open(":memory:".into(), "en".into()).unwrap();
        let dir = std::env::temp_dir().join(types::new_id("worker-unoffered"));
        std::fs::create_dir_all(&dir).unwrap();
        let journal = dir.join("steps.json");
        let item = format!("it_deck_{}", "ab".repeat(32));
        let call = ToolCall {
            id: "retained".into(),
            name: crate::deckcraft::model_name("deckcraft_text_set", &item).unwrap(),
            arguments: json!({"artifactId":item,"version":1,"sha256":"00".repeat(32),"slideId":257,"id":258,"text":"retained"}),
        };
        let host = SpaceTools {
            e: &e,
            journal: &journal,
            source: "prior-source",
            serving: "prior-member",
            names: vec![],
            deckcraft: None,
            deckcraft_items: vec![],
        };
        host.record(ToolStep {
            call: call.clone(),
            result: json!({"itemId":item,"version":2,"replayed":false}).to_string(),
            is_error: false,
        });
        let before = std::fs::read(&journal).unwrap();
        assert_eq!(host.done().len(), 1);
        assert!(host.call(&call).is_err());
        assert_eq!(std::fs::read(&journal).unwrap(), before);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[tokio::test]
    async fn runtime_public_references_preserve_gate_on_semantics_and_exact_mismatch() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use zoen_agentd::citizen::testing::scripted::ScriptedBrain;
        struct CheckedHost {
            definition: ToolDef,
            references: Option<zoen_agentd::PublicArtifactReferences>,
            calls: AtomicUsize,
        }
        impl ToolHost for CheckedHost {
            fn tools(&self) -> Vec<ToolDef> {
                vec![self.definition.clone()]
            }
            fn gate_public_references(
                &self,
                _: &ToolCall,
            ) -> Option<zoen_agentd::PublicArtifactReferences> {
                self.references.clone()
            }
            fn call(&self, _: &ToolCall) -> Result<String, String> {
                self.calls.fetch_add(1, Ordering::SeqCst);
                Ok("accepted".into())
            }
        }
        let item = format!("it_deck_{}", "a1".repeat(32));
        let sha = "c2".repeat(32);
        let name = crate::deckcraft::model_name("deckcraft_slide_notes", &item).unwrap();
        let base = json!({"artifactId":item,"version":1,"sha256":sha,"slideId":257,"text":"Reviewed presentation notes"});
        let definition: Definition = serde_json::from_value(json!({"version":1,"kind":"user","persona":"Edit reviewed notes","rules":[],"state":{}})).unwrap();
        let cases = [
            (
                "exact public references",
                base.clone(),
                "Presentation notes".to_string(),
                true,
                1,
            ),
            (
                "default raw call still blocks",
                base.clone(),
                "Presentation notes".to_string(),
                false,
                0,
            ),
            (
                "payload secret retained",
                {
                    let mut a = base.clone();
                    a["text"] = format!("sk-test-{}", "a1".repeat(24)).into();
                    a
                },
                "Presentation notes".to_string(),
                true,
                0,
            ),
            (
                "destructive payload retained",
                {
                    let mut a = base.clone();
                    a["text"] = "rm -rf ~/Documents".into();
                    a
                },
                "Presentation notes".to_string(),
                true,
                0,
            ),
            (
                "title retained",
                base.clone(),
                format!("Presentation sk-test-{}", "a1".repeat(24)),
                true,
                0,
            ),
            (
                "wrong artifact not stripped",
                {
                    let mut a = base.clone();
                    a["artifactId"] = format!("it_deck_{}", "e3".repeat(32)).into();
                    a
                },
                "Presentation notes".to_string(),
                true,
                0,
            ),
            (
                "wrong hash not stripped",
                {
                    let mut a = base.clone();
                    a["sha256"] = "e3".repeat(32).into();
                    a
                },
                "Presentation notes".to_string(),
                true,
                0,
            ),
            (
                "nested public-looking payload retained",
                {
                    let mut a = base.clone();
                    a["text"] = format!("Reference {}", item).into();
                    a
                },
                "Presentation notes".to_string(),
                true,
                0,
            ),
        ];
        for (label, arguments, description, use_refs, expected_calls) in cases {
            let host = CheckedHost {
                definition: ToolDef {
                    name: name.clone(),
                    description,
                    parameters: json!({}),
                },
                references: use_refs.then(|| {
                    zoen_agentd::PublicArtifactReferences::new(
                        "deckcraft_slide_notes".into(),
                        item.clone(),
                        Some(sha.clone()),
                    )
                    .unwrap()
                }),
                calls: AtomicUsize::new(0),
            };
            let response = format!(
                "```tools\n{}\n```\nApplied notes.",
                json!([{"name":name,"arguments":arguments}])
            );
            let runtime = Runtime::new(
                Gate::default(),
                Box::new(ScriptedBrain {
                    replies: [("reviewed edit".into(), response)].into(),
                }),
            );
            let inbound = Turn {
                space: "space".into(),
                from: "owner".into(),
                text: "Update presentation notes with the reviewed edits".into(),
            };
            let outcome = runtime
                .turn_with_tools(
                    TurnContext {
                        agent: "agent",
                        device: "device",
                        owner: "owner",
                        definition: &definition,
                    },
                    &[],
                    &inbound,
                    &host,
                    vec![],
                )
                .await
                .unwrap();
            assert_eq!(
                host.calls.load(Ordering::SeqCst),
                expected_calls,
                "{label}: {outcome:?}"
            );
            if expected_calls == 1 {
                assert_eq!(
                    outcome,
                    TurnOutcome::Say {
                        text: "Applied notes.".into()
                    },
                    "{label}"
                );
            }
        }
    }
}
