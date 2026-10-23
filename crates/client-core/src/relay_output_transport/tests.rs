//! A refused binding never exposes a usable publication/upload transport.
use super::*;
use protocol::runtime_binding::Request;
use tokio::net::TcpListener;

fn expected(actor: &Author, nonce: u8) -> Expectation {
    Expectation::new(
        &"12".repeat(32),
        Request {
            namespace: "runtime-a".into(),
            nonce: vec![nonce; 32],
        },
        "cell-a",
        "relay-a",
        &actor.identity,
        &actor.key.id(),
    )
    .unwrap()
}

#[tokio::test]
async fn requires_negotiated_capability_and_exact_fresh_reply_before_effects() {
    for mode in [
        "valid",
        "unsupported",
        "replayed",
        "wrong-id",
        "wrong-actor",
    ] {
        let root = event_log::Signer::generate();
        let actor = Author::device(&root, event_log::Signer::generate());
        let expected = expected(&actor, 41);
        let request = expected.request().clone();
        let proof = match mode {
            "replayed" => self::expected(&actor, 42).proof(),
            "wrong-actor" => {
                let other = Author::device(&root, event_log::Signer::generate());
                self::expected(&other, 41).proof()
            }
            _ => expected.proof(),
        };
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let (identity, device) = (actor.identity.clone(), actor.key.id());
        let task = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(socket).await.unwrap();
            let hello = socket.next().await.unwrap().unwrap().into_data();
            assert!(
                matches!(ClientFrame::decode(&hello).unwrap(), ClientFrame::Hello { identity: i, .. } if i == identity)
            );
            socket
                .send(Message::Binary(
                    ServerFrame::Challenge {
                        nonce: "fixture-challenge".into(),
                        relay: "relay-a".into(),
                        protocol: protocol::PROTOCOL_VERSION,
                        capabilities: if mode == "unsupported" {
                            vec![]
                        } else {
                            vec![CAPABILITY.into()]
                        },
                        server_time_ms: 0,
                    }
                    .encode()
                    .into(),
                ))
                .await
                .unwrap();
            let auth = socket.next().await.unwrap().unwrap().into_data();
            let ClientFrame::Auth { sig } = ClientFrame::decode(&auth).unwrap() else {
                panic!("fixture Auth expected")
            };
            assert!(event_log::verify_sig(
                &device,
                &protocol::auth_message("fixture-challenge", "relay-a"),
                &sig
            ));
            socket
                .send(Message::Binary(
                    ServerFrame::Ready {
                        identity,
                        registered: true,
                    }
                    .encode()
                    .into(),
                ))
                .await
                .unwrap();
            if mode != "unsupported" {
                let next = socket.next().await.unwrap().unwrap().into_data();
                assert_eq!(
                    ClientFrame::decode(&next).unwrap(),
                    ClientFrame::Req {
                        id: 1,
                        op: protocol::Op::RuntimeBinding(request)
                    }
                );
                socket
                    .send(Message::Binary(
                        ServerFrame::Res {
                            id: if mode == "wrong-id" { 2 } else { 1 },
                            result: Ok(protocol::Reply::RuntimeBinding(proof)),
                        }
                        .encode()
                        .into(),
                    ))
                    .await
                    .unwrap();
            }
            // Dropping a refused/finished transport must not send a fresh effect.
            if let Ok(Some(Ok(Message::Binary(bytes)))) =
                tokio::time::timeout(Duration::from_secs(2), socket.next()).await
            {
                panic!(
                    "unexpected frame after binding: {:?}",
                    ClientFrame::decode(&bytes)
                );
            }
        });
        let result = RelayOutputTransport::connect_bound(&url, "relay-a", actor, expected).await;
        if mode == "valid" {
            assert!(result.is_ok());
        } else {
            assert!(matches!(result, Err(RelayOutputError::Refused)));
        }
        drop(result);
        task.await.unwrap();
    }
}
