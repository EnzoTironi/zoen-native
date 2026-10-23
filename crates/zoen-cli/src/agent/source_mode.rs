//! An explicit source selector, never a grant or a substitute for canonical recovery.
use super::*;
use client_core::{Delivery, RequestStatus};
use sha2::{Digest, Sha256};

pub(super) type Head = Option<(String, RequestStatus, Option<String>)>;
pub(super) struct Attempt {
    pub handled: bool,
    /// Canonical head actually read inside turn, not a snapshot taken after its await.
    pub observed: Option<Head>,
}

pub(super) fn definition(
    status: &str,
    kind: &str,
    version: u32,
    bytes: &[u8],
) -> Result<Definition, String> {
    if !matches!(status, "active" | "paused" | "revoked") || !matches!(kind, "User" | "Creator") {
        return Err("agent_host_source_invalid_state".into());
    }
    if status == "revoked" {
        return Err("agent_host_source_revoked".into());
    }
    let definition =
        Definition::decode(bytes).map_err(|_| "agent_host_source_definition_unavailable")?;
    if version == 0 || definition.version != version || definition.kind.relay_kind() != kind {
        return Err("agent_host_source_definition_mismatch".into());
    }
    Ok(definition)
}

pub(super) async fn admit(e: &Client, timeout_ms: u64) -> Result<Vec<(String, u64)>, String> {
    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
    startup::settle(e, timeout_ms)
        .await
        .map_err(|_| "agent_host_source_startup_unavailable")?;
    let state = tokio::time::timeout_at(deadline, e.agent_state())
        .await
        .map_err(|_| "agent_host_source_state_unavailable")?
        .map_err(|_| "agent_host_source_state_unavailable")?;
    definition(
        &state.status,
        &state.kind,
        state.definition_version,
        &state.definition,
    )?;
    let mut spaces = BTreeSet::new();
    if state.cursors.iter().any(|(space, _)| !spaces.insert(space)) {
        return Err("agent_host_source_invalid_state".into());
    }
    Ok(state.cursors)
}

pub(crate) struct Source {
    id: String,
}
impl Source {
    pub(crate) fn preflight(e: &Client) -> Result<(), String> {
        Scope::current(e).map(|_| ())
    }
    /// Parse original argv before the legacy global parser can consume bad values.
    /// No selector preserves the normal host's existing argument semantics.
    pub(crate) fn parse(args: &[String]) -> Result<Option<Self>, String> {
        if !args.iter().any(|arg| arg.starts_with("--source")) {
            return Ok(None);
        }
        let mut command = false;
        let mut flags = BTreeSet::new();
        let mut source = None;
        let mut duration = None;
        let mut args = args.iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "agent-host" if !command => command = true,
                "--home" | "--source" | "--for" | "--timeout" if flags.insert(arg) => {
                    let value = args.next().ok_or("agent_host_source_invalid_arguments")?;
                    if value.is_empty() || value.starts_with("--") {
                        return Err("agent_host_source_invalid_arguments".into());
                    }
                    match arg.as_str() {
                        "--source" => {
                            if value.len() > 128
                                || !value
                                    .bytes()
                                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                            {
                                return Err("agent_host_source_invalid_identifier".into());
                            }
                            source = Some(value.clone());
                        }
                        "--for" | "--timeout" => {
                            let number = value
                                .parse::<u64>()
                                .ok()
                                .filter(|number| *number > 0)
                                .ok_or("agent_host_source_positive_duration_required")?;
                            if arg == "--for" {
                                duration = Instant::now().checked_add(Duration::from_secs(number));
                            }
                        }
                        _ => {}
                    }
                }
                _ => return Err("agent_host_source_invalid_arguments".into()),
            }
        }
        if !command || duration.is_none() {
            return Err("agent_host_source_positive_duration_required".into());
        }
        Ok(source.map(|id| Self { id }))
    }
}

#[derive(PartialEq, Eq)]
struct Scope {
    agent: String,
    device: String,
    owner: String,
    relay: String,
}
impl Scope {
    fn current(e: &Client) -> Result<Self, String> {
        let account = e.account().ok_or("agent_host_source_account_unavailable")?;
        if !account.registered || !account.unlocked || account.setup_pending {
            return Err("agent_host_source_account_unavailable".into());
        }
        Ok(Self {
            agent: account.identity_id,
            device: account.device_id,
            owner: e.agent_owner().ok_or("agent_host_source_not_agent")?,
            relay: account.relay_url,
        })
    }
}

