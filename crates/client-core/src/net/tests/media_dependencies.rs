//! Production WebSocket scheduling regression for confirmed attachment prerequisites.
//! The relay fixture orders actual signed/MLS envelopes; no transport implementation is mocked.
use super::*;
use crate::{MediaDraftAssetInput, MediaDraftScope, PrivacyDto, SecretVault};

#[derive(Default)]
struct Vault(Mutex<HashMap<String, Vec<u8>>>);
impl SecretVault for Vault {
    fn load(&self, key: String) -> Option<Vec<u8>> {
        self.0.lock().unwrap().get(&key).cloned()
    }
    fn save(&self, key: String, value: Vec<u8>) -> bool {
        self.0.lock().unwrap().insert(key, value);
        true
    }
    fn delete(&self, key: String) {
        self.0.lock().unwrap().remove(&key);
    }
}
struct Directory(std::path::PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn wire_barrier(socket: &mut RelaySocket, commands: &mpsc::UnboundedSender<Cmd>, name: &str) {
    // Flush and request share the actual command FIFO. Every resulting Publish
    // would be on this socket before the marker; no sleep infers its absence.
    commands.send(Cmd::Flush).unwrap();
    let (answer, answered) = oneshot::channel();
    commands
        .send(Cmd::Req {
            op: Op::FetchLink { id: name.into() },
            reply: answer,
        })
        .unwrap();
    loop {
        match request(socket).await {
            ClientFrame::Req {
                id,
                op: Op::FetchLink { id: marker },
            } if marker == name => {
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
            ClientFrame::Ephemeral { .. } => {}
            other => panic!("unexpected publication before {name} barrier: {other:?}"),
        }
    }
    assert!(matches!(answered.await.unwrap(), Ok(Reply::Link(None))));
}

#[tokio::test(flavor = "current_thread")]
async fn media_dependencies_ws_retry_keeps_ciphertext_and_cold_receiver_album() {
    run_media_dependencies_ws(false).await;
}

#[tokio::test(flavor = "current_thread")]
async fn media_dependencies_ws_terminal_member_wakes_later_heads_without_another_confirmation() {
    run_media_dependencies_ws(true).await;
}

async fn run_media_dependencies_ws(terminal: bool) {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let directory = Directory(
        std::env::temp_dir().join(format!("zoen-media-wire-{}", Signer::generate().id())),
    );
    std::fs::create_dir(&directory.0).unwrap();
    let receiver_path = directory.0.join("receiver.sqlite");
    let mut receiver = Engine::open(receiver_path.to_str().unwrap()).unwrap();
    let (_, receiver_secret, _) = receiver.create_account("Reader", "reader", &url).unwrap();
    let reader_id = receiver.me.clone().unwrap();
    let sender = crate::Client::open(":memory:".into(), "en".into()).unwrap();
    let vault = Arc::new(Vault::default());
    sender
        .create_account("Sender".into(), "sender".into(), url.clone(), vault)
        .unwrap();
    let space = sender
        .create_group_with("Media wire".into(), vec![reader_id], PrivacyDto::EndToEnd)
        .unwrap();
    let author = sender.lock().net.author.clone().unwrap();
    let mut head = None;
    // Canonical membership precedes adding a real, independently persisted reader leaf.
    let setup_rows = sender.lock().store.outbox().unwrap();
    for row in setup_rows {
        let frame = sequence(Envelope::plain(&row.event), &mut head);
        assert_eq!(sender.lock().ingest(frame.clone()), Ingest::Confirmed);
        assert_eq!(receiver.ingest(frame), Ingest::Applied);
    }
    let packages = receiver.device().unwrap().key_packages(1, false).unwrap();
    let adding = sender
        .lock()
        .device()
        .unwrap()
        .commit(&space, &packages, &Default::default())
        .unwrap();
    let commit = sequence(
        Envelope::sealed(
            &author,
            &space,
            "wire-add-reader",
            1,
            head.as_ref(),
            Sealed::new(SealedKind::Commit, SUITE_ID, adding.commit),
        ),
        &mut head,
    );
    assert_eq!(sender.lock().ingest(commit.clone()), Ingest::Applied);
    assert_eq!(receiver.ingest(commit), Ingest::Applied);
    let welcome = sequence(
        Envelope::sealed(
            &author,
            &space,
            "wire-reader-welcome",
            2,
            head.as_ref(),
            Sealed::new(SealedKind::Welcome, SUITE_ID, adding.welcome.unwrap()),
        ),
        &mut head,
    );
    assert_eq!(sender.lock().ingest(welcome.clone()), Ingest::Applied);
    assert_eq!(receiver.ingest(welcome), Ingest::Applied);
    assert!(sender.lock().ready_epoch(&space).is_some());
    assert!(receiver.device().unwrap().has_group(&space));
    let account = sender.account().unwrap();
    let scope = MediaDraftScope {
        expected_identity_id: account.identity_id,
        expected_device_id: account.device_id,
        space_id: space.clone(),
        thread_root_id: None,
    };
    let content = serde_json::json!({"kind":"attachments", "options":{"presentation":"file", "grouping":"grouped"}, "items":[
        {"asset_id":"a", "caption":"First wire caption", "mutation_id":"wire-a"},
        {"asset_id":"b", "caption":"Second wire caption", "mutation_id":"wire-b"}
    ]}).to_string();
    let assets = ["a", "b"]
        .into_iter()
        .map(|id| MediaDraftAssetInput {
            id: id.into(),
            name: format!("{id}.pdf"),
            mime: "application/pdf".into(),
            bytes: format!("private {id}").into_bytes(),
        })
        .collect();
    let saved = sender
        .save_media_draft(scope.clone(), "0".into(), content, assets)
        .unwrap();
    let sent = sender
        .send_media_draft(scope, saved.draft.revision)
        .unwrap();
    let ids: Vec<_> = sent.entries.iter().map(|entry| entry.id.clone()).collect();
    // This fixture exercises WS ordering after the upload prerequisite succeeds;
    // media_uploads covers the actual HTTP worker and its terminal/transient paths.
    let uploaded = sender.lock().pending_uploads(8);
    for (blob, _) in uploaded {
        sender.lock().upload_done(&blob);
    }
    let later = terminal.then(|| {
        sender
            .send_message(space.clone(), "Later ordinary wire message".into())
            .unwrap()
            .id
    });
    let (expected, file_id) = {
        let mut engine = sender.lock();
        engine.set_registered(true).unwrap();
        let _ = engine.mls_key_packages_to_publish().unwrap();
        engine.mls_key_packages_published(Ok(()));
        assert!(engine.mls_seal_outbox().unwrap());
        let rows = engine.store.outbox().unwrap();
        let file_id = rows
            .iter()
            .find(|row| matches!(row.event.body, EventBody::ItemCreated { .. }))
            .unwrap()
            .event
            .client_id
            .clone();
        let mut ready = HashMap::new();
        let envelopes: HashMap<_, _> = rows
            .iter()
            .map(|row| {
                let env = engine
                    .outgoing_envelope_with(&row.event, &mut ready)
                    .unwrap();
                assert_eq!(env.sealed_kind(), Some(SealedKind::Application));
                assert!(env.plain_event().is_none());
                (row.event.client_id.clone(), env)
            })
            .collect();
        assert_eq!(envelopes.len(), if terminal { 5 } else { 4 });
        (envelopes, file_id)
    };
    let (commands_tx, mut commands_rx) = mpsc::unbounded_channel();
    let (status, _) = watch::channel(NetStatus {
        state: ConnState::Connecting,
        synced: false,
        error: None,
        registered: true,
        authenticated_relay: None,
    });
    let notices = Arc::new(RejectionListener::default());
    let ctx = Ctx {
        engine: EngineLease::new(sender.inner.clone()),
        listener: Some(notices.clone()),
        status,
        lang: crate::i18n::Lang::En,
        maintenance_work: None,
    };
    let (finished, close_socket) = oneshot::channel();
    let relay = async {
        let mut socket = catch_up_socket(&listener).await;
        reply(&mut socket, ServerFrame::SyncDone).await;
        let mut refused = HashSet::new();
        let mut confirmed = HashSet::new();
        let mut wire_members = Vec::new();
        let mut deferred = std::collections::VecDeque::new();
        let expected_confirmed = expected.len() - if terminal { 2 } else { 0 };
        while confirmed.len() < expected_confirmed {
            let frame = if let Some(env) = deferred.pop_front() {
                ClientFrame::Publish { env }
            } else {
                request(&mut socket).await
            };
            match frame {
                ClientFrame::Publish { env } => {
                    let cid = env.client_id().to_string();
                    assert_eq!(
                        &env,
                        expected.get(&cid).expect("unexpected fixture publication"),
                        "every retry must retain exact CID, signature and MLS ciphertext"
                    );
                    if (cid == file_id || cid == ids[0]) && refused.insert(cid.clone()) {
                        reply(
                            &mut socket,
                            ServerFrame::Rejected {
                                space: space.clone(),
                                client_id: cid.clone(),
                                reason: if terminal && cid == ids[0] {
                                    "terminal fixture refusal".into()
                                } else {
                                    "transient fixture refusal".into()
                                },
                                permanent: terminal && cid == ids[0],
                            },
                        )
                        .await;
                        if terminal && env.client_id() == ids[0] {
                            // Notification follows the production refusal handling.
                            // Withhold every later confirmation and every incidental
                            // request response until the ordered marker proves a wake.
                            notices.changed.notified().await;
                            let (answer, answered) = oneshot::channel();
                            commands_tx
                                .send(Cmd::Req {
                                    op: Op::FetchLink {
                                        id: "terminal-wake-only".into(),
                                    },
                                    reply: answer,
                                })
                                .unwrap();
                            let mut held_answers = Vec::new();
                            loop {
                                match request(&mut socket).await {
                                    ClientFrame::Publish { env } => {
                                        assert!(
                                            !ids.iter().any(|id| id == env.client_id()),
                                            "terminal group member must not be published"
                                        );
                                        deferred.push_back(env);
                                    }
                                    ClientFrame::Req {
                                        id,
                                        op: Op::FetchLink { id: marker },
                                    } if marker == "terminal-wake-only" => {
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
                                    ClientFrame::Req { id, .. } => held_answers.push(id),
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
                                    other => {
                                        panic!("unexpected during terminal wake barrier: {other:?}")
                                    }
                                }
                            }
                            assert!(matches!(answered.await.unwrap(), Ok(Reply::Link(None))));
                            assert!(deferred.iter().any(|env| Some(env.client_id()) == later.as_deref()), "terminal refusal must itself wake the later ordinary head before another confirmation or Flush");
                            for id in held_answers {
                                reply(
                                    &mut socket,
                                    ServerFrame::Res {
                                        id,
                                        result: Ok(Reply::Done),
                                    },
                                )
                                .await;
                            }
                        }
                        continue;
                    }
                    assert!(
                        confirmed.insert(cid.clone()),
                        "a confirmed publication must not be duplicated"
                    );
                    if cid == ids[1] {
                        assert!(
                            confirmed.contains(&ids[0]),
                            "group predecessor must be canonical first"
                        );
                    }
                    let ev = sequence(env, &mut head);
                    reply(
                        &mut socket,
                        ServerFrame::Accepted {
                            space: space.clone(),
                            client_id: cid.clone(),
                            seq: ev.seq,
                        },
                    )
                    .await;
                    if cid == file_id {
                        wire_barrier(&mut socket, &commands_tx, "accepted-file-is-not-confirmed")
                            .await;
                        let engine = sender.lock();
                        assert!(engine.store.outbox_get(&file_id).unwrap().is_some());
                        for id in &ids {
                            assert!(engine
                                .timeline(&space)
                                .unwrap()
                                .iter()
                                .any(|entry| &entry.id == id && entry.seq == 0));
                        }
                    }
                    assert_eq!(receiver.ingest(ev.clone()), Ingest::Applied);
                    if ids.contains(&cid) {
                        wire_members.push(cid);
                    }
                    reply(&mut socket, ServerFrame::Event { ev }).await;
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
                other => panic!("unexpected media relay request: {other:?}"),
            }
        }
        wire_barrier(&mut socket, &commands_tx, "all-media-confirmed").await;
        assert_eq!(refused, HashSet::from([file_id.clone(), ids[0].clone()]));
        if terminal {
            assert!(wire_members.is_empty());
            let rows = sender.lock().store.outbox().unwrap();
            assert_eq!(rows.len(), 2);
            assert!(rows
                .iter()
                .all(|row| row.failed && ids.contains(&row.event.client_id)));
            let timeline = sender.timeline(space.clone()).unwrap();
            for original in &sent.entries {
                let entry = timeline
                    .iter()
                    .find(|entry| entry.id == original.id)
                    .unwrap();
                assert_eq!(entry.delivery, crate::Delivery::Failed);
                assert_eq!(entry.kind, original.kind);
                assert_eq!(entry.media, original.media);
            }
            assert_eq!(
                timeline
                    .iter()
                    .find(|entry| Some(entry.id.as_str()) == later.as_deref())
                    .unwrap()
                    .delivery,
                crate::Delivery::Sent
            );
        } else {
            assert_eq!(wire_members, ids);
            assert!(sender.lock().store.outbox().unwrap().is_empty());
            assert!(sender
                .timeline(space.clone())
                .unwrap()
                .iter()
                .filter(|entry| ids.contains(&entry.id))
                .all(|entry| entry.delivery == crate::Delivery::Sent));
        }
        commands_tx.send(Cmd::Stop).unwrap();
        close_socket.await.unwrap();
    };
    let client = async {
        let mut backoff = Duration::from_millis(500);
        let exit = session(&ctx, &mut commands_rx, &mut backoff).await;
        finished.send(()).unwrap();
        exit
    };
    let (exit, ()) = tokio::time::timeout(Duration::from_secs(20), async {
        tokio::join!(client, relay)
    })
    .await
    .expect("actual media WebSocket retry did not converge");
    assert!(matches!(exit, Exit::Stop));
    ctx.engine.revoke();
    drop(ctx);
    drop(sender);
    receiver.logs[&space].verify().unwrap();
    let expected_events = receiver.store.event_count().unwrap();
    drop(receiver);
    let mut receiver = Engine::open(receiver_path.to_str().unwrap()).unwrap();
    assert!(receiver.unlock(Some(receiver_secret.to_vec())).unwrap());
    assert_eq!(receiver.store.event_count().unwrap(), expected_events);
    if terminal {
        let timeline = receiver.timeline(&space).unwrap();
        assert!(
            timeline.iter().all(|entry| !ids.contains(&entry.id)),
            "refused group cannot gain phantom remote delivery"
        );
        assert_eq!(
            timeline
                .iter()
                .filter(|entry| Some(entry.id.as_str()) == later.as_deref())
                .count(),
            1
        );
        return;
    }
    let messages: Vec<_> = receiver
        .timeline(&space)
        .unwrap()
        .into_iter()
        .filter(|entry| ids.contains(&entry.id))
        .collect();
    assert_eq!(
        messages
            .iter()
            .map(|entry| entry.id.clone())
            .collect::<Vec<_>>(),
        ids
    );
    assert!(messages
        .iter()
        .all(|entry| entry.delivery == crate::Delivery::Sent));
    assert!(
        matches!(&messages[0].kind, crate::EntryKind::Message { text, .. } if text == "First wire caption")
    );
    assert!(
        matches!(&messages[1].kind, crate::EntryKind::Message { text, .. } if text == "Second wire caption")
    );
    assert_eq!(
        messages[0]
            .media
            .as_ref()
            .unwrap()
            .group
            .as_ref()
            .unwrap()
            .index,
        0
    );
    assert_eq!(
        messages[1]
            .media
            .as_ref()
            .unwrap()
            .group
            .as_ref()
            .unwrap()
            .index,
        1
    );
}
