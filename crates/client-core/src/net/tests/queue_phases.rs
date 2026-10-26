//! Phases of the same continuous queued-message fixture and relay socket.

use super::*;

pub(super) async fn collision(
    socket: &mut RelaySocket,
    catch_up: &CatchUp,
    engine: &Shared,
    notices: &RejectionListener,
    space: &str,
    losing_commit: &Option<String>,
    missed_commit: &Sequenced,
) -> Option<Sequenced> {
    let mut held_commit = None;

    reply(socket, ServerFrame::SyncDone).await;
    loop {
        match request(socket).await {
            ClientFrame::Publish { env } if Some(env.client_id()) == losing_commit.as_deref() => {
                reply(
                    socket,
                    ServerFrame::Rejected {
                        space: space.into(),
                        client_id: env.client_id().into(),
                        reason: protocol::STALE_COMMIT.into(),
                        permanent: true,
                    },
                )
                .await;
                break;
            }
            ClientFrame::Req { id, .. } => {
                reply(
                    socket,
                    ServerFrame::Res {
                        id,
                        result: Ok(Reply::Done),
                    },
                )
                .await
            }
            ClientFrame::Ping => {
                reply(
                    socket,
                    ServerFrame::Pong {
                        server_time_ms: None,
                    },
                )
                .await
            }
            other => panic!("unexpected before commit collision {other:?}"),
        }
    }
    if matches!(catch_up, CatchUp::StorageRefusal) {
        notices.changed.notified().await;
        {
            let engine = lock(engine);
            assert!(engine.device().unwrap().pending(space));
            assert!(engine
                .store
                .outbox_get(losing_commit.as_ref().unwrap())
                .unwrap()
                .is_some());
            engine
                .store
                .conn()
                .execute_batch("DROP TRIGGER fail_refusal")
                .unwrap();
        }
        reply(
            socket,
            ServerFrame::Rejected {
                space: space.into(),
                client_id: losing_commit.clone().unwrap(),
                reason: protocol::STALE_COMMIT.into(),
                permanent: true,
            },
        )
        .await;
    }
    if matches!(catch_up, CatchUp::StorageRefusal) {
        // Exercise the real gap after retry succeeds but before the winning
        // commit arrives: the relay must refuse an old-epoch application.
        held_commit = Some(missed_commit.clone());
    } else {
        reply(
            socket,
            ServerFrame::Event {
                ev: missed_commit.clone(),
            },
        )
        .await;
    }
    held_commit
}

pub(super) struct QueuedPair<'a> {
    pub(super) space: &'a str,
    pub(super) cached: &'a types::Event,
    pub(super) fresh: &'a types::Event,
    pub(super) cached_key: &'a String,
    pub(super) fresh_key: &'a String,
}

