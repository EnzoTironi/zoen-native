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

mod providers;
use providers::{brain, gate};
mod turn;
use turn::turn;
mod messages;
use messages::{
    owner_note, selected_owner_note, selected_proposal, selected_reply, tell_owner, text_of,
    turn_note,
};
mod journal;
#[cfg(test)]
mod page_tool_tests;
mod page_tools;
mod space_tools;
#[cfg(test)]
mod test_support;
mod tool_execution;
#[cfg(test)]
pub(crate) use test_support::{exercise_deckcraft_dispatch, exercise_deckcraft_operation};

mod source_mode;
mod startup;
pub(crate) use source_mode::Source;
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

#[cfg(test)]
#[path = "agent/gate_transport_tests.rs"]
mod gate_transport_tests;

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
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
    source: Option<Source>,
) -> Result<(), String> {
    // Normal mode retains provider construction before startup. Source mode must
    // validate the selected signed message before constructing any provider.
    let scoped = source.is_some();
    if scoped {
        Source::preflight(e)?;
    }
    let runtime = if scoped {
        None
    } else {
        Some(Runtime::new(gate()?, brain()?))
    };
    let acct = e.account().ok_or("no account")?;
    let owner = e
        .agent_owner()
        .ok_or("this account is not an agent with an owner")?;
    let deckcraft = if scoped {
        None
    } else {
        Some(crate::deckcraft::configured()?)
    };
    let waker = Arc::new(Waker::default());
    e.stop_sync();
    e.start_sync(Some(waker.clone() as Arc<dyn CoreListener>))
        .map_err(|error| {
            if scoped {
                "agent_host_source_sync_unavailable".into()
            } else {
                error.to_string()
            }
        })?;
    let (mut cursors, mut selected) = if let Some(source) = source {
        let cursors = source_mode::admit(e, 15000).await?;
        (
            cursors.into_iter().collect(),
            Some(source_mode::Run::prepare(e, source)?),
        )
    } else {
        (startup::prepare(e, readiness, 15000).await?, None)
    };
    if let Some(selected) = &selected {
        selected.actor(&acct.identity_id, &acct.device_id, &owner)?;
        selected.check(e)?;
    }
    let cx = Ctx {
        me: acct.identity_id.clone(),
        device: acct.device_id.clone(),
        owner,
        runtime: match runtime {
            Some(runtime) => runtime,
            None => Runtime::new(
                gate().map_err(|_| "agent_host_source_gate_unavailable")?,
                brain().map_err(|_| "agent_host_source_brain_unavailable")?,
            ),
        },
        deckcraft: match deckcraft {
            Some(worker) => worker,
            None => crate::deckcraft::configured()
                .map_err(|_| "agent_host_source_worker_unavailable")?,
        },
        journal: home.join("agent-tool-steps.json"),
    };
    if let Some(selected) = &selected {
        selected.check(e)?;
        readiness
            .publish()
            .map_err(|_| "agent_host_source_readiness_unavailable")?;
    }
    let mut known: BTreeSet<String> = cursors.keys().cloned().collect();
    say(format_args!("agent @{} hosting", acct.handle));

    let deadline = for_secs.map(|s| Instant::now() + Duration::from_secs(s));
    let mut backoff = Duration::from_secs(1);
    let mut offline_since: Option<Instant> = None;
    let mut last_activity = Instant::now();
    let mut failures = 0u32;
    loop {
        if deadline.is_some_and(|d| Instant::now() >= d) {
            if let Some(selected) = &selected {
                selected.check(e)?;
                say("agent_host_source_wait_elapsed");
            }
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

        if let Some(selected) = &selected {
            selected.check(e)?;
        }
        let state = if scoped {
            tokio::time::timeout(
                deadline
                    .expect("bounded source mode")
                    .saturating_duration_since(Instant::now()),
                e.agent_state(),
            )
            .await
            .map_err(|_| "agent_host_source_state_unavailable")?
        } else {
            e.agent_state().await
        };
        let status = match state {
            Ok(s) => {
                failures = 0;
                s
            }
            Err(err) => {
                if scoped {
                    return Err("agent_host_source_state_unavailable".into());
                }
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
        if let Some(selected) = &selected {
            selected.check(e)?;
        }
        let selected_definition = if scoped {
            Some(source_mode::definition(
                &status.status,
                &status.kind,
                status.definition_version,
                &status.definition,
            )?)
        } else {
            None
        };
        if status.status == "revoked" {
            say("revoked by the owner");
            break;
        }
        let def = match selected_definition
            .map(Ok)
            .unwrap_or_else(|| Definition::decode(&status.definition))
        {
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

        // This branch precedes BOTH continuation discovery and the normal backlog
        // loop. Source mode never seeds/advances a cursor or emits a read watermark.
        if let Some(selected) = &mut selected {
            if selected
                .tick(e, &cx, &def, deadline.expect("bounded source mode"))
                .await?
            {
                e.wait_until_settled(8000)
                    .await
                    .map_err(|_| "agent_host_source_reply_unsettled")?;
                selected.check(e)?;
                say("agent_host_source_reply_completed");
                return Ok(());
            }
            wait(&waker, deadline).await;
            continue;
        }

        'spaces: for s in e.spaces() {
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
                    if turn(e, &cx, &def, &s.id, &tl[..index], &tl[index]).await
                        == turn::Outcome::Deferred
                    {
                        continue 'spaces;
                    }
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
                if handled == turn::Outcome::Deferred {
                    caught_up = false;
                    break; // no attempt, advance or read watermark before admission
                }
                if handled == turn::Outcome::Retry {
                    caught_up = false;
                    // Reservation and exhaustion are durable inside turn(). A
                    // corrupt/busy journal cannot become a skipped source merely
                    // because this in-memory wake loop has run three times.
                    break;
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
    if let Some(selected) = &selected {
        selected.check(e)?;
    }
    Ok(())
}

async fn wait(w: &Waker, deadline: Option<Instant>) {
    let mut d = Duration::from_secs(5);
    if let Some(end) = deadline {
        d = d.min(end.saturating_duration_since(Instant::now()));
    }
    let _ = tokio::time::timeout(d, w.notify.notified()).await;
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
    /// A held call was durably registered before it could enter its executor.
    registered: bool,
    /// Persisted before executor IO; absence of a result never clears this fence.
    outcome_unknown: bool,
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
    selection: Option<&'a source_mode::Bound>,
    page_items: Vec<String>,
}

struct CanonicalToolAdmission<'a, 'b> {
    tools: &'a SpaceTools<'b>,
    permit: page_tools::CanonicalPermit,
}

#[cfg(test)]
mod deckcraft_binding_tests;

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
