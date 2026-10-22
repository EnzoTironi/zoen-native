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
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::Notify;
use zoen_agentd::citizen::testing::{fault, scripted_brain_from_env};
use zoen_agentd::citizen::{CodexBrain, ModelBrain};
use zoen_agentd::{
    AgentSpec, ApprovedToolAdmission, Brain, Definition, Runtime, ToolAsk, ToolCall, ToolDef,
    ToolHost, ToolProgress, ToolResult, ToolStep, Turn, TurnContext, TurnOutcome,
};

mod startup;
mod trace;
pub(crate) use startup::Readiness;

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

// Match the existing Workers AI ModelBrain envelope/budgets (citizen/models.rs).
// Clef returns typed question answers inside its JSON envelope, not binary data.
const GATE_REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
const GATE_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const GATE_MAX_RESPONSE_BYTES: usize = 1024 * 1024;

struct ReqwestTransport {
    client: reqwest::Client,
    max_response_bytes: usize,
}
impl ReqwestTransport {
    fn new() -> Result<Self, String> {
        Self::with_limits(
            reqwest::Client::builder(),
            GATE_REQUEST_TIMEOUT,
            GATE_CONNECT_TIMEOUT,
            GATE_MAX_RESPONSE_BYTES,
        )
    }

    fn with_limits(
        builder: reqwest::ClientBuilder,
        timeout: Duration,
        connect_timeout: Duration,
        max_response_bytes: usize,
    ) -> Result<Self, String> {
        let client = builder
            .connect_timeout(connect_timeout)
            .read_timeout(timeout)
            .timeout(timeout)
            .build()
            .map_err(|_| "transport configuration".to_string())?;
        Ok(Self {
            client,
            max_response_bytes,
        })
    }
}
impl HttpTransport for ReqwestTransport {
    fn post_json(&self, url: &str, bearer: Option<&str>, body: &str) -> Result<String, String> {
        let mut rq = self
            .client
            .post(url)
            .header("content-type", "application/json")
            .body(body.to_string());
        if let Some(b) = bearer {
            rq = rq.bearer_auth(b);
        }
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let mut response = rq.send().await.map_err(|_| "transport".to_string())?;
                if response
                    .content_length()
                    .is_some_and(|length| length > self.max_response_bytes as u64)
                {
                    return Err("response too large".to_string());
                }
                let mut bytes = Vec::new();
                while let Some(chunk) = response.chunk().await.map_err(|_| "body".to_string())? {
                    if chunk.len() > self.max_response_bytes.saturating_sub(bytes.len()) {
                        return Err("response too large".to_string());
                    }
                    bytes.extend_from_slice(&chunk);
                }
                // Retain reqwest::Response::text's UTF-8/lossy behavior with
                // this workspace's charset-disabled reqwest configuration.
                Ok(String::from_utf8_lossy(&bytes).into_owned())
            })
        })
    }
}

