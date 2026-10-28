//! Read-only operational inventory of the authenticated Agent's current projection.
//! This is an observation, never task admission, a readiness marker or a clean-queue claim.
use client_core::{
    AccountDto, AgentRequestDto, Client, EntryKind, RequestStatus, SpaceKindDto, TimelineEntry,
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

mod failure;

type Result<T> = std::result::Result<T, &'static str>;

pub(crate) fn validate_args(args: &[String]) -> Result<()> {
    // Inspect the original argv: the legacy global parser consumes invalid --for
    // and --timeout values too, which must not silently become inspection defaults.
    let mut command = false;
    let mut flags = BTreeSet::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "agent-inspect" if !command => command = true,
            "--home" | "--timeout" if flags.insert(arg) => {
                let value = args.next().ok_or("agent_inspect_invalid_arguments")?;
                if value.is_empty() || value.starts_with("--") {
                    return Err("agent_inspect_invalid_arguments");
                }
                if arg == "--timeout" && value.parse::<u64>().ok().is_none_or(|ms| ms == 0) {
                    return Err("agent_inspect_positive_timeout_required");
                }
            }
            _ => return Err("agent_inspect_invalid_arguments"),
        }
    }
    if !command {
        return Err("agent_inspect_invalid_arguments");
    }
    Ok(())
}

pub(crate) fn existing_home(home: &std::path::Path) -> Result<()> {
    if !home.is_dir() || !home.join("zoen.sqlite").is_file() {
        return Err("agent_inspect_existing_home_required");
    }
    Ok(())
}

pub(crate) fn open_existing(home: &std::path::Path) -> Result<std::sync::Arc<Client>> {
    existing_home(home)?;
    let database = home.join("zoen.sqlite");
    Client::open(
        database.to_string_lossy().into(),
        std::env::var("ZOEN_LOCALE").unwrap_or_else(|_| "en-US".into()),
    )
    .map_err(|_| "agent_inspect_open_failed")
}

fn identifier(value: &str) -> Result<&str> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err("agent_inspect_invalid_identifier");
    }
    Ok(value)
}

#[derive(PartialEq, Eq)]
struct Scope {
    agent: String,
    device: String,
    owner: String,
    // Bind the configured relay across awaits without exporting a possibly private URL.
    relay: String,
}
fn scope(client: &Client) -> Result<(Scope, AccountDto)> {
    let account = client.account().ok_or("agent_inspect_missing_account")?;
    if !account.unlocked || account.setup_pending {
        return Err("agent_inspect_locked_or_pending_account");
    }
    let owner = client.agent_owner().ok_or("agent_inspect_not_agent")?;
    for id in [&account.identity_id, &account.device_id, &owner] {
        identifier(id)?;
    }
    Ok((
        Scope {
            agent: account.identity_id.clone(),
            device: account.device_id.clone(),
            owner,
            relay: account.relay_url.clone(),
        },
        account,
    ))
}

pub(crate) fn preflight(client: &Client) -> Result<()> {
    let (_, account) = scope(client)?;
    if !account.registered {
        return Err("agent_inspect_existing_registered_agent_required");
    }
    Ok(())
}

fn request_status(status: RequestStatus) -> &'static str {
    match status {
        RequestStatus::Cancelled => "cancelled",
        RequestStatus::Pending => "pending",
        RequestStatus::Approved => "approved",
        RequestStatus::Denied => "denied",
        RequestStatus::Stale => "stale",
    }
}

fn replied_sources<'a>(timeline: &'a [TimelineEntry], agent: &str) -> BTreeSet<&'a str> {
    // Match the normal host's existing source-reply deduplication exactly.
    timeline
        .iter()
        .filter(|entry| entry.author.id == agent)
        .filter_map(|entry| entry.reply_to.as_ref().map(|reply| reply.id.as_str()))
        .collect()
}

fn entry(entry: &TimelineEntry, replied: &BTreeSet<&str>) -> Result<Value> {
    let (kind, artifact, version, request) = match &entry.kind {
        EntryKind::Message {
            card: Some(card), ..
        } => (
            "message",
            Some(identifier(&card.item_id)?),
            Some(card.version),
            None,
        ),
        EntryKind::Message { card: None, .. } => ("message", None, None, None),
        EntryKind::ItemEdited {
            item_id, version, ..
        } => (
            "item_edited",
            Some(identifier(item_id)?),
            Some(*version),
            None,
        ),
        EntryKind::Request { request_id, .. } => {
            ("request", None, None, Some(identifier(request_id)?))
        }
        EntryKind::System { .. } => ("system", None, None, None),
        EntryKind::Background { .. } => ("background", None, None, None),
    };
    Ok(json!({
        "id": identifier(&entry.id)?, "seq": entry.seq, "kind": kind,
        "authorId": identifier(&entry.author.id)?, "artifactId": artifact,
        "version": version, "requestId": request,
        "replyExists": replied.contains(entry.id.as_str()),
    }))
}

fn request(request: &AgentRequestDto, sources: &BTreeMap<String, String>) -> Result<Value> {
    let (artifact, version, kind) = match &request.tool_approval {
        Some(tool) => (
            Some(tool.artifact_id.as_str()),
            Some(tool.version),
            "canonical_tool",
        ),
        None => (request.item_id.as_deref(), None, "legacy"),
    };
    if let Some(id) = artifact {
        identifier(id)?;
    }
    if let Some(source) = sources.get(&request.id) {
        identifier(source)?;
    }
    Ok(json!({
        "id": identifier(&request.id)?, "spaceId": identifier(&request.space_id)?,
        "kind": kind, "status": request_status(request.status),
        "artifactId": artifact, "version": version, "sourceId": sources.get(&request.id),
    }))
}

