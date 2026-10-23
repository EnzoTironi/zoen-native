//! Exercise the actual optimistic append -> Sequenced ingestion path, without a
//! reload or manually changing confirmation flags to stand in for relay ACKs.
use super::*;
use crate::sync::Ingest;
use protocol::{Envelope, Sequenced};

fn deliver(f: &mut Fixture, pending: Event, expected: Ingest) {
    let event = f.log.sequence(pending).clone();
    assert_eq!(
        f.engine.ingest(Sequenced {
            seq: event.seq,
            prev: event.prev.clone(),
            hash: event.hash.clone(),
            env: Envelope::plain(&event),
        }),
        expected
    );
}

#[test]
fn own_invite_and_accept_ack_reproject_confirmation_before_media_without_reload() {
    let mut f = Fixture::new();
    let invite = f
        .engine
        .append_synced(
            "sp_calls",
            &f.caller,
            100,
            EventBody::CallV1 {
                signal: CallSignal::Invite {
                    call_id: new_secret_id("call"),
                    callee: f.callee.identity.clone(),
                    media: CallMedia::Audio,
                    expires_at_ms: 1100,
                },
            },
        )
        .unwrap();
    let initial = f.record();
    assert!(!initial.invitation_confirmed);
    deliver(&mut f, invite, Ingest::Confirmed);
    assert!(f.record().invitation_confirmed);
    assert_eq!(f.record().invitation, initial.invitation);
    let acceptance = f.accept(&initial);
    let accept = f
        .engine
        .append_synced(
            "sp_calls",
            &f.callee,
            200,
            EventBody::CallV1 { signal: acceptance },
        )
        .unwrap();
    assert!(!f.record().acceptance_confirmed);
    let offer = signal(
        &f,
        &initial,
        P::Offer {
            sdp: "offer".into(),
        },
    );
    assert!(!f
        .engine
        .state
        .call_transport_valid(&event(&f, &f.caller, offer.clone())));
    deliver(&mut f, accept, Ingest::Confirmed);
    assert!(f.record().invitation_confirmed && f.record().acceptance_confirmed);
    assert!(f
        .engine
        .state
        .call_transport_valid(&event(&f, &f.caller, offer)));
    assert!(f.engine.store.outbox().unwrap().is_empty());
}

#[test]
fn canonical_other_device_accept_overrides_local_accept_before_any_media() {
    let mut f = Fixture::new();
    let call = f.invite();
    let acceptance = f.accept(&call);
    let pending = f
        .engine
        .append_synced(
            "sp_calls",
            &f.other_device,
            201,
            EventBody::CallV1 { signal: acceptance },
        )
        .unwrap();
    assert_eq!(f.record().accepted_device, f.other_device.device);
    assert!(!f.record().acceptance_confirmed);
    let first = f.event(&f.callee, 200, f.accept(&call));
    deliver(&mut f, first, Ingest::Applied);
    assert_eq!(f.record().accepted_device, f.callee.device);
    assert!(f.record().acceptance_confirmed);
    let mut answer = signal(
        &f,
        &call,
        P::Answer {
            sdp: "answer".into(),
        },
    );
    answer.recipient_device = f.caller.device.clone().unwrap();
    assert!(!f
        .engine
        .state
        .call_transport_valid(&event(&f, &f.other_device, answer.clone())));
    deliver(&mut f, pending, Ingest::Confirmed);
    assert_eq!(f.record().accepted_device, f.callee.device);
    assert!(!f
        .engine
        .state
        .call_transport_valid(&event(&f, &f.other_device, answer)));
    assert!(f.engine.store.outbox().unwrap().is_empty());
}