#[cfg(test)]
#[path = "agent/gate_transport_tests.rs"]
mod gate_transport_tests;

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
                transport: Arc::new(ReqwestTransport::new()?),
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
    match env("ZOEN_BRAIN").as_deref() {
        Some("codex-local") => CodexBrain::from_env().map(|b| Box::new(b) as Box<dyn Brain>),
        None | Some("workers-ai") => ModelBrain::workers_ai_from_env()
            .map(|b| Box::new(b) as Box<dyn Brain>)
            .ok_or_else(|| {
                "no model: set CLOUDFLARE_WORKERS_AI_TOKEN and CLOUDFLARE_ACCOUNT_ID".into()
            }),
        Some(_) => Err("unsupported ZOEN_BRAIN; use workers-ai or codex-local".into()),
    }
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
pub async fn host(
    e: &Arc<Client>,
    home: &Path,
    readiness: &Readiness<'_>,
    for_secs: Option<u64>,
) -> Result<(), String> {
    let runtime = Runtime::new(gate()?, brain()?);
    let acct = e.account().ok_or("no account")?;
    let owner = e
        .agent_owner()
        .ok_or("this account is not an agent with an owner")?;
    let cx = Ctx {
        me: acct.identity_id.clone(),
        device: acct.device_id.clone(),
        owner,
        runtime,
        deckcraft: crate::deckcraft::configured()?,
        journal: home.join("agent-tool-steps.json"),
    };
    let waker = Arc::new(Waker::default());
    e.stop_sync();
    e.start_sync(Some(waker.clone() as Arc<dyn CoreListener>))
        .map_err(|error| error.to_string())?;
    let mut cursors = startup::prepare(e, readiness, 15000).await?;
    let mut known: BTreeSet<String> = cursors.keys().cloned().collect();
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
            // Canonical Requests survive cursor advancement and cold restart. An
            // owner's resolution resumes the original source, never a new model task.
            let continuations = e.approved_tool_sources(&s.id);
            for source in continuations {
                if let Some(index) = tl.iter().position(|entry| entry.id == source) {
                    let _ = turn(e, &cx, &def, &s.id, &tl[..index], &tl[index]).await;
                }
            }
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
    Ok(())
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
                tell_owner(e, cx, n, space, &t.id)
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
        trace::stop(space, &t.id, trace::Stop::AlreadyReplied, None);
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
    let pending = e.tool_approval_requests(space, &t.id);
    let resumed = pending.last();
    let result = if let Some(request) = resumed {
        let stop = |category| trace::stop(space, &t.id, category, Some(&request.id));
        let Some(dto) = e.requests().into_iter().find(|r| r.id == request.id) else {
            stop(trace::Stop::RequestUnavailable);
            return true;
        };
        if let Some(category) = trace::Stop::request_status(dto.status) {
            stop(category);
            return true;
        }
        let Some(worker) = tools.deckcraft else {
            stop(trace::Stop::ExecutorUnavailable);
            return true;
        };
        let Some(proposal) = &request.tool_proposal else {
            stop(trace::Stop::InvalidContinuation);
            return true;
        };
        let ask: ToolAsk = match serde_json::from_str(&proposal.continuation_json) {
            Ok(ask) => ask,
            Err(_) => {
                stop(trace::Stop::InvalidContinuation);
                return true;
            }
        };
        let checkpoint = match tools.recovery_checkpoint(&request.id, &request.content_hash) {
            Ok(checkpoint) => checkpoint,
            Err(error) => {
                say(format_args!("continuation checkpoint refused: {error}"));
                stop(trace::Stop::CheckpointRefused);
                return true;
            }
        };
        let recovered = if let Some((mut progress, prepared)) = checkpoint {
            if let Some(prepared) = prepared {
                if prepared.envelope != ask.envelope
                    || prepared.policy_sha256 != ask.policy_sha256
                    || prepared.round < progress.round
                    || prepared.steps != progress.steps
                    || tools
                        .tools()
                        .iter()
                        .find(|definition| definition.name == prepared.call.name)
                        != Some(&prepared.definition)
                {
                    say("prepared continuation changed");
                    stop(trace::Stop::InvalidContinuation);
                    return true;
                }
                progress = ToolProgress {
                    round: prepared.round,
                    steps: prepared.steps,
                    remaining: prepared.remaining,
                };
                progress.steps.push(ToolStep {
                    call: prepared.call,
                    result: String::new(),
                    is_error: false,
                });
            }
            let encoded = match serde_json::to_string(&progress.steps) {
                Ok(encoded) => encoded,
                Err(_) => {
                    stop(trace::Stop::InvalidContinuation);
                    return true;
                }
            };
            let verified = match e.recover_tool_results(
                &request.id,
                worker.binding_sha256(),
                &encoded,
            ) {
                Ok(results) => results,
                Err(_) => {
                    say(
                        "completed continuation is stale, unauthorized or lacks a canonical receipt",
                    );
                    stop(trace::Stop::RecoveryRefused);
                    return true;
                }
            };
            progress.steps = match serde_json::from_str(&verified) {
                Ok(steps) => steps,
                Err(_) => {
                    stop(trace::Stop::InvalidContinuation);
                    return true;
                }
            };
            Some(progress)
        } else {
            None
        };
        // Both fresh execution and completed-result recovery retain the same
        // persistent operational attempt budget. This is not a provider meter.
        match tools.begin_resume(&request.id, &request.content_hash) {
            Ok(true) => {}
            Ok(false) => {
                say("reviewed action reached its continuation retry limit");
                stop(trace::Stop::AttemptLimit);
                return true;
            }
            Err(error) => {
                say(format_args!("continuation reservation refused: {error}"));
                stop(trace::Stop::ReservationRefused);
                return true;
            }
        }
        if let Some(progress) = recovered {
            trace::progress(space, &t.id, &request.id, &progress);
            cx.runtime
                .resume_tool_progress(tc, &history, &inbound, &tools, ask, progress)
                .await
        } else {
            let permit = match e.approved_tool_turn(&request.id, worker.binding_sha256()) {
                Ok(permit) => permit,
                Err(_) => {
                    say("reviewed action is stale or no longer authorized");
                    stop(trace::Stop::ApprovalRefused);
                    return true;
                }
            };
            let admission = CanonicalToolAdmission {
                tools: &tools,
                permit,
            };
            trace::approved(space, &t.id, &request.id, &ask);
            cx.runtime
                .resume_tool_ask(tc, &history, &inbound, &tools, ask, &admission)
                .await
        }
    } else {
        trace::normal(space, &t.id, &done);
        cx.runtime
            .turn_with_tools(tc, &history, &inbound, &tools, done)
            .await
    };
    trace::turn_result(space, &t.id, &result);
    let out = match result {
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
        TurnOutcome::ToolAsk { ask } => match tools.stage_approval(&ask) {
            Ok(dto) => {
                trace::staged(space, &t.id, &ask, &dto);
                true
            }
            Err(error) => {
                trace::delivery(space, &t.id, trace::Delivery::ApprovalStaging, false);
                say(format_args!("tool approval refused: {error}"));
                true
            }
        },
        TurnOutcome::AskOwner { draft, rule } => tell_owner(
            e,
            cx,
            json!({"ask_owner": {"space": space, "source": t.id, "from": t.author.id,
                                 "draft": draft, "rule": rule}}),
            space,
            &t.id,
        ),
        TurnOutcome::ProposeAgent { spec, confirmation } => {
            propose(e, cx, space, t, &spec, confirmation.as_deref())
        }
    }
}

