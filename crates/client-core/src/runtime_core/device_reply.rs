//! Device reply operations on the original DeviceCore state.

use super::*;

impl DeviceCore {
    pub fn reply(&self, space: &str, trigger: &str, at_ms: i64) -> Result<ReplyFacts> {
        self.extract(space, trigger, at_ms)
    }

    /// Applied native position only. This is not a current relay head or grant.
    pub fn retained_head(&self, space: &str) -> Result<Option<Seen>> {
        if !self.usable {
            return Err(NativeError::History);
        }
        Ok(self.engine.logs.get(space).and_then(SpaceLog::head))
    }

    pub fn discover_replies(&mut self, space: &str, at_ms: i64) -> Result<ReplyDiscovery> {
        if !self.usable || space.is_empty() || space.len() > 256 || at_ms < 0 {
            return Err(NativeError::History);
        }
        let key = format!("runtime.reply-scan/1/{space}");
        let cursor = self
            .engine
            .store
            .meta(&key)
            .map_err(|_| NativeError::Storage)?;
        let cursor: Option<Seen> = cursor
            .map(|value| {
                if value.len() > 256 {
                    return Err(NativeError::History);
                }
                serde_json::from_str(&value).map_err(|_| NativeError::History)
            })
            .transpose()?;
        let log = self.engine.logs.get(space).ok_or(NativeError::History)?;
        let from = match &cursor {
            Some(cursor) => {
                let index = usize::try_from(cursor.seq).map_err(|_| NativeError::History)?;
                if log
                    .events()
                    .get(index)
                    .is_none_or(|event| event.seq != cursor.seq || event.hash != cursor.hash)
                {
                    return Err(NativeError::History);
                }
                index.checked_add(1).ok_or(NativeError::History)?
            }
            None => 0,
        };
        let mut replies = Vec::new();
        let mut examined = 0u16;
        let mut through = cursor;
        for event in log.events().iter().skip(from).take(MAX_BATCH) {
            if event.author == self.owner
                && matches!(
                    &event.body,
                    EventBody::MessagePosted {
                        attaches: None,
                        reply: None,
                        ..
                    }
                )
            {
                match self.extract(space, &event.hash, at_ms) {
                    Ok(facts) => replies.push(facts),
                    Err(NativeError::Permission) => {}
                    Err(error) => return Err(error),
                }
            }
            examined += 1;
            through = Some(Seen {
                seq: event.seq,
                hash: event.hash.clone(),
            });
            if replies.len() == 16 {
                break;
            }
        }
        let complete = through.as_ref() == log.head().as_ref();
        if examined > 0 {
            let value = serde_json::to_string(&through.ok_or(NativeError::History)?)
                .map_err(|_| NativeError::Storage)?;
            if self.engine.store.set_meta(&key, &value).is_err() {
                self.usable = false;
                return Err(NativeError::Storage);
            }
        }
        Ok(ReplyDiscovery {
            replies,
            examined,
            complete,
            changed: examined > 0,
        })
    }

    pub fn refresh(&self, original: &ReplyIntent, at_ms: i64) -> Result<ReplyFacts> {
        let current = self.extract(&original.space, &original.trigger, at_ms)?;
        if &current.intent != original {
            return Err(NativeError::Permission);
        }
        Ok(current)
    }

