//! Original malformed-wire/authentication rejection followed by valid real-socket ingest.
use super::*;
use types::IdentityKind;

const SPACE: &str = "sp_incoming_pipeline";
const AT: i64 = 1_700_000_000_000;
const FIRST: &str = "after-invalid-one";
const SECOND: &str = "after-invalid-two";
const FIRST_TEXT: &str = "Mensagem válida após entrada inválida 🌄";
const SECOND_TEXT: &str = "A conversa continua, sem duplicar.";

struct Directory(std::path::PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn history(engine: &Engine, peer: &str) -> Vec<crate::TimelineEntry> {
    let entries = engine.timeline(SPACE).unwrap();
    assert_eq!(
        entries.len(),
        2,
        "only the two admissible messages are readable"
    );
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>(),
        [FIRST, SECOND]
    );
    assert_eq!(
        entries.iter().map(|entry| entry.seq).collect::<Vec<_>>(),
        [2, 3]
    );
    assert_eq!(
        entries.iter().map(|entry| entry.at_ms).collect::<Vec<_>>(),
        [AT + 2, AT + 3]
    );
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.author.id.as_str())
            .collect::<Vec<_>>(),
        [peer, peer]
    );
    assert_eq!(
        entries
            .iter()
            .map(|entry| match &entry.kind {
                crate::EntryKind::Message { text, card: None } => text.as_str(),
                other => panic!("unexpected readable content: {other:?}"),
            })
            .collect::<Vec<_>>(),
        [FIRST_TEXT, SECOND_TEXT]
    );
    let retained = engine.store.events(SPACE).unwrap();
    assert_eq!(retained.len(), 4);
    assert_eq!(
        retained
            .iter()
            .map(|event| event.client_id.as_str())
            .collect::<Vec<_>>(),
        ["pipeline-create", "pipeline-member", FIRST, SECOND]
    );
    assert_eq!(engine.next_seq(SPACE), 4);
    assert!(engine.verify_log(SPACE).valid);
    entries
}

