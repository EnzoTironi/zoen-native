//! Real authenticated socket allocation, independent from unrelated durable work.
use super::*;
use event_log::SpaceLog;
use futures_util::{SinkExt, StreamExt};
use protocol::{
    turn::TurnCredentials, ClientFrame, Envelope, Sequenced, ServerFrame, PROTOCOL_VERSION,
};
use tokio::net::TcpListener;
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};

type Socket = WebSocketStream<tokio::net::TcpStream>;
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

mod lifecycle;

async fn poll_pending<F: std::future::Future>(mut future: std::pin::Pin<&mut F>) {
    std::future::poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
}

struct Pair {
    client: Arc<Client>,
    identity: String,
    device: String,
    peer: String,
    space: String,
}
impl Drop for Pair {
    fn drop(&mut self) {
        self.client.stop_sync();
    }
}
impl Pair {
    async fn new() -> TestResult<(Self, TcpListener)> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let (client, identity, device, peer) =
            account_fixture(&format!("http://{}", listener.local_addr()?));
        let space = "sp_turn_canonical_pair".to_string();
        {
            let mut engine = client.lock();
            // Reuse the real account and author, but create an already canonical
            // pair. TURN is preflight before a call invitation exists.
            let author = engine.net.author.clone().expect("fixture author");
            let mut log = SpaceLog::new(&space);
            for (at, body) in [
                EventBody::SpaceCreated {
                    title: "Canonical pair".into(),
                    kind: SpaceKind::Direct,
                    privacy: Privacy::EndToEnd,
                },
                EventBody::MemberAdded {
                    identity: peer.clone(),
                    role: Role::Member,
                },
            ]
            .into_iter()
            .enumerate()
            {
                let event = author.sign_event(
                    &space,
                    &format!("canonical-{at}"),
                    at as i64,
                    log.head(),
                    body,
                )?;
                let event = log.sequence(event)?.clone();
                let canonical = Sequenced {
                    seq: event.seq,
                    prev: event.prev.clone(),
                    hash: event.hash.clone(),
                    env: Envelope::plain(&event)?,
                };
                assert_eq!(engine.ingest(canonical), crate::sync::Ingest::Applied);
            }
            // An unrelated offline chat and external native upload are retained.
            // The fixture never acknowledges/drops either to make TURN succeed.
            engine.create_synced_space("Unrelated", SpaceKind::Group, Privacy::Closed, &[])?;
            engine.store.queue_external_media(
                &"a".repeat(64),
                &"b".repeat(64),
                &"c".repeat(64),
                105,
            )?;
            let _ = engine.mls_key_packages_to_publish();
            engine.mls_key_packages_published(Ok(()));
        }
        Ok((
            Self {
                client,
                identity,
                device,
                peer,
                space,
            },
            listener,
        ))
    }
    async fn turn(&self) -> Result<CallTurnDto, CoreError> {
        self.client
            .call_turn(
                self.identity.clone(),
                self.device.clone(),
                self.space.clone(),
                self.peer.clone(),
            )
            .await
    }
    fn pending(&self) -> (Vec<u8>, u64) {
        let engine = self.client.lock();
        (
            serde_json::to_vec(
                &engine
                    .store
                    .outbox()
                    .expect("durable outbox")
                    .iter()
                    .map(|row| &row.event)
                    .collect::<Vec<_>>(),
            )
            .expect("outbox encoding"),
            engine.uploads_pending(),
        )
    }
}

async fn receive(socket: &mut Socket) -> TestResult<ClientFrame> {
    loop {
        let frame = tokio::time::timeout(Duration::from_secs(5), socket.next())
            .await?
            .ok_or("socket closed")??;
        if let Message::Binary(bytes) = frame {
            return Ok(ClientFrame::decode(&bytes).map_err(|error| format!("decode: {error:?}"))?);
        }
    }
}
async fn send(socket: &mut Socket, frame: ServerFrame) -> TestResult {
    socket.send(Message::Binary(frame.encode().into())).await?;
    Ok(())
}
async fn authenticated(listener: &TcpListener) -> TestResult<Socket> {
    let (stream, _) = listener.accept().await?;
    let mut socket = tokio_tungstenite::accept_async(stream).await?;
    let ClientFrame::Hello {
        identity,
        device,
        cert,
        ..
    } = receive(&mut socket).await?
    else {
        panic!("signed Hello")
    };
    assert!(event_log::verify_sig(
        &identity,
        &event_log::device_cert_message(&device),
        &cert
    ));
    send(
        &mut socket,
        ServerFrame::Challenge {
            nonce: "turn-readiness".into(),
            relay: "turn-readiness".into(),
            protocol: PROTOCOL_VERSION,
            capabilities: Vec::new(),
            server_time_ms: None,
        },
    )
    .await?;
    let ClientFrame::Auth { sig } = receive(&mut socket).await? else {
        panic!("device Auth")
    };
    assert!(event_log::verify_sig(
        &device,
        &protocol::auth_message("turn-readiness", "turn-readiness"),
        &sig
    ));
    send(
        &mut socket,
        ServerFrame::Ready {
            identity,
            registered: true,
        },
    )
    .await?;
    assert!(matches!(
        receive(&mut socket).await?,
        ClientFrame::Sync { all: true, .. }
    ));
    Ok(socket)
}
fn credentials() -> TurnCredentials {
    let expires_at_ms = (crate::engine::now_ms() / 1000 + 120) * 1000;
    TurnCredentials {
        urls: vec!["turns:turn.example.test:443?transport=tcp".into()],
        username: format!("{}:{}", expires_at_ms / 1000, "a".repeat(64)),
        credential: format!("{}=", "a".repeat(27)),
        expires_at_ms,
    }
}

