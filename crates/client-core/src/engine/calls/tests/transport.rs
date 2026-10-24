use super::*;
use types::call_transport::{CallTransportPayload as P, CallTransportSignal};

fn signal(
    f: &Fixture,
    call: &CallRecord,
    payload: P,
) -> std::result::Result<CallTransportSignal, Box<dyn std::error::Error>> {
    Ok(CallTransportSignal {
        signal_id: new_secret_id("tx")?,
        call_id: call.call_id.clone(),
        invitation: call.invitation.clone(),
        recipient_device: f.callee.device.clone().unwrap(),
        generation: 0,
        payload,
    })
}
fn event(
    f: &Fixture,
    by: &Author,
    signal: CallTransportSignal,
) -> std::result::Result<Event, Box<dyn std::error::Error>> {
    Ok(by.sign_event(
        "sp_calls",
        &new_ulid(400)?,
        400,
        f.log.head(),
        EventBody::CallTransportV1 { signal },
    )?)
}
fn accepted() -> std::result::Result<(Fixture, CallRecord), Box<dyn std::error::Error>> {
    let mut f = Fixture::new()?;
    let call = f.invite()?;
    let accept = f.accept(&call);
    f.push(f.callee.clone(), 200, EventBody::CallV1 { signal: accept })?;
    Ok((f, call))
}

#[test]
fn no_sdp_or_ice_before_both_events_are_canonically_confirmed() {
    let (mut f, call) = accepted().unwrap();
    let offer = event(
        &f,
        &f.caller,
        signal(
            &f,
            &call,
            P::Offer {
                sdp: "v=0\r\na=fingerprint:sha-256 exact\r\n".into(),
            },
        )
        .unwrap(),
    )
    .unwrap();
    assert!(f.engine.state.call_transport_valid(&offer));
    f.engine.state.spaces.get_mut("sp_calls").unwrap().calls[0].acceptance_confirmed = false;
    assert!(!f.engine.state.call_transport_valid(&offer));
    f.engine.state.spaces.get_mut("sp_calls").unwrap().calls[0].acceptance_confirmed = true;
    f.engine.state.spaces.get_mut("sp_calls").unwrap().calls[0].invitation_confirmed = false;
    assert!(!f.engine.state.call_transport_valid(&offer));
}

#[test]
fn selected_devices_only_negotiate_and_wrong_invitation_or_recipient_is_refused() {
    let (f, call) = accepted().unwrap();
    let base = signal(
        &f,
        &call,
        P::Offer {
            sdp: "offer".into(),
        },
    )
    .unwrap();
    assert!(!f
        .engine
        .state
        .call_transport_valid(&event(&f, &f.other_device, base.clone()).unwrap()));
    let mut wrong = base.clone();
    wrong.recipient_device = f.other_device.device.clone().unwrap();
    assert!(!f
        .engine
        .state
        .call_transport_valid(&event(&f, &f.caller, wrong).unwrap()));
    let mut wrong = base.clone();
    wrong.invitation = "another valid invitation".into();
    assert!(!f
        .engine
        .state
        .call_transport_valid(&event(&f, &f.caller, wrong).unwrap()));
    assert!(!f
        .engine
        .state
        .call_transport_valid(&event(&f, &f.callee, base).unwrap()));
}

#[test]
fn accepted_device_transport_replays_once_and_terminal_calls_refuse_old_signals() {
    let (mut f, call) = accepted().unwrap();
    let offer = signal(
        &f,
        &call,
        P::Offer {
            sdp: "v=0\r\na=fingerprint:sha-256 exact\r\n".into(),
        },
    )
    .unwrap();
    f.push(
        f.caller.clone(),
        300,
        EventBody::CallTransportV1 {
            signal: offer.clone(),
        },
    )
    .unwrap();
    f.push(
        f.caller.clone(),
        301,
        EventBody::CallTransportV1 {
            signal: offer.clone(),
        },
    )
    .unwrap();
    assert_eq!(f.engine.state.spaces["sp_calls"].call_transport.len(), 1);
    let image = f.engine.store.memory_image().unwrap();
    let reopened =
        Engine::from_store(Store::from_memory_image(&image).unwrap(), ":memory:").unwrap();
    assert_eq!(
        reopened.state.spaces["sp_calls"].call_transport[0].signal,
        offer
    );
    let end = f.end(&call, CallEndReason::HungUp);
    f.push(f.callee.clone(), 350, EventBody::CallV1 { signal: end })
        .unwrap();
    assert!(f.engine.state.spaces["sp_calls"].call_transport.is_empty());
    let image = f.engine.store.memory_image().unwrap();
    let reopened =
        Engine::from_store(Store::from_memory_image(&image).unwrap(), ":memory:").unwrap();
    assert!(reopened.state.spaces["sp_calls"].call_transport.is_empty());
    assert!(!f
        .engine
        .state
        .call_transport_valid(&event(&f, &f.caller, offer).unwrap()));
}

