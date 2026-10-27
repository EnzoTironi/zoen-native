//! Exercise the actual optimistic append -> Sequenced ingestion path, without a
//! reload or manually changing confirmation flags to stand in for relay ACKs.
use super::*;
use crate::sync::Ingest;
use protocol::{Envelope, Sequenced};

fn deliver(
    f: &mut Fixture,
    pending: Event,
    expected: Ingest,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let event = f.log.sequence(pending)?.clone();
    assert_eq!(
        f.engine.ingest(Sequenced {
            seq: event.seq,
            prev: event.prev.clone(),
            hash: event.hash.clone(),
            env: Envelope::plain(&event)?,
        }),
        expected
    );
    Ok(())
}

#[test]
fn own_invite_and_accept_ack_reproject_confirmation_before_media_without_reload(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    own_confirmation(false)
}

#[test]
fn own_invite_and_accept_confirmation_does_not_drain_unrelated_chat_or_upload(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    own_confirmation(true)
}

fn own_confirmation(unrelated_work: bool) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::new().unwrap();
    if unrelated_work {
        f.engine.append_synced(
            "sp_unrelated_call_backlog",
            &f.caller,
            1,
            EventBody::SpaceCreated {
                title: "Unrelated pending chat".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::Closed,
            },
        )?;
        f.engine.store.queue_external_media(
            &"a".repeat(64),
            &"b".repeat(64),
            &"c".repeat(64),
            105,
        )?;
    }
    let unrelated: Vec<_> = f
        .engine
        .store
        .outbox()?
        .into_iter()
        .map(|row| row.event)
        .collect();
    let invite = f
        .engine
        .append_synced(
            "sp_calls",
            &f.caller,
            100,
            EventBody::CallV1 {
                signal: CallSignal::Invite {
                    call_id: new_secret_id("call").unwrap(),
                    callee: f.callee.identity.clone(),
                    media: CallMedia::Audio,
                    expires_at_ms: 1100,
                },
            },
        )
        .unwrap();
    let initial = f.record();
    assert!(!initial.invitation_confirmed);
    deliver(&mut f, invite, Ingest::Confirmed)?;
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
    )
    .unwrap();
    assert!(!f
        .engine
        .state
        .call_transport_valid(&event(&f, &f.caller, offer.clone()).unwrap()));
    deliver(&mut f, accept, Ingest::Confirmed)?;
    assert!(f.record().invitation_confirmed && f.record().acceptance_confirmed);
    assert!(f
        .engine
        .state
        .call_transport_valid(&event(&f, &f.caller, offer).unwrap()));
    assert_eq!(
        f.engine
            .store
            .outbox()?
            .into_iter()
            .map(|row| row.event)
            .collect::<Vec<_>>(),
        unrelated
    );
    assert_eq!(f.engine.uploads_pending(), u64::from(unrelated_work));
    Ok(())
}

#[test]
fn canonical_other_device_accept_overrides_local_accept_before_any_media(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::new().unwrap();
    let call = f.invite().unwrap();
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
    let first = f.event(&f.callee, 200, f.accept(&call)).unwrap();
    deliver(&mut f, first, Ingest::Applied)?;
    assert_eq!(f.record().accepted_device, f.callee.device);
    assert!(f.record().acceptance_confirmed);
    let mut answer = signal(
        &f,
        &call,
        P::Answer {
            sdp: "answer".into(),
        },
    )
    .unwrap();
    answer.recipient_device = f.caller.device.clone().unwrap();
    assert!(!f
        .engine
        .state
        .call_transport_valid(&event(&f, &f.other_device, answer.clone()).unwrap()));
    deliver(&mut f, pending, Ingest::Confirmed)?;
    assert_eq!(f.record().accepted_device, f.callee.device);
    assert!(!f
        .engine
        .state
        .call_transport_valid(&event(&f, &f.other_device, answer).unwrap()));
    assert!(f.engine.store.outbox().unwrap().is_empty());
    Ok(())
}

#[test]
fn accepted_before_queued_cancel_recovers_through_signed_hangup(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::new().unwrap();
    let call = f.invite().unwrap();
    let cancel = f.end(&call, CallEndReason::Cancelled);
    let pending = f
        .engine
        .append_synced(
            "sp_calls",
            &f.caller,
            201,
            EventBody::CallV1 { signal: cancel },
        )
        .unwrap();
    assert!(f.record().terminal());
    let accepted = f.event(&f.callee, 200, f.accept(&call)).unwrap();
    deliver(&mut f, accepted, Ingest::Applied)?;
    assert!(!f.record().terminal() && f.record().acceptance_confirmed);
    deliver(&mut f, pending, Ingest::Confirmed)?;
    assert!(!f.record().terminal());
    let hangup = f.end(&call, CallEndReason::HungUp);
    let retirement = f
        .engine
        .append_synced(
            "sp_calls",
            &f.caller,
            250,
            EventBody::CallV1 { signal: hangup },
        )
        .unwrap();
    deliver(&mut f, retirement, Ingest::Confirmed)?;
    assert!(f.record().terminal());
    assert_eq!(f.record().end_reason, Some(CallEndReason::HungUp));
    Ok(())
}
