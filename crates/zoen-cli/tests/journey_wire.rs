//! Wire protocol v2 journeys (S1): signed bytes travel verbatim, causal links hold the
//! relay to one history, and old clients are told to upgrade. Real relay, real Postgres,
//! real `zoen` processes; the "newer client" and the attacker speak the protocol directly.

mod common;
use common::*;
use roda_log::content::{Payload, SignedContent};
use roda_proto::{ErrorCode, ServerFrame, PROTOCOL_VERSION};
use roda_types::{EventBody, Privacy, Role, Seen, SpaceKind};

fn head(ev: &roda_proto::Sequenced) -> Option<Seen> {
    Some(Seen {
        seq: ev.seq,
        hash: ev.hash.clone(),
    })
}

#[tokio::test]
async fn an_event_from_a_newer_client_survives_the_relay_and_an_older_peer() {
    let w = World::new("newer").await;
    w.init("bruno", "Bruno");
    let bruno = w.id_of("bruno").await;

    let mut nova = RawClient::connect(&w.relay_url(), "nova").await;
    let me = nova.identity();
    let space = roda_types::new_id("sp");
    let created = nova
        .publish_body(
            &space,
            None,
            EventBody::SpaceCreated {
                title: "Do futuro".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::Closed,
            },
        )
        .await
        .unwrap();
    let added = nova
        .publish_body(
            &space,
            head(&created),
            EventBody::MemberAdded {
                identity: bruno,
                role: Role::Member,
            },
        )
        .await
        .unwrap();
    nova.publish_body(
        &space,
        head(&added),
        EventBody::MessagePosted {
            message: "m1".into(),
            text: "chegou enquete nova".into(),
            attaches: None,
            reply: None,
        },
    )
    .await
    .unwrap();
    let last = w.events_in(&space).await[2].hash.clone();

    let mut future = SignedContent {
        v: 3,
        space: space.clone(),
        client_id: roda_types::new_ulid(now_ms()),
        author: me,
        device: nova.author.device.clone(),
        at_ms: now_ms(),
        seen: Some(roda_log::content::SeenLink { seq: 2, hash: last }),
        payload: Some(Payload::Body(
            br#"{"PollOpened":{"question":"Praia ou serra?","options":["praia","serra"]}}"#
                .to_vec(),
        )),
    }
    .encode();
    future.extend_from_slice(&[0xA2, 0x06, 0x07, b'f', b'u', b't', b'u', b'r', b'e', b'!']);
    let poll = nova
        .publish_content(future.clone())
        .await
        .expect("the relay sequences a kind it doesn't know");
    assert_eq!(
        poll.env.content(),
        future.as_slice(),
        "the relay echoes the exact bytes"
    );
    assert_eq!(
        w.events_in(&space).await[3].env.content(),
        future.as_slice(),
        "and stores them byte for byte"
    );

    let read = w.zoen("bruno", &["read", "Do futuro"]);
    assert!(read.contains("chegou enquete nova"), "{read}");
    assert!(
        !w.zoen("bruno", &["verify"]).contains("BROKEN"),
        "Bruno's older build still verifies the newer event"
    );

    w.zoen("bruno", &["send", "Do futuro", "serra!"]);
    let after = w.events_in(&space).await;
    assert!(
        after.len() == 5 && after[4].env.author() == w.id_of("bruno").await,
        "Bruno's reply is sequenced on top of the event he can't read"
    );
    assert!(!w.zoen("bruno", &["verify"]).contains("BROKEN"));
}

#[tokio::test]
async fn the_relay_refuses_events_signed_on_a_history_it_doesnt_have() {
    let w = World::new("causal").await;
    let mut m = RawClient::connect(&w.relay_url(), "mallory").await;
    let space = roda_types::new_id("sp");
    let created = m
        .publish_body(
            &space,
            None,
            EventBody::SpaceCreated {
                title: "g".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::Closed,
            },
        )
        .await
        .unwrap();

    let forked = Some(Seen {
        seq: 0,
        hash: "f".repeat(64),
    });
    let r = m
        .publish_body(
            &space,
            forked,
            EventBody::MessagePosted {
                message: "m".into(),
                text: "outra história".into(),
                attaches: None,
                reply: None,
            },
        )
        .await;
    assert_eq!(
        r.unwrap_err(),
        "signed on a history this relay doesn't have"
    );

    let ahead = Some(Seen {
        seq: 7,
        hash: created.hash.clone(),
    });
    let r = m
        .publish_body(
            &space,
            ahead,
            EventBody::MessagePosted {
                message: "m".into(),
                text: "do futuro".into(),
                attaches: None,
                reply: None,
            },
        )
        .await;
    assert_eq!(
        r.unwrap_err(),
        "signed on a history this relay doesn't have"
    );

    let r = m
        .publish_body(
            &space,
            None,
            EventBody::MessagePosted {
                message: "m".into(),
                text: "sem contexto".into(),
                attaches: None,
                reply: None,
            },
        )
        .await;
    assert_eq!(r.unwrap_err(), "an event must say what its author had seen");

    assert!(m
        .publish_body(
            &space,
            head(&created),
            EventBody::MessagePosted {
                message: "m".into(),
                text: "ok".into(),
                attaches: None,
                reply: None,
            }
        )
        .await
        .is_ok());
    assert_eq!(w.events_in(&space).await.len(), 2);
}

#[tokio::test]
async fn an_outdated_client_is_told_to_upgrade() {
    let w = World::new("upgrade").await;
    // Protocol 2 cannot verify sealed v4 entries; protocol 3 sponsors cannot enroll
    // linked devices. Both need an explicit upgrade before authentication.
    for protocol in [2, 3] {
        match hello_only(&w.relay_url(), protocol).await {
            ServerFrame::Error { code, message } => {
                assert_eq!(code, ErrorCode::UpgradeRequired);
                assert!(message.contains("too old"), "{message}");
            }
            other => panic!("expected an upgrade error, got {other:?}"),
        }
    }
    assert!(
        matches!(
            hello_only(&w.relay_url(), PROTOCOL_VERSION + 1).await,
            ServerFrame::Challenge {
                protocol: PROTOCOL_VERSION,
                ..
            }
        ),
        "a newer client talks down to this relay's version"
    );
}
