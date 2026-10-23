//! WebRTC owns transport mechanics; the Core owns which two devices may negotiate.
use super::*;
use types::call_transport::{
    CallTransportPayload as P, CallTransportRecord, CallTransportSignal, MAX_NEGOTIATION_BYTES,
    MAX_NEGOTIATION_SIGNALS,
};

impl State {
    pub(crate) fn call_transport_valid(&self, event: &Event) -> bool {
        let EventBody::CallTransportV1 { signal } = &event.body else {
            return true;
        };
        let Some(space) = self.spaces.get(&event.space) else {
            return false;
        };
        let Some(call) = space.calls.iter().find(|c| c.call_id == signal.call_id) else {
            return false;
        };
        let Some(sender) = event.device.as_deref() else {
            return false;
        };
        let Some(callee_device) = call.accepted_device.as_deref() else {
            return false;
        };
        if call.terminal()
            || !call.invitation_confirmed
            || !call.acceptance_confirmed
            || signal.invitation != call.invitation
            || signal.generation > 7
            || !signal.payload.valid_shape()
            || !signal.signal_id.strip_prefix("tx_").is_some_and(|id| {
                id.len() == 32
                    && id
                        .bytes()
                        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
            })
        {
            return false;
        }
        let caller = event.author == call.caller
            && sender == call.caller_device
            && signal.recipient_device == callee_device;
        let callee = event.author == call.callee
            && sender == callee_device
            && signal.recipient_device == call.caller_device;
        if !(caller || callee)
            || (matches!(signal.payload, P::Offer { .. }) && !caller)
            || (matches!(signal.payload, P::Answer { .. }) && !callee)
        {
            return false;
        }
        let previous: Vec<_> = space
            .call_transport
            .iter()
            .filter(|r| r.signal.call_id == call.call_id)
            .collect();
        if let Some(duplicate) = previous
            .iter()
            .find(|r| r.signal.signal_id == signal.signal_id)
        {
            return duplicate.sender_device == sender && duplicate.signal == *signal;
        }
        if previous.len() >= MAX_NEGOTIATION_SIGNALS
            || previous
                .iter()
                .map(|r| r.signal.payload.byte_len())
                .sum::<usize>()
                + signal.payload.byte_len()
                > MAX_NEGOTIATION_BYTES
        {
            return false;
        }
        let generation = previous
            .iter()
            .map(|r| r.signal.generation)
            .max()
            .unwrap_or(0);
        if signal.generation < generation || signal.generation > generation.saturating_add(1) {
            return false;
        }
        if signal.generation > generation
            && !previous.iter().any(|r| {
                r.signal.generation == generation && matches!(r.signal.payload, P::Answer { .. })
            })
        {
            return false;
        }
        let same: Vec<_> = previous
            .iter()
            .filter(|r| r.signal.generation == signal.generation)
            .collect();
        match &signal.payload {
            P::Offer { .. } => !same
                .iter()
                .any(|r| matches!(r.signal.payload, P::Offer { .. })),
            P::Answer { .. } => {
                same.iter()
                    .any(|r| matches!(r.signal.payload, P::Offer { .. }))
                    && !same
                        .iter()
                        .any(|r| matches!(r.signal.payload, P::Answer { .. }))
            }
            P::Ice { .. } => {
                signal.generation == generation
                    || same
                        .iter()
                        .any(|r| matches!(r.signal.payload, P::Offer { .. }))
            }
        }
    }

    pub(crate) fn apply_call_transport(&mut self, event: &Event) {
        let EventBody::CallTransportV1 { signal } = &event.body else {
            return;
        };
        let space = self
            .spaces
            .get_mut(&event.space)
            .expect("validated call transport Space");
        if !space
            .call_transport
            .iter()
            .any(|record| record.signal.signal_id == signal.signal_id)
        {
            space.call_transport.push(CallTransportRecord {
                sender_device: event.device.clone().expect("validated device"),
                signal: signal.clone(),
            });
        }
    }
}

impl Engine {
    pub(crate) fn call_transport_read(
        &self,
        identity: &str,
        device: &str,
        space: &str,
        id: &str,
    ) -> R<Vec<CallTransportRecord>> {
        self.assert_call_owner(identity, device)?;
        let state = self.space_state(space)?;
        let call = state
            .calls
            .iter()
            .find(|c| c.call_id == id)
            .ok_or_else(|| not_found("Call"))?;
        self.assert_call_peer_allowed(if call.caller == identity {
            &call.callee
        } else {
            &call.caller
        })?;
        let caller = call.caller == identity && call.caller_device == device;
        let callee = call.callee == identity && call.accepted_device.as_deref() == Some(device);
        if call.terminal() || !(caller || callee) {
            return Err(CoreError::Forbidden {
                reason: "Call device is no longer authorized".into(),
            });
        }
        Ok(state
            .call_transport
            .iter()
            .filter(|r| r.signal.call_id == id && r.signal.recipient_device == device)
            .cloned()
            .collect())
    }

    pub(crate) fn call_transport_send(
        &mut self,
        identity: &str,
        device: &str,
        space: &str,
        signal: CallTransportSignal,
    ) -> R<()> {
        self.assert_call_owner(identity, device)?;
        let call = self
            .space_state(space)?
            .calls
            .iter()
            .find(|call| call.call_id == signal.call_id)
            .ok_or_else(|| not_found("Call"))?;
        self.assert_call_peer_allowed(if call.caller == identity {
            &call.callee
        } else {
            &call.caller
        })?;
        // The stable signal ID makes an ambiguous bridge retry idempotent, including
        // an intent restored from the existing durable outbox after process death.
        if let Some(record) = self
            .space_state(space)?
            .call_transport
            .iter()
            .find(|r| r.signal.signal_id == signal.signal_id)
        {
            if record.sender_device == device && record.signal == signal {
                return Ok(());
            }
            return Err(CoreError::Forbidden {
                reason: "Call signal ID was reused for another intent".into(),
            });
        }
        self.append(space, identity, EventBody::CallTransportV1 { signal })?;
        Ok(())
    }
}