pub(super) struct Bound {
    scope: Scope,
    space: String,
    id: String,
    author: String,
    text_sha256: Vec<u8>,
}
impl Bound {
    pub(super) fn dispatch(
        &self,
        e: &Client,
        cx: &Ctx,
        space: &str,
        entry: &TimelineEntry,
    ) -> Result<(), String> {
        self.check(e)?;
        if space != self.space
            || entry.id != self.id
            || entry.author.id != self.author
            || cx.me != self.scope.agent
            || cx.device != self.scope.device
            || cx.owner != self.scope.owner
            || Sha256::digest(Self::text(entry)?.as_bytes()).to_vec() != self.text_sha256
        {
            return Err("agent_host_source_changed".into());
        }
        Ok(())
    }
    fn text(entry: &TimelineEntry) -> Result<&str, String> {
        match &entry.kind {
            EntryKind::Message { text, card: None }
                if !entry.deleted && !text.trim().is_empty() =>
            {
                Ok(text)
            }
            _ => Err("agent_host_source_not_text_message".into()),
        }
    }
    fn membership(&self, e: &Client) -> Result<(), String> {
        let space = e
            .space(self.space.clone())
            .map_err(|_| "agent_host_source_space_unavailable")?;
        if self.author == self.scope.agent
            || ![&self.author, &self.scope.agent]
                .iter()
                .all(|id| space.members.iter().any(|member| &member.id == *id))
        {
            return Err("agent_host_source_member_unavailable".into());
        }
        Ok(())
    }
    fn locate(e: &Client, source: Source) -> Result<Self, String> {
        let scope = Scope::current(e)?;
        let mut found = None;
        for space in e.spaces() {
            let timeline = e
                .timeline(space.id.clone())
                .map_err(|_| "agent_host_source_timeline_unavailable")?;
            for entry in timeline.iter().filter(|entry| entry.id == source.id) {
                if found.is_some() {
                    return Err("agent_host_source_ambiguous".into());
                }
                found = Some(Self {
                    scope: Scope::current(e)?,
                    space: space.id.clone(),
                    id: entry.id.clone(),
                    author: entry.author.id.clone(),
                    text_sha256: Sha256::digest(Self::text(entry)?.as_bytes()).to_vec(),
                });
            }
        }
        let bound = found.ok_or("agent_host_source_unavailable_in_projection")?;
        if scope != bound.scope {
            return Err("agent_host_source_scope_changed".into());
        }
        bound.check(e)?;
        Ok(bound)
    }
    pub(super) fn check(&self, e: &Client) -> Result<(), String> {
        if Scope::current(e)? != self.scope {
            return Err("agent_host_source_scope_changed".into());
        }
        self.membership(e)?;
        let timeline = e
            .timeline(self.space.clone())
            .map_err(|_| "agent_host_source_timeline_unavailable")?;
        let mut entries = timeline.iter().filter(|entry| entry.id == self.id);
        let entry = entries
            .next()
            .ok_or("agent_host_source_unavailable_in_projection")?;
        if entries.next().is_some()
            || entry.author.id != self.author
            || Sha256::digest(Self::text(entry)?.as_bytes()).to_vec() != self.text_sha256
        {
            return Err("agent_host_source_changed".into());
        }
        if Scope::current(e)? != self.scope {
            return Err("agent_host_source_scope_changed".into());
        }
        Ok(())
    }
}

/// Only an in-memory scheduling observation. The canonical Request/journal/receipt
/// remains the existing recovery authority. No cursor or completion record is written.
pub(super) struct Run {
    bound: Bound,
    observed: Option<Head>,
}
impl Run {
    pub(super) fn prepare(e: &Client, source: Source) -> Result<Self, String> {
        Ok(Self {
            bound: Bound::locate(e, source)?,
            observed: None,
        })
    }
    pub(super) fn check(&self, e: &Client) -> Result<(), String> {
        self.bound.check(e)
    }
    pub(super) fn actor(&self, agent: &str, device: &str, owner: &str) -> Result<(), String> {
        if self.bound.scope.agent != agent
            || self.bound.scope.device != device
            || self.bound.scope.owner != owner
        {
            return Err("agent_host_source_scope_changed".into());
        }
        Ok(())
    }

    fn head(&self, e: &Client) -> Result<Head, String> {
        self.check(e)?;
        let requests = e.tool_approval_requests(&self.bound.space, &self.bound.id);
        let Some(request) = requests.last() else {
            return Ok(None);
        };
        let dto = e
            .requests()
            .into_iter()
            .find(|dto| dto.id == request.id)
            .ok_or("agent_host_source_request_unavailable")?;
        self.check(e)?;
        let intent = e.tool_review_pending_intent(&dto.id);
        Ok(Some((dto.id, dto.status, intent)))
    }
    pub(super) async fn tick(
        &mut self,
        e: &Arc<Client>,
        cx: &Ctx,
        definition: &Definition,
        deadline: Instant,
    ) -> Result<bool, String> {
        self.check(e)?;
        let timeline = e
            .timeline(self.bound.space.clone())
            .map_err(|_| "agent_host_source_timeline_unavailable")?;
        let index = timeline
            .iter()
            .position(|entry| entry.id == self.bound.id)
            .ok_or("agent_host_source_unavailable_in_projection")?;
        // A queued own reply must settle; it is never a reason to run the model again.
        if let Some(reply) = timeline.iter().find(|entry| {
            entry.author.id == self.bound.scope.agent
                && entry
                    .reply_to
                    .as_ref()
                    .is_some_and(|reply| reply.id == self.bound.id)
        }) {
            self.check(e)?;
            return Ok(matches!(
                reply.delivery,
                Delivery::Sent | Delivery::Delivered | Delivery::Read
            ));
        }
        let head = self.head(e)?;
        if self.observed.as_ref() == Some(&head) {
            return Ok(false);
        }
        // Remember BEFORE awaiting: do not swallow an approval arriving during the turn.
        // An unknown result terminates this run, never implicit provider replay.
        self.observed = Some(head);
        self.check(e)?;
        let attempt = tokio::time::timeout(
            deadline.saturating_duration_since(Instant::now()),
            super::turn::selected(
                e,
                cx,
                definition,
                &self.bound.space,
                &timeline[..index],
                &timeline[index],
                &self.bound,
            ),
        )
        .await
        .map_err(|_| "agent_host_source_outcome_unknown")?;
        self.check(e)?;
        if let Some(observed) = attempt.observed {
            self.observed = Some(observed);
        }
        if !attempt.handled {
            return Err("agent_host_source_outcome_unknown".into());
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests;
