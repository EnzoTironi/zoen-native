//! Actual WebSocket barrier and publication assertions for the upload fixture.
use super::*;

pub(super) struct Relay<'a> {
    pub(super) listener: &'a tokio::net::TcpListener,
    pub(super) retrying: bool,
    pub(super) commands_tx: &'a mpsc::UnboundedSender<Cmd>,
    pub(super) sender: &'a Client,
    pub(super) queued: Vec<(String, Vec<u8>)>,
    pub(super) release_http: std::sync::mpsc::Sender<()>,
    pub(super) head: Option<Seen>,
    pub(super) receiver: &'a mut Engine,
    pub(super) space: String,
    pub(super) close_socket: oneshot::Receiver<()>,
}

pub(super) async fn drive(input: Relay<'_>) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let Relay {
        listener,
        retrying,
        commands_tx,
        sender,
        queued,
        release_http,
        mut head,
        receiver,
        space,
        close_socket,
    } = input;

    let mut socket = catch_up_socket(listener).await;
    reply(&mut socket, ServerFrame::SyncDone).await;
    if retrying {
        let (answer, answered) = oneshot::channel();
        commands_tx.send(Cmd::Flush).unwrap();
        commands_tx
            .send(Cmd::Req {
                op: Op::FetchLink {
                    id: "upload-not-confirmed".into(),
                },
                reply: answer,
            })
            .unwrap();
        loop {
            match request(&mut socket).await {
                ClientFrame::Req {
                    id,
                    op: Op::FetchLink { id: marker },
                } if marker == "upload-not-confirmed" => {
                    reply(
                        &mut socket,
                        ServerFrame::Res {
                            id,
                            result: Ok(Reply::Link(None)),
                        },
                    )
                    .await;
                    break;
                }
                ClientFrame::Req { id, .. } => {
                    reply(
                        &mut socket,
                        ServerFrame::Res {
                            id,
                            result: Ok(Reply::Done),
                        },
                    )
                    .await
                }
                ClientFrame::Ping => {
                    reply(
                        &mut socket,
                        ServerFrame::Pong {
                            server_time_ms: None,
                        },
                    )
                    .await
                }
                ClientFrame::Ephemeral { .. } => {}
                other => panic!("publication before actual upload completion: {other:?}"),
            }
        }
        assert!(matches!(answered.await.unwrap(), Ok(Reply::Link(None))));
        assert_eq!(sender.lock().pending_uploads(8), queued);
        release_http.send(()).unwrap();
    }
    let marker = async {
        while sender
            .lock()
            .store
            .outbox()
            .unwrap()
            .iter()
            .any(|row| !row.failed)
        {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        let (answer, answered) = oneshot::channel();
        commands_tx
            .send(Cmd::Req {
                op: Op::FetchLink {
                    id: "upload-quiescent".into(),
                },
                reply: answer,
            })
            .unwrap();
        assert!(matches!(answered.await.unwrap(), Ok(Reply::Link(None))));
        commands_tx.send(Cmd::Stop).unwrap();
    };
    let wire = async {
        loop {
            match request(&mut socket).await {
                ClientFrame::Publish { env } => {
                    let cid = env.client_id().to_string();
                    let ev = sequence(env, &mut head)?;
                    assert_eq!(receiver.ingest(ev.clone()), Ingest::Applied);
                    reply(
                        &mut socket,
                        ServerFrame::Accepted {
                            space: space.clone(),
                            client_id: cid,
                            seq: ev.seq,
                        },
                    )
                    .await;
                    reply(&mut socket, ServerFrame::Event { ev }).await;
                }
                ClientFrame::Req {
                    id,
                    op: Op::FetchLink { id: marker },
                } if marker == "upload-quiescent" => {
                    reply(
                        &mut socket,
                        ServerFrame::Res {
                            id,
                            result: Ok(Reply::Link(None)),
                        },
                    )
                    .await;
                    break;
                }
                ClientFrame::Req { id, .. } => {
                    reply(
                        &mut socket,
                        ServerFrame::Res {
                            id,
                            result: Ok(Reply::Done),
                        },
                    )
                    .await
                }
                ClientFrame::Ping => {
                    reply(
                        &mut socket,
                        ServerFrame::Pong {
                            server_time_ms: None,
                        },
                    )
                    .await
                }
                ClientFrame::Ephemeral { .. } => {}
                other => panic!("unexpected relay request: {other:?}"),
            }
        }
        Ok::<_, Box<dyn std::error::Error>>(())
    };
    let (_, produced) = tokio::join!(marker, wire);
    produced?;
    close_socket.await.unwrap();
    Ok(())
}