fn reply(e: &Arc<Client>, space: &str, text: &str, source: &str) -> bool {
    fault("before-send");
    let result = e.send_reply(space.into(), text.into(), source.into(), false);
    trace::delivery(space, source, trace::Delivery::Reply, result.is_ok());
    match result {
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
        space,
        &t.id,
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
fn tell_owner(
    e: &Arc<Client>,
    cx: &Ctx,
    body: serde_json::Value,
    space: &str,
    source: &str,
) -> bool {
    let Ok(dm) = e.start_direct(cx.owner.clone()) else {
        trace::delivery(space, source, trace::Delivery::OwnerNote, false);
        return false;
    };
    let text = format!("```json\n{body}\n```");
    let result = e.send_message(dm, text);
    trace::delivery(space, source, trace::Delivery::OwnerNote, result.is_ok());
    match result {
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

/// Operational recovery data in the existing tool journal. It is neither an approval
/// nor a provider billing record; canonical native Requests remain the only authority.
#[derive(Default)]
struct JournalState {
    steps: Vec<(String, Vec<ToolStep>)>,
    continuations: BTreeMap<String, ResumeAttempts>,
}
struct ResumeAttempts {
    content_hash: String,
    attempts: u32,
    source: Option<String>,
    prepared: Option<ToolAsk>,
    progress: Option<ToolProgress>,
}
impl JournalState {
    fn valid_hash(hash: &str) -> bool {
        hash.len() == 64
            && hash
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    }
    fn load(path: &Path) -> Result<Self, String> {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(_) => return Err("The tool journal could not be read".into()),
        };
        let value: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|_| "The tool journal is corrupt")?;
        if value.is_array() {
            return Ok(Self {
                steps: serde_json::from_value(value).map_err(|_| "The tool journal is corrupt")?,
                continuations: BTreeMap::new(),
            });
        }
        let root = value.as_object().ok_or("The tool journal is corrupt")?;
        if root.len() != 3 || root.get("version").and_then(serde_json::Value::as_u64) != Some(1) {
            return Err("The tool journal version is unsupported".into());
        }
        let steps = serde_json::from_value(
            root.get("steps")
                .cloned()
                .ok_or("The tool journal is corrupt")?,
        )
        .map_err(|_| "The tool journal is corrupt")?;
        let metadata = root
            .get("continuations")
            .and_then(serde_json::Value::as_object)
            .ok_or("The tool journal is corrupt")?;
        let mut continuations = BTreeMap::new();
        for (request, budget) in metadata {
            let budget = budget
                .as_object()
                .filter(|budget| {
                    budget.keys().all(|key| {
                        matches!(
                            key.as_str(),
                            "content_hash" | "attempts" | "source" | "prepared" | "progress"
                        )
                    })
                })
                .ok_or("The continuation budget is corrupt")?;
            let content_hash = budget
                .get("content_hash")
                .and_then(serde_json::Value::as_str)
                .filter(|hash| Self::valid_hash(hash))
                .ok_or("The continuation budget is corrupt")?;
            let attempts = budget
                .get("attempts")
                .and_then(serde_json::Value::as_u64)
                .filter(|attempts| *attempts > 0 && *attempts <= MAX_TRIES as u64)
                .ok_or("The continuation budget is corrupt")? as u32;
            if request.is_empty() || request.len() > 128 {
                return Err("The continuation request is invalid".into());
            }
            let source = match budget.get("source") {
                None | Some(serde_json::Value::Null) => None,
                Some(serde_json::Value::String(source))
                    if !source.is_empty() && source.len() <= 128 =>
                {
                    Some(source.clone())
                }
                _ => return Err("The continuation source is corrupt".into()),
            };
            let prepared: Option<ToolAsk> = budget
                .get("prepared")
                .filter(|value| !value.is_null())
                .map(|value| {
                    serde_json::from_value(value.clone())
                        .map_err(|_| "The prepared tool is corrupt")
                })
                .transpose()?;
            let progress: Option<ToolProgress> = budget
                .get("progress")
                .filter(|value| !value.is_null())
                .map(|value| {
                    serde_json::from_value(value.clone())
                        .map_err(|_| "The completed progress is corrupt")
                })
                .transpose()?;
            if prepared.as_ref().is_some_and(|ask| {
                ask.version != 1
                    || ask.round >= 4
                    || ask.steps.len() >= 16
                    || ask.remaining.len() > 3
                    || ask.steps.len() + 1 + ask.remaining.len() > 16
            }) || progress.as_ref().is_some_and(|progress| {
                progress.round >= 4
                    || progress.steps.is_empty()
                    || progress.steps.len() > 16
                    || progress.remaining.len() > 3
                    || progress.steps.len() + progress.remaining.len() > 16
            }) {
                return Err("The continuation progress budget is corrupt".into());
            }
            if prepared.as_ref().is_some_and(|ask| {
                serde_json::to_vec(ask).map_or(true, |bytes| {
                    bytes.len() > zoen_agentd::MAX_TOOL_CONTINUATION_BYTES
                })
            }) || progress.as_ref().is_some_and(|progress| {
                serde_json::to_vec(progress).map_or(true, |bytes| {
                    bytes.len() > zoen_agentd::MAX_TOOL_CONTINUATION_BYTES
                })
            }) {
                return Err("The continuation progress exceeds its capacity".into());
            }
            continuations.insert(
                request.clone(),
                ResumeAttempts {
                    content_hash: content_hash.into(),
                    attempts,
                    source,
                    prepared,
                    progress,
                },
            );
        }
        Ok(Self {
            steps,
            continuations,
        })
    }
    fn bytes(&self) -> Result<Vec<u8>, String> {
        if self.continuations.is_empty() {
            return serde_json::to_vec(&self.steps)
                .map_err(|_| "The tool journal could not be encoded".into());
        }
        let continuations: serde_json::Map<String, serde_json::Value> = self
            .continuations
            .iter()
            .map(|(request, budget)| {
                (
                    request.clone(),
                    json!({"content_hash":budget.content_hash,"attempts":budget.attempts,"source":budget.source,"prepared":budget.prepared,"progress":budget.progress}),
                )
            })
            .collect();
        serde_json::to_vec(&json!({"version":1,"steps":self.steps,"continuations":continuations}))
            .map_err(|_| "The tool journal could not be encoded".into())
    }
    /// Reserve the temp path before reading the old state so simultaneous local writers
    /// cannot overwrite a newer retry count with a stale snapshot. A leftover temp file
    /// after interruption refuses another attempt until the host/operator reconciles it.
    fn update(
        path: &Path,
        apply: impl FnOnce(&mut Self) -> Result<bool, String>,
    ) -> Result<bool, String> {
        let temporary = path.with_extension("tmp");
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .map_err(|_| "The tool journal is busy or unwritable")?;
        struct Temporary(Option<std::path::PathBuf>);
        impl Drop for Temporary {
            fn drop(&mut self) {
                if let Some(path) = &self.0 {
                    let _ = std::fs::remove_file(path);
                }
            }
        }
        let mut temporary_guard = Temporary(Some(temporary.clone()));
        let mut state = Self::load(path)?;
        if !apply(&mut state)? {
            return Ok(false);
        }
        file.write_all(&state.bytes()?)
            .map_err(|_| "The tool journal could not be written")?;
        file.sync_all()
            .map_err(|_| "The tool journal could not be synchronized")?;
        drop(file);
        std::fs::rename(&temporary, path).map_err(|_| "The tool journal could not be committed")?;
        temporary_guard.0 = None;
        #[cfg(unix)]
        {
            let directory = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            std::fs::File::open(directory)
                .and_then(|directory| directory.sync_all())
                .map_err(|_| "The tool journal directory could not be synchronized")?;
        }
        Ok(true)
    }
    fn begin_resume(
        path: &Path,
        request: &str,
        content_hash: &str,
        source: Option<&str>,
    ) -> Result<bool, String> {
        if request.is_empty()
            || request.len() > 128
            || !Self::valid_hash(content_hash)
            || source.is_some_and(|source| source.is_empty() || source.len() > 128)
        {
            return Err("The continuation request is invalid".into());
        }
        Self::update(path, |state| {
            if let Some(budget) = state.continuations.get_mut(request) {
                if budget.content_hash != content_hash
                    || (source.is_some()
                        && budget
                            .source
                            .as_deref()
                            .is_some_and(|old| Some(old) != source))
                {
                    return Err("The continuation request hash changed".into());
                }
                if budget.attempts >= MAX_TRIES {
                    return Ok(false);
                }
                budget.attempts += 1;
                if budget.source.is_none() {
                    budget.source = source.map(str::to_string);
                }
            } else {
                state.continuations.insert(
                    request.into(),
                    ResumeAttempts {
                        content_hash: content_hash.into(),
                        attempts: 1,
                        source: source.map(str::to_string),
                        prepared: None,
                        progress: None,
                    },
                );
            }
            Ok(true)
        })
    }
}

/// The mini-app tools of one chat, for one turn: calls run as the agent serving `serving`.
struct SpaceTools<'a> {
    e: &'a Arc<Client>,
    journal: &'a Path,
    source: &'a str,
    space: String,
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
            space: space.into(),
            serving,
            names,
            deckcraft: cx.deckcraft.as_ref(),
            deckcraft_items,
        }
    }

    fn stage_approval(&self, ask: &ToolAsk) -> Result<client_core::AgentRequestDto, String> {
        let (_, item, tool) = self
            .names
            .iter()
            .find(|(definition, _, _)| definition.name == ask.call.name)
            .ok_or("The held tool is no longer offered")?;
        if !self.deckcraft_items.contains(item) || !crate::deckcraft::recognizes(tool) {
            return Err("This tool has no supported canonical approval shape".into());
        }
        let worker = self
            .deckcraft
            .ok_or("The presentation executor is unavailable")?;
        let (version, hash, _) =
            client_core::deckcraft_agent_tools::command(tool, &ask.call.arguments)?;
        let turn = self
            .e
            .agent_binary_turn(item, self.source, self.serving, version, hash.as_deref())
            .map_err(|e| e.to_string())?;
        self.e
            .stage_tool_approval(
                &turn,
                client_core::AgentToolApprovalInput {
                    canonical_tool: tool.clone(),
                    arguments: ask.call.arguments.clone(),
                    worker_binding: worker.binding_sha256().into(),
                    continuation_json: serde_json::to_string(ask)
                        .map_err(|_| "Invalid held call")?,
                    gate_rule: ask.rule.clone(),
                },
            )
            .map_err(|e| e.to_string())
    }

    fn load(&self) -> Vec<(String, Vec<ToolStep>)> {
        JournalState::load(self.journal)
            .map(|state| state.steps)
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

    fn begin_resume(&self, request: &str, content_hash: &str) -> Result<bool, String> {
        JournalState::begin_resume(self.journal, request, content_hash, Some(self.source))
    }

    fn canonical_continuation(&self) -> Result<Option<(String, String, ToolAsk)>, String> {
        let requests = self.e.tool_approval_requests(&self.space, self.source);
        let Some(request) = requests.last() else {
            return Ok(None);
        };
        let proposal = request
            .tool_proposal
            .as_ref()
            .ok_or("The canonical tool proposal is missing")?;
        let original: ToolAsk = serde_json::from_str(&proposal.continuation_json)
            .map_err(|_| "The canonical tool continuation is invalid")?;
        if proposal.source != self.source
            || original.envelope.space != self.space
            || original.envelope.from != self.serving
        {
            return Err("The canonical tool source changed".into());
        }
        Ok(Some((
            request.id.clone(),
            request.content_hash.clone(),
            original,
        )))
    }

    fn recovery_checkpoint(
        &self,
        request: &str,
        hash: &str,
    ) -> Result<Option<(ToolProgress, Option<ToolAsk>)>, String> {
        let state = JournalState::load(self.journal)?;
        let Some(budget) = state.continuations.get(request) else {
            return Ok(None);
        };
        if budget.content_hash != hash || budget.source.as_deref() != Some(self.source) {
            return Err("The continuation checkpoint binding changed".into());
        }
        match (budget.progress.clone(), budget.prepared.clone()) {
            (Some(progress), prepared) => Ok(Some((progress, prepared))),
            (None, None) => Ok(None),
            (None, Some(_)) => Err("The prepared call has no completed checkpoint".into()),
        }
    }

    fn same_calls(prefix: &[ToolStep], steps: &[ToolStep]) -> bool {
        prefix.len() <= steps.len()
            && prefix
                .iter()
                .zip(steps)
                .all(|(old, new)| old.call == new.call)
    }

    fn prepare_continuation(&self, prepared: &ToolAsk) -> Result<(), String> {
        if prepared.version != 1
            || prepared.round >= 4
            || prepared.steps.len() >= 16
            || prepared.remaining.len() > 3
            || prepared.steps.len() + 1 + prepared.remaining.len() > 16
            || serde_json::to_vec(prepared).map_or(true, |bytes| {
                bytes.len() > zoen_agentd::MAX_TOOL_CONTINUATION_BYTES
            })
        {
            return Err("The prepared continuation exceeds its capacity".into());
        }
        let Some((request, hash, original)) = self.canonical_continuation()? else {
            return Ok(());
        };
        if prepared.envelope != original.envelope
            || prepared.policy_sha256 != original.policy_sha256
            || prepared.round < original.round
            || prepared.steps.len() <= original.steps.len()
            || !Self::same_calls(&original.steps, &prepared.steps)
            || prepared.steps[original.steps.len()].call != original.call
            || self
                .names
                .iter()
                .find(|(definition, _, _)| definition.name == prepared.call.name)
                .map(|(definition, _, _)| definition)
                != Some(&prepared.definition)
        {
            return Err("The prepared continuation changed its canonical context".into());
        }
        JournalState::update(self.journal, |state| {
            let budget = state
                .continuations
                .get_mut(&request)
                .ok_or("The continuation attempt was not reserved")?;
            if budget.content_hash != hash
                || budget.source.as_deref() != Some(self.source)
                || budget
                    .prepared
                    .as_ref()
                    .is_some_and(|prior| prior != prepared)
                || budget.progress.as_ref().is_none_or(|prior| {
                    prior.round > prepared.round || !Self::same_calls(&prior.steps, &prepared.steps)
                })
            {
                return Err("The prepared continuation conflicts with retained progress".into());
            }
            budget.prepared = Some(prepared.clone());
            Ok(true)
        })
        .map(|_| ())
    }

    fn checkpoint_continuation(&self, progress: &ToolProgress) -> Result<(), String> {
        if serde_json::to_vec(progress).map_or(true, |bytes| {
            bytes.len() > zoen_agentd::MAX_TOOL_CONTINUATION_BYTES
        }) {
            return Err("The completed continuation exceeds its capacity".into());
        }
        let Some((request, hash, original)) = self.canonical_continuation()? else {
            return Ok(());
        };
        if progress.round < original.round
            || progress.round >= 4
            || progress.steps.len() <= original.steps.len()
            || progress.steps.len() > 16
            || progress.remaining.len() > 3
            || progress.steps.len() + progress.remaining.len() > 16
            || !Self::same_calls(&original.steps, &progress.steps)
            || progress.steps[original.steps.len()].call != original.call
        {
            return Err("The completed continuation changed its canonical context".into());
        }
        JournalState::update(self.journal, |state| {
            let budget = state
                .continuations
                .get_mut(&request)
                .ok_or("The continuation attempt was not reserved")?;
            if budget.content_hash != hash
                || budget.source.as_deref() != Some(self.source)
                || budget.progress.as_ref().is_some_and(|prior| {
                    prior.round > progress.round || !Self::same_calls(&prior.steps, &progress.steps)
                })
            {
                return Err("The completed continuation conflicts with retained progress".into());
            }
            if let Some(prepared) = &budget.prepared {
                let matches = prepared.round == progress.round
                    && prepared.remaining == progress.remaining
                    && progress.steps.len() == prepared.steps.len() + 1
                    && Self::same_calls(&prepared.steps, &progress.steps)
                    && progress
                        .steps
                        .last()
                        .is_some_and(|step| step.call == prepared.call);
                if !matches {
                    return Err("The completed call does not match the prepared effect".into());
                }
                budget.prepared = None;
            }
            budget.progress = Some(progress.clone());
            match state
                .steps
                .iter_mut()
                .find(|(source, _)| source == self.source)
            {
                Some((_, steps)) => *steps = progress.steps.clone(),
                None => state
                    .steps
                    .push((self.source.into(), progress.steps.clone())),
            }
            let drop = state.steps.len().saturating_sub(JOURNAL_KEEP);
            state.steps.drain(..drop);
            Ok(true)
        })
        .map(|_| ())
    }

    fn record(&self, step: ToolStep) {
        let result = JournalState::update(self.journal, |state| {
            match state
                .steps
                .iter_mut()
                .find(|(source, _)| source == self.source)
            {
                Some((_, steps)) => steps.push(step),
                None => state.steps.push((self.source.to_string(), vec![step])),
            }
            let drop = state.steps.len().saturating_sub(JOURNAL_KEEP);
            state.steps.drain(..drop);
            Ok(true)
        });
        if result.is_err() {
            say("tool journal write failed");
        }
    }
}