async fn answer_until_turn(socket: &mut Socket, pair: &Pair) -> TestResult<u64> {
    loop {
        match receive(socket).await? {
            ClientFrame::Req {
                id,
                op: Op::CallTurn { space, peer },
            } => {
                assert_eq!((space, peer), (pair.space.clone(), pair.peer.clone()));
                return Ok(id);
            }
            ClientFrame::Req { id, .. } => {
                send(
                    socket,
                    ServerFrame::Res {
                        id,
                        result: Ok(Reply::Done),
                    },
                )
                .await?
            }
            ClientFrame::Ping => {
                send(
                    socket,
                    ServerFrame::Pong {
                        server_time_ms: None,
                    },
                )
                .await?
            }
            ClientFrame::Publish { .. } | ClientFrame::Ephemeral { .. } => {}
            frame => panic!("unexpected traffic: {frame:?}"),
        }
    }
}

async fn barrier(socket: &mut Socket, client: &Client) -> TestResult {
    let request = client.request(Op::FetchLink {
        id: "turn-ingress-barrier".into(),
    });
    tokio::pin!(request);
    poll_pending(request.as_mut()).await;
    loop {
        match receive(socket).await? {
            ClientFrame::Req {
                id,
                op: Op::FetchLink { id: marker },
            } if marker == "turn-ingress-barrier" => {
                send(
                    socket,
                    ServerFrame::Res {
                        id,
                        result: Ok(Reply::Link(None)),
                    },
                )
                .await?;
                break;
            }
            ClientFrame::Req {
                op: Op::CallTurn { .. },
                ..
            } => panic!("TURN escaped readiness or completed twice"),
            ClientFrame::Req { id, .. } => {
                send(
                    socket,
                    ServerFrame::Res {
                        id,
                        result: Ok(Reply::Done),
                    },
                )
                .await?
            }
            ClientFrame::Ping => {
                send(
                    socket,
                    ServerFrame::Pong {
                        server_time_ms: None,
                    },
                )
                .await?
            }
            ClientFrame::Publish { .. } | ClientFrame::Ephemeral { .. } => {}
            frame => panic!("unexpected barrier traffic: {frame:?}"),
        }
    }
    assert!(matches!(request.await?, Reply::Link(None)));
    Ok(())
}

#[tokio::test]
async fn authenticated_pair_allocates_before_invite_without_draining_unrelated_chat_or_upload(
) -> TestResult {
    let (pair, listener) = Pair::new().await?;
    let before = pair.pending();
    assert!(!before.0.is_empty() && before.1 == 1);
    assert!(pair.client.lock().state.spaces[&pair.space]
        .calls
        .is_empty());
    pair.client.start_sync(None)?;
    let mut socket = authenticated(&listener).await?;
    let turn = pair.turn();
    tokio::pin!(turn);
    poll_pending(turn.as_mut()).await;
    // Ordered RPC ingress proves the real dispatcher processed Ready, while
    // deliberately withholding catch-up. Auth alone cannot allocate TURN.
    barrier(&mut socket, &pair.client).await?;
    assert!(!pair.client.connection().synced);
    send(&mut socket, ServerFrame::SyncDone).await?;
    let relay = async {
        let id = answer_until_turn(&mut socket, &pair).await?;
        send(
            &mut socket,
            ServerFrame::Res {
                id,
                result: Ok(Reply::CallTurn(credentials())),
            },
        )
        .await
    };
    let (result, served) = tokio::join!(turn, relay);
    assert!(
        result.is_ok(),
        "authenticated pair allocation must not wait for unrelated work: {result:?}"
    );
    served?;
    assert_eq!(
        pair.pending(),
        before,
        "allocation cannot retire unrelated durable work"
    );
    assert!(
        pair.client.lock().state.spaces[&pair.space]
            .calls
            .is_empty(),
        "transport preflight creates no invitation or media authority"
    );
    Ok(())
}
