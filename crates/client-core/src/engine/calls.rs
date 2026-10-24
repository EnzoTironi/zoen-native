//! Call authority is a fold of the existing authenticated log, never an SDK store.
use super::*;
use types::calls::{CallEndReason, CallMedia, CallRecord, CallSignal, MAX_RING_MS};

impl State {
    pub(crate) fn call_event_valid(
        &self,
        event: &Event,
        identities: &HashMap<IdentityId, Identity>,
    ) -> bool {
        let EventBody::CallV1 { signal } = &event.body else {
            return true;
        };
        let Some(space) = self.spaces.get(&event.space) else {
            return false;
        };
        let Some(device) = event.device.as_deref() else {
            return false;
        };
        if space.kind != SpaceKind::Direct
            || space.privacy != Privacy::EndToEnd
            || space.members.len() != 2
            || !public_key(device)
            || !space.members.iter().any(|(id, _)| id == &event.author)
            || !space.members.iter().all(|(id, role)| {
                if *role == Role::Reader {
                    return false;
                }
                identities
                    .get(id)
                    .is_some_and(|p| p.kind == IdentityKind::Person)
            })
        {
            return false;
        }
        let call_id = signal.call_id();
        if !call_id.strip_prefix("call_").is_some_and(|id| {
            id.len() == 32
                && id
                    .bytes()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        }) {
            return false;
        }
        match signal {
            CallSignal::Invite {
                callee,
                expires_at_ms,
                ..
            } => {
                callee != &event.author
                    && space.members.iter().any(|(id, _)| id == callee)
                    && event.at_ms >= 0
                    && expires_at_ms
                        .checked_sub(event.at_ms)
                        .is_some_and(|n| n > 0 && n <= MAX_RING_MS)
                    && !space.calls.iter().any(|c| {
                        c.call_id == call_id
                            || (!c.terminal()
                                && (c.accepted_device.is_some() || c.ringing_at(event.at_ms)))
                    })
            }
            CallSignal::Accept { invitation, .. } => space
                .calls
                .iter()
                .find(|c| c.call_id == call_id)
                .is_some_and(|call| {
                    call.invitation == *invitation
                        && !call.terminal()
                        && call.callee == event.author
                        && event.at_ms >= call.invited_at_ms
                        && event.at_ms < call.expires_at_ms
                        && call
                            .accepted_device
                            .as_deref()
                            .is_none_or(|chosen| chosen == device)
                }),
            CallSignal::End {
                invitation, reason, ..
            } => space
                .calls
                .iter()
                .find(|c| c.call_id == call_id)
                .is_some_and(|call| {
                    if call.invitation != *invitation || event.at_ms < 0 {
                        return false;
                    }
                    let caller = call.caller == event.author && call.caller_device == device;
                    let callee = call.callee == event.author
                        && call
                            .accepted_device
                            .as_deref()
                            .is_none_or(|chosen| chosen == device);
                    let permitted = match reason {
                        CallEndReason::Cancelled => caller && call.accepted_device.is_none(),
                        CallEndReason::Declined => callee && call.accepted_device.is_none(),
                        CallEndReason::HungUp => {
                            (caller || callee) && call.accepted_device.is_some()
                        }
                    };
                    permitted && (!call.terminal() || call.end_reason == Some(*reason))
                }),
        }
    }

    pub(crate) fn apply_call(&mut self, event: &Event) {
        let EventBody::CallV1 { signal } = &event.body else {
            return;
        };
        let space = self
            .spaces
            .get_mut(&event.space)
            .expect("validated call Space");
        match signal {
            CallSignal::Invite {
                call_id,
                callee,
                media,
                expires_at_ms,
            } => {
                // An expired or future-dated, unanswered invitation cannot lock the DM.
                // This comparison uses signed event time and canonical order, so replay
                // is deterministic. It never retires an already accepted call.
                for old in &mut space.calls {
                    if !old.terminal()
                        && old.accepted_device.is_none()
                        && !old.ringing_at(event.at_ms)
                    {
                        old.invalidated = true;
                    }
                }
                space.call_transport.retain(|record| {
                    space
                        .calls
                        .iter()
                        .any(|call| call.call_id == record.signal.call_id && !call.terminal())
                });
                space.calls.push(CallRecord {
                    call_id: call_id.clone(),
                    invitation: event_log::content_hash_of(event),
                    caller: event.author.clone(),
                    caller_device: event.device.clone().expect("validated call device"),
                    callee: callee.clone(),
                    media: *media,
                    invited_at_ms: event.at_ms,
                    expires_at_ms: *expires_at_ms,
                    invitation_confirmed: event.seq > 0,
                    accepted_device: None,
                    accepted_at_ms: None,
                    acceptance_confirmed: false,
                    ended_at_ms: None,
                    end_reason: None,
                    invalidated: false,
                });
            }
            CallSignal::Accept { call_id, .. } => {
                let call = space
                    .calls
                    .iter_mut()
                    .find(|c| c.call_id == *call_id)
                    .expect("validated call");
                if call.accepted_device.is_none() {
                    call.accepted_device = event.device.clone();
                    call.accepted_at_ms = Some(event.at_ms);
                    call.acceptance_confirmed = event.seq > 0;
                }
            }
            CallSignal::End {
                call_id, reason, ..
            } => {
                let call = space
                    .calls
                    .iter_mut()
                    .find(|c| c.call_id == *call_id)
                    .expect("validated call");
                if call.end_reason.is_none() {
                    call.ended_at_ms = Some(event.at_ms);
                    call.end_reason = Some(*reason);
                }
                space
                    .call_transport
                    .retain(|record| record.signal.call_id != *call_id);
            }
        }
    }
}