struct CanonicalToolAdmission<'a, 'b> {
    tools: &'a SpaceTools<'b>,
    permit: client_core::ApprovedToolTurn,
}
impl ApprovedToolAdmission for CanonicalToolAdmission<'_, '_> {
    fn execute(&self, ask: &ToolAsk) -> Result<ToolResult, String> {
        let p = self.permit.proposal();
        if serde_json::to_string(ask).map_err(|_| "Invalid held call")? != p.continuation_json {
            return Err("The approved intent changed".into());
        }
        let worker = self
            .tools
            .deckcraft
            .ok_or("The presentation executor is unavailable")?;
        // Native authorization is rechecked even for a cached read result. Writes
        // always recover through their canonical binary receipt instead.
        self.tools
            .e
            .agent_binary_finish_read(self.permit.turn())
            .map_err(|e| e.to_string())?;
        let result = if p.canonical_tool == "read_deckcraft" {
            self.tools
                .done()
                .into_iter()
                .find(|step| step.call == ask.call)
                .map(|step| step.result)
        } else {
            None
        };
        let result = match result {
            Some(result) => result,
            None => tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(crate::deckcraft::call_approved(
                    self.tools.e,
                    worker,
                    &self.permit,
                ))
            })?,
        };
        if !self.tools.done().iter().any(|step| step.call == ask.call) {
            fault("before-journal");
            self.tools.record(ToolStep {
                call: ask.call.clone(),
                result: result.clone(),
                is_error: false,
            });
            fault("after-tool");
        }
        Ok(ToolResult {
            result,
            is_error: false,
        })
    }
}

