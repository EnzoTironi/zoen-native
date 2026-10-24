use super::*;
use types::call_transport::{CallTransportPayload, CallTransportSignal};

fn own(f: &mut Fixture, caller: bool) {
    let by = if caller {
        f.caller.clone()
    } else {
        f.callee.clone()
    };
    f.engine.store.set_meta("me", &by.identity).unwrap();
    f.engine.me = Some(by.identity.clone());
    f.engine.net.account = Some(crate::sync::AccountMeta {
        identity: by.identity.clone(),
        device: by.device.clone().unwrap(),
        cert: by.cert.clone().unwrap(),
        relay_url: "http://127.0.0.1:1".into(),
        registered: true,
    });
    f.engine.net.author = Some(by);
}
fn blocked<T>(result: R<T>) {
    assert!(
        matches!(result, Err(CoreError::Forbidden { reason }) if reason == "Call participant is blocked on this device")
    );
}

#[test]
fn blocked_peer_cannot_be_invited_or_accepted_and_decline_remains_available() {
    let mut f = Fixture::new().unwrap();
    own(&mut f, true);
    f.engine.net.synced.insert("sp_calls".into());
    f.engine.store.block(&f.callee.identity, now_ms()).unwrap();
    let before = f.engine.store.event_count().unwrap();
    blocked(f.engine.invite_call(
        &f.caller.identity,
        f.caller.device.as_deref().unwrap(),
        "sp_calls",
        false,
    ));
    assert_eq!(f.engine.store.event_count().unwrap(), before);
    assert!(f.engine.store.outbox().unwrap().is_empty());

    // Incoming canonical history still projects, independently of local block policy.
    let call = f.invite();
    own(&mut f, false);
    f.engine.store.block(&f.caller.identity, now_ms()).unwrap();
    let before = f.engine.store.event_count().unwrap();
    blocked(f.engine.respond_call(
        &f.callee.identity,
        f.callee.device.as_deref().unwrap(),
        "sp_calls",
        &call.call_id,
        &call.invitation,
        "accept",
    ));
    assert_eq!(f.engine.store.event_count().unwrap(), before);
    assert_eq!(f.engine.call_history("sp_calls").unwrap().len(), 1);
    // Decline is a normal signed terminal event, even for an expired/blocked invite.
    f.engine.net.synced.remove("sp_calls");
    let result = f
        .engine
        .respond_call(
            &f.callee.identity,
            f.callee.device.as_deref().unwrap(),
            "sp_calls",
            &call.call_id,
            &call.invitation,
            "decline",
        )
        .unwrap();
    assert!(result.terminal());
}

#[test]
fn blocking_live_peer_fences_negotiation_but_not_history_or_hangup() {
    let mut f = Fixture::new().unwrap();
    let call = f.invite();
    let accept = f.accept(&call);
    f.push(f.callee.clone(), 200, EventBody::CallV1 { signal: accept });
    own(&mut f, true);
    let signal = CallTransportSignal {
        signal_id: new_secret_id("tx").unwrap(),
        call_id: call.call_id.clone(),
        invitation: call.invitation.clone(),
        recipient_device: f.callee.device.clone().unwrap(),
        generation: 0,
        payload: CallTransportPayload::Offer {
            sdp: "v=0\r\na=fingerprint:sha-256 authenticated\r\n".into(),
        },
    };
    f.engine.store.block(&f.callee.identity, now_ms()).unwrap();
    let before = f.engine.store.event_count().unwrap();
    blocked(f.engine.call_transport_read(
        &f.caller.identity,
        f.caller.device.as_deref().unwrap(),
        "sp_calls",
        &call.call_id,
    ));
    blocked(f.engine.call_transport_send(
        &f.caller.identity,
        f.caller.device.as_deref().unwrap(),
        "sp_calls",
        signal.clone(),
    ));
    assert_eq!(f.engine.store.event_count().unwrap(), before);
    assert!(f.engine.store.outbox().unwrap().is_empty());
    assert_eq!(f.engine.call_history("sp_calls").unwrap().len(), 1);
    f.engine.store.unblock(&f.callee.identity).unwrap();
    f.engine
        .call_transport_send(
            &f.caller.identity,
            f.caller.device.as_deref().unwrap(),
            "sp_calls",
            signal.clone(),
        )
        .unwrap();
    f.engine.store.block(&f.callee.identity, now_ms()).unwrap();
    // Even an idempotent retry cannot return negotiated data to a blocked owner.
    blocked(f.engine.call_transport_send(
        &f.caller.identity,
        f.caller.device.as_deref().unwrap(),
        "sp_calls",
        signal,
    ));
    let ended = f
        .engine
        .respond_call(
            &f.caller.identity,
            f.caller.device.as_deref().unwrap(),
            "sp_calls",
            &call.call_id,
            &call.invitation,
            "hang_up",
        )
        .unwrap();
    assert!(ended.terminal());
    assert!(f.engine.state.spaces["sp_calls"].call_transport.is_empty());
}