fn public_key(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

impl Engine {
    /// Device-local contact policy gates effects, never the canonical call fold.
    /// Ending/declining and reading history remain possible after blocking.
    pub(crate) fn assert_call_peer_allowed(&self, peer: &str) -> R<()> {
        if self.store.blocked()?.iter().any(|id| id == peer) {
            return Err(CoreError::Forbidden {
                reason: "Call participant is blocked on this device".into(),
            });
        }
        Ok(())
    }

    pub(crate) fn assert_call_owner(&self, identity: &str, device: &str) -> R<()> {
        if !self.is_unlocked()
            || !self
                .account()
                .is_some_and(|account| account.identity == identity && account.device == device)
        {
            return Err(CoreError::Forbidden {
                reason: "Call owner or device changed".into(),
            });
        }
        Ok(())
    }
    pub(crate) fn call_history(&self, space: &str) -> R<Vec<CallRecord>> {
        let me = self.me_id()?;
        if !self.is_member(space, &me) {
            return Err(not_found("Conversation"));
        }
        Ok(self.space_state(space)?.calls.clone())
    }

    pub(crate) fn invite_call(
        &mut self,
        identity: &str,
        device: &str,
        space: &str,
        video: bool,
    ) -> R<CallRecord> {
        self.assert_call_owner(identity, device)?;
        let me = self.me_id()?;
        if !self.net.synced.contains(space) {
            return Err(CoreError::Forbidden {
                reason: "Calls need a relay-synchronized encrypted conversation".into(),
            });
        }
        let state = self.space_state(space)?;
        let callee = state
            .members
            .iter()
            .find(|(id, _)| id != &me)
            .ok_or_else(|| not_found("Call participant"))?
            .0
            .clone();
        self.assert_call_peer_allowed(&callee)?;
        let id = new_secret_id("call")?;
        let at = now_ms();
        self.append_at(
            space,
            &me,
            at,
            EventBody::CallV1 {
                signal: CallSignal::Invite {
                    call_id: id.clone(),
                    callee,
                    media: if video {
                        CallMedia::Video
                    } else {
                        CallMedia::Audio
                    },
                    expires_at_ms: at.saturating_add(MAX_RING_MS),
                },
            },
        )?;
        self.call_history(space)?
            .into_iter()
            .find(|c| c.call_id == id)
            .ok_or_else(|| not_found("Call"))
    }

    pub(crate) fn respond_call(
        &mut self,
        identity: &str,
        device: &str,
        space: &str,
        id: &str,
        invitation: &str,
        action: &str,
    ) -> R<CallRecord> {
        self.assert_call_owner(identity, device)?;
        let me = self.me_id()?;
        if action == "accept" {
            let call = self
                .space_state(space)?
                .calls
                .iter()
                .find(|call| call.call_id == id)
                .ok_or_else(|| not_found("Call"))?;
            self.assert_call_peer_allowed(if call.caller == me {
                &call.callee
            } else {
                &call.caller
            })?;
        }
        let signal = match action {
            "accept" => CallSignal::Accept {
                call_id: id.into(),
                invitation: invitation.into(),
            },
            "cancel" | "decline" | "hang_up" => CallSignal::End {
                call_id: id.into(),
                invitation: invitation.into(),
                reason: match action {
                    "cancel" => CallEndReason::Cancelled,
                    "decline" => CallEndReason::Declined,
                    _ => CallEndReason::HungUp,
                },
            },
            _ => {
                return Err(CoreError::Invalid {
                    reason: "Unknown call action".into(),
                })
            }
        };
        self.append(space, &me, EventBody::CallV1 { signal })?;
        self.call_history(space)?
            .into_iter()
            .find(|c| c.call_id == id)
            .ok_or_else(|| not_found("Call"))
    }
}

#[cfg(test)]
mod tests;