impl ToolHost for SpaceTools<'_> {
    fn tools(&self) -> Vec<ToolDef> {
        self.names.iter().map(|(d, _, _)| d.clone()).collect()
    }

    fn prepare(&self, prepared: &ToolAsk) -> Result<(), String> {
        self.prepare_continuation(prepared)
    }
    fn checkpoint(&self, progress: &ToolProgress) -> Result<(), String> {
        self.checkpoint_continuation(progress)
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
pub(crate) async fn exercise_deckcraft_operation(
    e: &Arc<Client>,
    worker: Arc<zoen_agentd::deckcraft::DeckcraftWorker>,
    journal: &Path,
    binding: (&str, &str, &str),
    arguments: serde_json::Value,
) -> Vec<ToolStep> {
    use zoen_agentd::citizen::testing::scripted::ScriptedBrain;
    let (item, source, serving) = binding;
    let space = e.item(item.into()).unwrap().space_id;
    let account = e.account().unwrap();
    let call_name = crate::deckcraft::model_name("deckcraft_slide_notes", item).unwrap();
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
                replies: [("reviewed edits".into(), response)].into(),
            }),
        ),
        deckcraft: Some(worker),
        journal: journal.into(),
    };
    let host = SpaceTools::new(e, &cx, &space, serving, source);
    let definition: Definition = serde_json::from_value(json!({"version":1,"kind":"user","persona":"Apply the reviewed presentation edit.","rules":[],"state":{}})).unwrap();
    let context = TurnContext {
        agent: &cx.me,
        device: &cx.device,
        owner: &cx.owner,
        definition: &definition,
    };
    let entry = e
        .timeline(space.clone())
        .unwrap()
        .into_iter()
        .find(|entry| entry.id == source)
        .unwrap();
    let EntryKind::Message { text, .. } = entry.kind else {
        panic!("signed message source")
    };
    let outcome = cx
        .runtime
        .turn_with_tools(
            context,
            &[],
            &Turn {
                space,
                from: entry.author.id,
                text,
            },
            &host,
            vec![],
        )
        .await
        .unwrap();
    assert!(
        matches!(outcome, TurnOutcome::Say { ref text } if text == "Applied the reviewed notes."),
        "actual Runtime outcome: {outcome:?}; durable steps: {:?}",
        host.done()
    );
    let steps = host.done();
    assert_eq!(steps.len(), 1);
    assert!(!steps[0].is_error);
    assert_eq!(steps[0].call.name, call_name);
    steps
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
            space: "synthetic-space".into(),
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

#[cfg(test)]
#[path = "agent/codex_brain_tests.rs"]
mod codex_brain_tests;
#[cfg(test)]
pub(crate) use codex_brain_tests::exercise_codex_deckcraft;

#[cfg(test)]
#[path = "agent/canonical_tool_approval_tests.rs"]
mod canonical_tool_approval_tests;

#[cfg(test)]
#[path = "agent/continuation_journal_tests.rs"]
mod continuation_journal_tests;

#[cfg(test)]
#[path = "agent/canonical_read_recovery_tests.rs"]
mod canonical_read_recovery_tests;

#[cfg(test)]
#[path = "agent/canonical_provider_acceptance_tests.rs"]
mod canonical_provider_acceptance_tests;