fn request_rows(
    values: Vec<AgentRequestDto>,
    agent: &str,
    sources: &BTreeMap<String, String>,
) -> Result<(Vec<Value>, usize)> {
    let mut unknown = 0;
    let mut requests = Vec::new();
    for value in values {
        if value.agent.id != agent {
            continue;
        }
        if !sources.contains_key(&value.id) {
            unknown += 1;
        }
        requests.push(request(&value, sources)?);
    }
    Ok((requests, unknown))
}

/// Sync must already be running. Main stops it on success and failure; no providers
/// or host startup helper are called. Receiving sync can persist the usual local cache.
pub(crate) async fn inspect(client: &Client, timeout_ms: u64) -> Result<String> {
    if timeout_ms == 0 {
        return Err("agent_inspect_positive_timeout_required");
    }
    preflight(client)?;
    let (bound, _) = scope(client)?;
    let started = tokio::time::Instant::now();
    let deadline = started
        .checked_add(std::time::Duration::from_millis(timeout_ms))
        .ok_or("agent_inspect_timeout_out_of_range")?;
    client
        .wait_until_settled(timeout_ms)
        .await
        .map_err(|_| "agent_inspect_sync_unsettled")?;
    let before_state = started.elapsed();
    let state_started = tokio::time::Instant::now();
    let state = tokio::time::timeout_at(deadline, client.agent_state())
        .await
        .map_err(|_| {
            failure::trace(
                None,
                timeout_ms,
                started.elapsed(),
                before_state,
                state_started.elapsed(),
            );
            "agent_inspect_state_unavailable"
        })?
        .map_err(|error| {
            failure::trace(
                Some(&error),
                timeout_ms,
                started.elapsed(),
                before_state,
                state_started.elapsed(),
            );
            "agent_inspect_state_unavailable"
        })?;
    if !matches!(state.status.as_str(), "active" | "paused" | "revoked")
        || !matches!(state.kind.as_str(), "User" | "Creator")
    {
        return Err("agent_inspect_invalid_state");
    }
    let (current, account) = scope(client)?;
    if bound != current || !account.registered {
        return Err("agent_inspect_scope_changed_or_unregistered");
    }
    let mut cursors = BTreeMap::new();
    for (space, seq) in state.cursors {
        identifier(&space)?;
        if cursors.insert(space, seq).is_some() {
            return Err("agent_inspect_duplicate_cursor");
        }
    }
    let mut sources = BTreeMap::new();
    let mut spaces = Vec::new();
    for space in client.spaces() {
        identifier(&space.id)?;
        let timeline = client
            .timeline(space.id.clone())
            .map_err(|_| "agent_inspect_timeline_unavailable")?;
        let approved = client.approved_tool_sources(&space.id);
        let replied = replied_sources(&timeline, &bound.agent);
        let mut approved_rows = Vec::new();
        let mut discover: BTreeSet<_> = timeline.iter().map(|entry| entry.id.clone()).collect();
        for source in approved {
            identifier(&source)?;
            discover.insert(source.clone());
            approved_rows.push(json!({
                "sourceId": source,
                "replyExists": replied.contains(source.as_str()),
                "sourceInCurrentTimeline": timeline.iter().any(|entry| entry.id == source),
            }));
        }
        for source in discover {
            for request in client.tool_approval_requests(&space.id, &source) {
                identifier(&request.id)?;
                identifier(&source)?;
                sources.insert(request.id, source.clone());
            }
        }
        let entries = timeline
            .iter()
            .map(|value| entry(value, &replied))
            .collect::<Result<Vec<_>>>()?;
        let kind = match space.kind {
            SpaceKindDto::Direct => "direct",
            SpaceKindDto::Group => "group",
            SpaceKindDto::Community => "community",
            SpaceKindDto::Saved => "saved",
        };
        spaces.push(json!({
            "id": space.id, "kind": kind,
            "cursor": cursors.get(&space.id),
            "head": timeline.iter().map(|entry| entry.seq).max(),
            "entries": entries, "approvedSources": approved_rows,
        }));
    }
    let (requests, unknown) = request_rows(client.requests(), &bound.agent, &sources)?;
    let (after, after_account) = scope(client)?;
    if bound != after || !after_account.registered {
        return Err("agent_inspect_scope_changed_or_unregistered");
    }
    if tokio::time::Instant::now() >= deadline {
        return Err("agent_inspect_deadline_elapsed");
    }
    serde_json::to_string(&json!({
        "schema": "zoen.agent.inspect.v1",
        "agentId": bound.agent, "ownerId": bound.owner, "deviceId": bound.device,
        "status": state.status, "kind": state.kind, "definitionVersion": state.definition_version,
        "cursors": cursors, "spaces": spaces, "requests": requests,
        "coverage": {
            "scope": "current_local_projection", "atomicSnapshot": false,
            "olderHistoryFetched": false, "wholeRelayHistory": false,
            "requestsWithUnknownSource": unknown,
        },
    }))
    .map_err(|_| "agent_inspect_encoding_failed")
}

#[cfg(test)]
mod tests;