#[test]
fn one_offer_one_answer_per_generation_and_bounds_prevent_unlimited_log_growth() {
    let (mut f, call) = accepted().unwrap();
    let offer = signal(
        &f,
        &call,
        P::Offer {
            sdp: "offer".into(),
        },
    )
    .unwrap();
    f.push(
        f.caller.clone(),
        300,
        EventBody::CallTransportV1 { signal: offer },
    )
    .unwrap();
    let second = signal(
        &f,
        &call,
        P::Offer {
            sdp: "different offer".into(),
        },
    )
    .unwrap();
    assert!(!f
        .engine
        .state
        .call_transport_valid(&event(&f, &f.caller, second).unwrap()));
    let mut answer = signal(
        &f,
        &call,
        P::Answer {
            sdp: "answer".into(),
        },
    )
    .unwrap();
    answer.recipient_device = f.caller.device.clone().unwrap();
    assert!(f
        .engine
        .state
        .call_transport_valid(&event(&f, &f.callee, answer.clone()).unwrap()));
    f.push(
        f.callee.clone(),
        301,
        EventBody::CallTransportV1 { signal: answer },
    )
    .unwrap();
    let mut restart = signal(
        &f,
        &call,
        P::Offer {
            sdp: "restart".into(),
        },
    )
    .unwrap();
    restart.generation = 1;
    assert!(f
        .engine
        .state
        .call_transport_valid(&event(&f, &f.caller, restart.clone()).unwrap()));
    restart.generation = 8;
    assert!(!f
        .engine
        .state
        .call_transport_valid(&event(&f, &f.caller, restart).unwrap()));
    let huge = signal(
        &f,
        &call,
        P::Offer {
            sdp: "x".repeat(types::call_transport::MAX_SDP_BYTES + 1),
        },
    )
    .unwrap();
    assert!(!f
        .engine
        .state
        .call_transport_valid(&event(&f, &f.caller, huge).unwrap()));
}

#[test]
fn cold_orphan_is_retired_by_normal_signed_hangup_before_a_new_invite() {
    let (mut f, call) = accepted().unwrap();
    let image = f.engine.store.memory_image().unwrap();
    f.engine = Engine::from_store(Store::from_memory_image(&image).unwrap(), ":memory:").unwrap();
    assert!(f.record().accepted_device.is_some() && !f.record().terminal());
    let end = f.end(&call, CallEndReason::HungUp);
    f.push(f.caller.clone(), 500, EventBody::CallV1 { signal: end })
        .unwrap();
    let next = CallSignal::Invite {
        call_id: new_secret_id("call").unwrap(),
        callee: f.callee.identity.clone(),
        media: CallMedia::Video,
        expires_at_ms: 1500,
    };
    assert!(f.valid(&f.event(&f.caller, 600, next).unwrap()));
    assert!(f.record().terminal());
}

#[test]
fn role_revocation_retires_media_and_refuses_future_call_admission() {
    let (mut f, call) = accepted().unwrap();
    let offer = signal(
        &f,
        &call,
        P::Offer {
            sdp: "offer".into(),
        },
    )
    .unwrap();
    f.push(
        f.caller.clone(),
        300,
        EventBody::CallTransportV1 {
            signal: offer.clone(),
        },
    )
    .unwrap();
    f.push(
        f.caller.clone(),
        350,
        EventBody::MemberAdded {
            identity: f.callee.identity.clone(),
            role: Role::Reader,
        },
    )
    .unwrap();
    assert!(f.record().terminal());
    assert!(f.engine.state.spaces["sp_calls"].call_transport.is_empty());
    assert!(!f
        .engine
        .state
        .call_transport_valid(&event(&f, &f.caller, offer).unwrap()));
    let next = CallSignal::Invite {
        call_id: new_secret_id("call").unwrap(),
        callee: f.callee.identity.clone(),
        media: CallMedia::Audio,
        expires_at_ms: 1500,
    };
    assert!(!f.valid(&f.event(&f.caller, 400, next).unwrap()));
}

mod confirmation;