// A response follows all prior inbound frames; the next request follows that response.
// Together the barriers fence both directions without sleeps or an extra chat message.
async fn barrier(
    command: &mpsc::UnboundedSender<Cmd>,
    socket: &mut RelaySocket,
    id: &str,
    delivered: &mut Vec<u64>,
) {
    let (reply_to, answer) = oneshot::channel();
    command
        .send(Cmd::Req {
            op: Op::FetchLink { id: id.into() },
            reply: reply_to,
        })
        .unwrap();
    loop {
        match request(socket).await {
            ClientFrame::Req {
                id: request_id,
                op: Op::FetchLink { id: link },
            } if link == id => {
                reply(
                    socket,
                    ServerFrame::Res {
                        id: request_id,
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
            ClientFrame::Ephemeral {
                space,
                kind: EphemeralKind::Delivered { seq },
            } => {
                assert_eq!(space, SPACE);
                delivered.push(seq);
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
            other => panic!("unexpected pipeline traffic before {id}: {other:?}"),
        }
    }
    assert!(
        matches!(
            answer.await.expect("session ended before ordered response"),
            Ok(Reply::Link(None))
        ),
        "subsequent valid wire work must complete"
    );
}

// Observe the real maintenance pass, then fence its socket writes. A rejected
// event must not become a delayed false receipt merely because batching runs later.
async fn maintenance_barrier(
    passes: &mut watch::Receiver<usize>,
    command: &mpsc::UnboundedSender<Cmd>,
    socket: &mut RelaySocket,
    id: &str,
    delivered: &mut Vec<u64>,
) {
    drop(passes.borrow_and_update());
    tokio::time::timeout(Duration::from_secs(3), passes.changed())
        .await
        .expect("the real receipt maintenance pass did not run")
        .expect("maintenance observer was retired early");
    barrier(command, socket, id, delivered).await;
}

#[tokio::test(flavor = "current_thread")]
async fn malformed_wire_and_rejected_signature_do_not_poison_following_messages_or_readable_receipts(
) {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let directory = Directory(std::env::temp_dir().join(types::new_id("incoming-pipeline")));
    std::fs::create_dir(&directory.0).unwrap();
    let path = directory.0.join("reader.sqlite");
    let root = Signer::from_secret(&[83; 32]);
    let device = Signer::from_secret(&[84; 32]);
    let peer = Author::root(Signer::from_secret(&[85; 32]));
    let mut engine = Engine::open(path.to_str().unwrap()).unwrap();
    for (id, name) in [(root.id(), "Owner"), (peer.identity.clone(), "Peer")] {
        engine
            .store
            .put_identity(
                &Identity {
                    id,
                    kind: IdentityKind::Person,
                    name: name.into(),
                    handle: name.to_lowercase(),
                    tint_hex: "#246844".into(),
                    glyph: None,
                    owner: None,
                    owner_proof: None,
                    bio: "Synthetic incoming continuity fixture".into(),
                },
                None,
            )
            .unwrap();
    }
    engine
        .install_restored_account(&root, device.clone(), &url)
        .unwrap();
    let _ = engine.mls_key_packages_to_publish().unwrap();
    engine.mls_key_packages_published(Ok(()));
    let owner = engine.net.author.clone().unwrap();
    let mut head = None;
    for (id, body) in [
        (
            "pipeline-create",
            EventBody::SpaceCreated {
                title: "Incoming continuity".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::Closed,
            },
        ),
        (
            "pipeline-member",
            EventBody::MemberAdded {
                identity: peer.identity.clone(),
                role: Role::Member,
            },
        ),
    ] {
        let frame = sequence(
            Envelope::plain(&owner.sign_event(SPACE, id, AT, head.clone(), body)),
            &mut head,
        );
        assert_eq!(engine.ingest(frame), Ingest::Applied);
    }
    let mut valid = Vec::new();
    for (id, text, offset) in [(FIRST, FIRST_TEXT, 2), (SECOND, SECOND_TEXT, 3)] {
        valid.push(sequence(
            Envelope::plain(&peer.sign_event(
                SPACE,
                id,
                AT + offset,
                head.clone(),
                EventBody::MessagePosted {
                    message: format!("body-{id}"),
                    text: text.into(),
                    attaches: None,
                    reply: None,
                    forwarded: false,
                },
            )),
            &mut head,
        ));
    }
    let mut rejected = valid[0].clone();
    let replacement = if rejected.env.sig.starts_with('0') {
        "1"
    } else {
        "0"
    };
    rejected.env.sig.replace_range(..1, replacement);
    assert_ne!(rejected.env.sig, valid[0].env.sig);
    assert_eq!(
        rejected.hash, valid[0].hash,
        "corruption must exercise the same signed-content ID and sequence, not another row"
    );
    let malformed = vec![0xff];
    assert!(
        ServerFrame::decode(&malformed).is_err(),
        "the wire corruption must reach the actual decoder error path"
    );
    let engine = Arc::new(Mutex::new(engine));
    let (cmd, mut commands) = mpsc::unbounded_channel();
    let (status, _) = watch::channel(NetStatus {
        state: ConnState::Connecting,
        synced: false,
        error: None,
        registered: true,
        authenticated_relay: None,
    });
    let (maintenance_pass, mut maintenance_done) = watch::channel(0usize);
    let ctx = Ctx {
        engine: EngineLease::new(engine.clone()),
        listener: None,
        status,
        lang: crate::i18n::Lang::En,
        maintenance_work: Some(Arc::new(move || {
            maintenance_pass.send_modify(|passes| *passes += 1)
        })),
    };
    let (finished, closed) = oneshot::channel();
    let server = async {
        let mut socket = catch_up_socket(&listener).await;
        socket
            .send(Message::Binary(malformed.into()))
            .await
            .unwrap();
        reply(&mut socket, ServerFrame::Event { ev: rejected }).await;
        reply(&mut socket, ServerFrame::SyncDone).await;
        let mut delivered = Vec::new();
        barrier(&cmd, &mut socket, "rejected-receive", &mut delivered).await;
        maintenance_barrier(
            &mut maintenance_done,
            &cmd,
            &mut socket,
            "rejected-wire-fence",
            &mut delivered,
        )
        .await;
        {
            let engine = lock(&engine);
            assert!(
                ctx.status.borrow().synced,
                "wire control flow survives malformed input"
            );
            assert!(engine.timeline(SPACE).unwrap().is_empty());
            assert_eq!(engine.store.events(SPACE).unwrap().len(), 2);
            assert_eq!(engine.next_seq(SPACE), 2);
            assert_eq!(engine.delivered_to_announce(SPACE), None);
            assert_eq!(engine.read_up_to(SPACE), None);
            assert!(engine.verify_log(SPACE).valid);
        }
        assert!(
            delivered.is_empty(),
            "rejected content cannot publish a readable delivery watermark"
        );
        reply(
            &mut socket,
            ServerFrame::Event {
                ev: valid[0].clone(),
            },
        )
        .await;
        reply(
            &mut socket,
            ServerFrame::Event {
                ev: valid[0].clone(),
            },
        )
        .await;
        reply(
            &mut socket,
            ServerFrame::Event {
                ev: valid[1].clone(),
            },
        )
        .await;
        barrier(&cmd, &mut socket, "valid-receive", &mut delivered).await;
        maintenance_barrier(
            &mut maintenance_done,
            &cmd,
            &mut socket,
            "valid-wire-fence",
            &mut delivered,
        )
        .await;
        let expected = history(&lock(&engine), &peer.identity);
        assert_eq!(
            delivered.last(),
            Some(&3),
            "readable delivery reaches the complete valid prefix"
        );
        assert!(delivered.iter().all(|seq| (2..=3).contains(seq)));
        assert!(
            delivered.windows(2).all(|pair| pair[0] < pair[1]),
            "duplicate replay cannot repeat or rewind a delivered watermark"
        );
        cmd.send(Cmd::Stop).unwrap();
        closed.await.unwrap();
        expected
    };
    let mut backoff = Duration::from_millis(500);
    let client = async {
        let exit = session(&ctx, &mut commands, &mut backoff).await;
        finished.send(()).unwrap();
        exit
    };
    let (exit, expected) = tokio::time::timeout(Duration::from_secs(15), async {
        tokio::join!(client, server)
    })
    .await
    .expect("incoming pipeline did not reach both ordered barriers");
    assert!(matches!(exit, Exit::Stop));
    ctx.engine.revoke();
    assert_eq!(Arc::strong_count(&engine), 1);
    drop(ctx);
    drop(engine);
    let mut reopened = Engine::open(path.to_str().unwrap()).unwrap();
    assert!(reopened.unlock(Some(device.secret().to_vec())).unwrap());
    assert_eq!(
        history(&reopened, &peer.identity),
        expected,
        "the complete readable projection survives cold reopen"
    );
    assert_eq!(reopened.ingest(valid[0].clone()), Ingest::Duplicate);
    assert_eq!(reopened.ingest(valid[1].clone()), Ingest::Duplicate);
    assert_eq!(history(&reopened, &peer.identity), expected);
    assert_eq!(reopened.delivered_to_announce(SPACE), Some(3));
    assert_eq!(reopened.read_up_to(SPACE), None);
}