    pub(super) fn extract(&self, space: &str, trigger: &str, at_ms: i64) -> Result<ReplyFacts> {
        if !self.usable {
            return Err(NativeError::History);
        }
        if at_ms < 0 {
            return Err(NativeError::Permission);
        }
        let log = self.engine.logs.get(space).ok_or(NativeError::History)?;
        let mut ordered = State::default();
        let mut selected: Option<(GrantState, &Event)> = None;
        let mut grant_events = std::collections::HashMap::new();
        let mut original = None;
        let mut readable = false;
        let mut text = Zeroizing::new(String::new());
        let mut plans = crate::pages::PlanReplay::new(&self.engine.store, &self.engine.logs);
        for (end, event) in log.events().iter().enumerate() {
            let opened = crate::mls_opening::opened_leaf(&self.engine.store, event);
            let own_echo = self.engine.net.account.as_ref().is_some_and(|account| {
                crate::mls_opening::own_echo(&self.engine.store, event, account)
            });
            let historical_writers = direct_writers(&ordered, space, &self.owner, &self.agent);
            let context = plans
                .before(&ordered, &self.engine.identities, event, end)
                .map_err(|_| NativeError::History)?;
            let accepted = match ordered.apply_checked(event, &self.engine.identities, &context) {
                crate::pages::Projection::Applied => true,
                crate::pages::Projection::Refused => false,
                crate::pages::Projection::Waiting => return Err(NativeError::History),
            };
            if let EventBody::GrantIssued { grant } = &event.body {
                if accepted {
                    grant_events.insert(grant.id.as_str(), event);
                }
                if accepted
                    && grant.grantor == self.owner
                    && grant.grantee.as_deref() == Some(self.agent.as_str())
                    && grant.scope == GrantScope::Space(space.into())
                    && matches!(grant.capability, Capability::Trust(_))
                {
                    // A new baseline needs an actual owner opening. Old opaque
                    // history supplies neither permission nor model context.
                    readable = opened
                        .as_ref()
                        .is_some_and(|from| from.identity == self.owner);
                    text.clear();
                }
            }
            let supported = match &event.body {
                EventBody::SpaceCreated { .. }
                | EventBody::MemberAdded { .. }
                | EventBody::MemberRemoved { .. }
                | EventBody::SpaceEncrypted
                | EventBody::Checkpoint { .. }
                | EventBody::DeviceJoining { .. } => true,
                EventBody::Sealed { kind } => kind == "Commit" || kind == "Welcome",
                EventBody::GrantIssued { .. } | EventBody::GrantRevoked { .. } => opened.is_some(),
                EventBody::MessagePosted {
                    attaches, reply, ..
                } => attaches.is_none() && reply.is_none() && (opened.is_some() || own_echo),
                _ => false,
            };
            if !supported {
                readable = false;
            }
            // Select by ordered issuance, never a wall-clock event timestamp.
            let active = latest_trust(&ordered, space, &self.owner, &self.agent);
            selected = active.and_then(|grant| {
                grant_events
                    .get(grant.grant.id.as_str())
                    .map(|event| (grant.clone(), *event))
            });
            if let EventBody::MessagePosted { text: message, .. } = &event.body {
                if readable
                    && historical_writers
                    && original.is_none()
                    && (own_echo
                        || opened.as_ref().is_some_and(|from| {
                            from.identity == self.owner || from.identity == self.agent
                        }))
                {
                    if text
                        .len()
                        .checked_add(message.len())
                        .and_then(|n| n.checked_add(1))
                        .is_none_or(|n| n > MAX_CONTEXT_BYTES)
                    {
                        return Err(NativeError::Bounds);
                    }
                    text.push_str(message);
                    text.push('\n');
                }
                if event.hash == trigger {
                    let (grant, grant_event) = selected.as_ref().ok_or(NativeError::Permission)?;
                    if !readable
                        || !historical_writers
                        || event.author != self.owner
                        || opened
                            .as_ref()
                            .is_none_or(|from| from.identity != self.owner)
                        || grant
                            .grant
                            .expires_at_ms
                            .is_some_and(|expiry| expiry <= event.at_ms)
                    {
                        return Err(NativeError::Permission);
                    }
                    original = Some(ReplyIntent {
                        profile: PROFILE.into(),
                        agent: self.agent.clone(),
                        owner: self.owner.clone(),
                        space: space.into(),
                        trigger: event.hash.clone(),
                        trigger_digest: event_log::content_hash_of(event),
                        grant: grant.grant.id.clone(),
                        grant_digest: event_log::content_hash_of(grant_event),
                    });
                }
            }
        }
        let intent = original.ok_or(NativeError::Permission)?;
        let (grant, grant_event) = selected.ok_or(NativeError::Permission)?;
        let Capability::Trust(level) = grant.grant.capability else {
            return Err(NativeError::Permission);
        };
        if !readable
            || !direct_writers(&ordered, space, &self.owner, &self.agent)
            || intent.grant != grant.grant.id
            || intent.grant_digest != event_log::content_hash_of(grant_event)
            || grant
                .grant
                .expires_at_ms
                .is_some_and(|expiry| expiry <= at_ms)
            || !evaluate_permission(level, &ActionClass::Reply, &Policy::default()).acts()
        {
            return Err(NativeError::Permission);
        }
        if self
            .engine
            .state
            .spaces
            .get(space)
            .is_some_and(|space| space.integrity_error.is_some())
        {
            return Err(NativeError::NotReady);
        }
        let epoch = self
            .engine
            .ready_epoch_in(space, &ordered)
            .ok_or(NativeError::NotReady)?;
        let last = log.events().last().ok_or(NativeError::History)?;
        Ok(ReplyFacts {
            intent,
            frontier: NativeFrontier {
                seq: last.seq,
                hash: last.hash.clone(),
                epoch,
            },
            expires_at_ms: grant.grant.expires_at_ms,
            text,
        })
    }
}