pub(super) async fn incremental(
    socket: &mut RelaySocket,
    engine: &Shared,
    ctx: &Ctx,
    pair: QueuedPair<'_>,
    missed_commit: &Sequenced,
    maintenance_started: &mut mpsc::UnboundedReceiver<()>,
    current_copies: &mut HashMap<String, Envelope>,
) {
    let QueuedPair {
        space,
        cached,
        fresh,
        cached_key,
        fresh_key,
    } = pair;

    reply(socket, ServerFrame::SyncDone).await;
    let mut sent_at_old_epoch = HashSet::new();
    while sent_at_old_epoch.is_empty() {
        match request(socket).await {
            ClientFrame::Publish { env } if env.sealed_kind() == Some(SealedKind::Application) => {
                assert!(ctx.status.borrow().synced);
                let (_, bytes) = env.sealed_data().unwrap();
                assert_eq!(::mls::application_epoch(bytes), Some(1));
                assert!(sent_at_old_epoch.insert(env.client_id().to_string()));
            }
            ClientFrame::Req { id, .. } => {
                reply(
                    socket,
                    ServerFrame::Res {
                        id,
                        result: Ok(Reply::Done),
                    },
                )
                .await;
            }
            ClientFrame::Ping => {
                reply(
                    socket,
                    ServerFrame::Pong {
                        server_time_ms: None,
                    },
                )
                .await
            }
            ClientFrame::Publish { .. } => {}
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(sent_at_old_epoch.contains(&cached.client_id));
    assert!(!sent_at_old_epoch.contains(&fresh.client_id));
    // Live catch-up overtakes the first epoch-1 publication. The newer
    // message remains queued behind it and is resealed before its first offer.
    reply(
        socket,
        ServerFrame::Event {
            ev: missed_commit.clone(),
        },
    )
    .await;
    reply(socket, ServerFrame::SyncDone).await;
    loop {
        maintenance_started.recv().await.unwrap();
        let engine = lock(engine);
        if [cached_key, fresh_key].iter().all(|key| {
            engine
                .store
                .meta(key)
                .unwrap()
                .is_some_and(|v| v.starts_with("2:"))
        }) {
            assert_eq!(engine.mls_status(space).unwrap().0, 2);
            break;
        }
    }
    *current_copies = lock(engine)
        .outbox_envelopes()
        .into_iter()
        .filter(|env| env.sealed_kind() == Some(SealedKind::Application))
        .map(|env| (env.client_id().to_string(), env))
        .collect();
    assert_eq!(current_copies.len(), 2);
    // Both caches have new ciphertext, but only the first envelope was
    // actually written at epoch1. Its late answer cannot clear the newer copy.
    reply(
        socket,
        ServerFrame::Rejected {
            space: space.into(),
            client_id: cached.client_id.clone(),
            reason: protocol::STALE_SEAL.into(),
            permanent: true,
        },
    )
    .await;
}

pub(super) async fn initial(
    socket: &mut RelaySocket,
    engine: &Shared,
    cmd: &mpsc::UnboundedSender<Cmd>,
    space: &str,
    keys: (&String, &String, String),
    missed_commit: Sequenced,
    maintenance_started: &mut mpsc::UnboundedReceiver<()>,
) {
    let (cached_key, fresh_key, cached_copy) = keys;

    // Wait for a real maintenance turn before the FIFO request marker.
    // Its arrival proves the initial flush and explicit Flush have completed;
    // no sleep is used to infer the absence of a publication.
    maintenance_started.recv().await.unwrap();
    cmd.send(Cmd::Flush).unwrap();
    let (marker, marked) = oneshot::channel();
    let marker_box = "11".repeat(32);
    cmd.send(Cmd::Req {
        op: Op::FetchLink {
            id: marker_box.clone(),
        },
        reply: marker,
    })
    .unwrap();
    loop {
        match request(socket).await {
            ClientFrame::Publish { .. } => {
                panic!("an offline write overtook the initial catch-up")
            }
            ClientFrame::Req {
                id,
                op: Op::FetchLink { id: link },
            } if link == marker_box => {
                reply(
                    socket,
                    ServerFrame::Res {
                        id,
                        result: Ok(Reply::Link(None)),
                    },
                )
                .await;
                break;
            }
            ClientFrame::Ping => {
                reply(
                    socket,
                    ServerFrame::Pong {
                        server_time_ms: None,
                    },
                )
                .await
            }
            other => panic!("maintenance sent {other:?} before catch-up"),
        }
    }
    assert!(matches!(marked.await.unwrap(), Ok(Reply::Link(None))));
    {
        let engine = lock(engine);
        assert_eq!(engine.outbox_len(), 2);
        assert_eq!(engine.store.meta(cached_key).unwrap(), Some(cached_copy));
        assert!(engine.store.meta(fresh_key).unwrap().is_none());
        assert!(engine.store.outbox_handshakes().unwrap().is_empty());
        assert_eq!(engine.mls_status(space).unwrap().0, 1);
    }
    reply(socket, ServerFrame::Event { ev: missed_commit }).await;
    reply(socket, ServerFrame::SyncDone).await;
}
