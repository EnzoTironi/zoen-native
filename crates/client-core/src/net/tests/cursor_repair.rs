//! Original final-lost-wake regressions. No Signal code or fixture is copied.
use super::*;
use types::IdentityKind;

const SPACE: &str = "sp_final_unannounced";
const EVENT: &str = "last-unannounced-event";
const TEXT: &str = "O último evento também precisa chegar.";
const AT: i64 = 1_700_000_000_000;

fn policy() -> CursorRepairPolicy {
    // Short fixture policy, not a measurement of DEFAULT's 60s/30s budgets.
    CursorRepairPolicy {
        interval: Duration::from_millis(250),
        // The missing-Sync oracle expires before the watchdog fallback so its
        // negative control cannot be disguised as a transport-close failure.
        timeout: Duration::from_secs(5),
        initial_inactivity: Duration::from_secs(5),
    }
}

#[test]
fn ordered_partial_completions_cannot_complete_or_postpone_a_full_repair() {
    let start = tokio::time::Instant::now();
    let policy = policy();
    let mut repair = CursorRepair::new(policy, start);
    assert_eq!(
        repair.admission(),
        None,
        "initial catch-up has no polling timer"
    );
    assert!(repair.submitted(), "an overlapping initial Joined request");
    repair.completed(start);
    assert_eq!(repair.admission(), Some(start + policy.interval));
    assert_eq!(
        repair.abort_deadline(),
        Some(start + policy.interval + policy.timeout)
    );
    assert!(!repair.begin(start), "repair admission cannot run early");
    assert!(repair.begin(start + policy.interval));
    assert!(
        !repair.begin(start + policy.interval),
        "one full repair only"
    );
    assert!(repair.submitted(), "a gap queued AFTER the full repair");
    let deadline = start + policy.interval + policy.timeout;
    assert_eq!(repair.abort_deadline(), Some(deadline));
    repair.completed(start + policy.interval + Duration::from_millis(1));
    assert!(
        repair.in_flight(),
        "the older Joined completion is not the repair"
    );
    assert_eq!(repair.abort_deadline(), Some(deadline));
    let completed = start + policy.interval + Duration::from_millis(2);
    repair.completed(completed);
    assert!(!repair.in_flight());
    assert_eq!(repair.admission(), Some(completed + policy.interval));
    repair.completed(completed + Duration::from_millis(1));
    repair.completed(completed + Duration::from_millis(2));
    assert_eq!(
        repair.admission(),
        Some(completed + policy.interval),
        "later partial and unsolicited completions do not postpone reconciliation"
    );
    let fresh_socket = CursorRepair::new(policy, start);
    assert_eq!(fresh_socket.admission(), None);
    assert_eq!(
        fresh_socket.abort_deadline(),
        Some(start + policy.initial_inactivity)
    );
    assert!(
        !fresh_socket.in_flight(),
        "no repair admission survives reconnect"
    );
}

struct Directory(std::path::PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn assert_history(engine: &Engine, peer: &str) -> Vec<crate::TimelineEntry> {
    let rows = engine.store.events(SPACE).unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows.iter()
            .map(|row| row.client_id.as_str())
            .collect::<Vec<_>>(),
        ["repair-create", "repair-member", EVENT]
    );
    assert_eq!(
        rows.iter().map(|row| row.seq).collect::<Vec<_>>(),
        [0, 1, 2]
    );
    let entries = engine.timeline(SPACE).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, EVENT);
    assert_eq!(entries[0].seq, 2);
    assert_eq!(entries[0].at_ms, AT + 2);
    assert_eq!(entries[0].author.id, peer);
    assert!(
        matches!(&entries[0].kind, crate::EntryKind::Message { text, card: None } if text == TEXT)
    );
    assert_eq!(engine.next_seq(SPACE), 3);
    assert!(engine.verify_log(SPACE).valid);
    entries
}

async fn ordinary(socket: &mut RelaySocket, frame: ClientFrame, marks: &mut Vec<u64>) {
    match frame {
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
        ClientFrame::Ephemeral {
            space,
            kind: EphemeralKind::Delivered { seq },
        } => {
            assert_eq!(space, SPACE);
            marks.push(seq);
        }
        ClientFrame::Ephemeral { .. } => {}
        other => panic!("unexpected repair fixture traffic: {other:?}"),
    }
}

// The response is ordered behind prior inbound frames and adds no user content.
async fn barrier(
    command: &mpsc::UnboundedSender<Cmd>,
    socket: &mut RelaySocket,
    name: &str,
    marks: &mut Vec<u64>,
) {
    let (answer_to, answer) = oneshot::channel();
    command
        .send(Cmd::Req {
            op: Op::FetchLink { id: name.into() },
            reply: answer_to,
        })
        .unwrap();
    loop {
        let frame = request(socket).await;
        if let ClientFrame::Req {
            id,
            op: Op::FetchLink { id: link },
        } = &frame
        {
            if link == name {
                reply(
                    socket,
                    ServerFrame::Res {
                        id: *id,
                        result: Ok(Reply::Link(None)),
                    },
                )
                .await;
                break;
            }
        }
        if let ClientFrame::Sync { cursors, all } = &frame {
            assert_ne!(
                name, "rejected-repair-content",
                "the active full repair cannot admit a second repair"
            );
            assert!(*all);
            assert_eq!(cursors.len(), 1);
            assert_eq!(cursors[0].space, SPACE);
            assert_eq!(
                cursors[0].next_seq,
                if name == "initial-complete" { 2 } else { 3 }
            );
            // A slow scheduler can legitimately reach a later interval while an
            // unrelated wire barrier is pending. Serve its current empty tail.
            reply(socket, ServerFrame::SyncDone).await;
            continue;
        }
        ordinary(socket, frame, marks).await;
    }
    assert!(matches!(answer.await.unwrap(), Ok(Reply::Link(None))));
}

#[tokio::test(flavor = "current_thread")]
async fn healthy_socket_recovers_final_unannounced_event_without_reconnect_or_followup() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let directory = Directory(std::env::temp_dir().join(types::new_id("final-unannounced")));
    std::fs::create_dir(&directory.0).unwrap();
    let path = directory.0.join("reader.sqlite");
    let canonical_path = directory.0.join("canonical.sqlite");
    let root = Signer::from_secret(&[121; 32]);
    let device = Signer::from_secret(&[122; 32]);
    let peer = Author::root(Signer::from_secret(&[123; 32]));
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
                    bio: "Synthetic final-wake fixture".into(),
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
    let canonical = store::Store::open(canonical_path.to_str().unwrap()).unwrap();
    let mut head = None;
    for (id, body) in [
        (
            "repair-create",
            EventBody::SpaceCreated {
                title: "Final wake".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::Closed,
            },
        ),
        (
            "repair-member",
            EventBody::MemberAdded {
                identity: peer.identity.clone(),
                role: Role::Member,
            },
        ),
    ] {
        let ev = sequence(
            Envelope::plain(&owner.sign_event(SPACE, id, AT, head.clone(), body)),
            &mut head,
        );
        canonical.append_event(&ev.plain_event().unwrap()).unwrap();
        assert_eq!(engine.ingest(ev), Ingest::Applied);
    }
    let missing = sequence(
        Envelope::plain(&peer.sign_event(
            SPACE,
            EVENT,
            AT + 2,
            head.clone(),
            EventBody::MessagePosted {
                message: "body-final-unannounced".into(),
                text: TEXT.into(),
                attaches: None,
                reply: None,
                media: None,
                forwarded: false,
            },
        )),
        &mut head,
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
    let ctx = Ctx {
        engine: EngineLease::new(engine.clone()),
        listener: None,
        status,
        lang: crate::i18n::Lang::En,
        maintenance_work: None,
    };
    let (finished, closed) = oneshot::channel();
    let server = async {
        let mut socket = catch_up_socket(&listener).await;
        reply(&mut socket, ServerFrame::SyncDone).await;
        let mut marks = Vec::new();
        barrier(&cmd, &mut socket, "initial-complete", &mut marks).await;
        assert!(ctx.status.borrow().synced);
        assert_eq!(lock(&engine).next_seq(SPACE), 2);
        // Persist ONLY after initial catch-up has completed; drop the sole live frame.
        canonical
            .append_event(&missing.plain_event().unwrap())
            .unwrap();
        drop(canonical);
        let canonical = store::Store::open(canonical_path.to_str().unwrap()).unwrap();
        assert_eq!(canonical.events(SPACE).unwrap().len(), 3);
        // No Event, Joined, later content or explicit refresh is sent. Only
        // transport liveness continues while the durable row is unannounced.
        let mut heartbeats = tokio::time::interval(Duration::from_millis(20));
        let repair = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                tokio::select! {
                    frame = request(&mut socket) => match frame {
                        ClientFrame::Sync { cursors, all } => break (cursors, all),
                        other => ordinary(&mut socket, other, &mut marks).await,
                    },
                    _ = heartbeats.tick() => reply(&mut socket, ServerFrame::Pong { server_time_ms: None }).await,
                }
            }
        }).await.expect("healthy Pongs must not hide a completely lost final live frame");
        assert!(
            repair.1,
            "repair includes current Spaces even if Joined itself was lost"
        );
        assert_eq!(repair.0.len(), 1);
        assert_eq!(repair.0[0].space, SPACE);
        assert_eq!(repair.0[0].next_seq, 2);
        assert!(marks.iter().all(|seq| *seq <= 1));
        let mut forged = missing.clone();
        let replacement = if forged.env.sig.starts_with('0') {
            "1"
        } else {
            "0"
        };
        forged.env.sig.replace_range(..1, replacement);
        reply(&mut socket, ServerFrame::Event { ev: forged }).await;
        barrier(&cmd, &mut socket, "rejected-repair-content", &mut marks).await;
        assert_eq!(
            lock(&engine).next_seq(SPACE),
            2,
            "repair never bypasses signature admission"
        );
        assert_eq!(lock(&engine).store.events(SPACE).unwrap().len(), 2);
        assert!(marks.iter().all(|seq| *seq <= 1));
        // Serve the actual retained row twice: overlap cannot duplicate content.
        let row = canonical.events(SPACE).unwrap().pop().unwrap();
        let recovered = Sequenced {
            seq: row.seq,
            prev: row.prev.clone(),
            hash: row.hash.clone(),
            env: Envelope::plain(&row),
        };
        reply(
            &mut socket,
            ServerFrame::Event {
                ev: recovered.clone(),
            },
        )
        .await;
        reply(&mut socket, ServerFrame::Event { ev: recovered }).await;
        reply(&mut socket, ServerFrame::SyncDone).await;
        barrier(&cmd, &mut socket, "repaired-content", &mut marks).await;
        let expected = assert_history(&lock(&engine), &peer.identity);
        tokio::time::timeout(Duration::from_secs(2), async {
            while !marks.contains(&2) {
                let frame = request(&mut socket).await;
                match frame {
                    ClientFrame::Sync { cursors, all } => {
                        assert!(all);
                        assert_eq!(cursors.len(), 1);
                        assert_eq!(cursors[0].space, SPACE);
                        assert_eq!(cursors[0].next_seq, 3);
                        reply(&mut socket, ServerFrame::SyncDone).await;
                    }
                    other => ordinary(&mut socket, other, &mut marks).await,
                }
            }
        })
        .await
        .expect("processed readable content must eventually announce its durable cursor");
        assert_eq!(marks.iter().copied().max(), Some(2));
        cmd.send(Cmd::Stop).unwrap();
        closed.await.unwrap();
        // The client used this one socket: no reconnect can conceal the missing wake.
        assert!(futures_util::FutureExt::now_or_never(listener.accept()).is_none());
        expected
    };
    let mut backoff = Duration::from_millis(500);
    let client = async {
        let exit = session_with_repair_policy(&ctx, &mut commands, &mut backoff, policy()).await;
        finished.send(()).unwrap();
        exit
    };
    let (exit, expected) = tokio::time::timeout(Duration::from_secs(12), async {
        tokio::join!(client, server)
    })
    .await
    .expect("final lost-wake recovery stalled");
    assert!(matches!(exit, Exit::Stop));
    ctx.engine.revoke();
    assert_eq!(Arc::strong_count(&engine), 1);
    drop(ctx);
    drop(engine);
    let mut reopened = Engine::open(path.to_str().unwrap()).unwrap();
    assert!(reopened.unlock(Some(device.secret().to_vec())).unwrap());
    assert_eq!(assert_history(&reopened, &peer.identity), expected);
    assert_eq!(reopened.ingest(missing), Ingest::Duplicate);
    assert_eq!(assert_history(&reopened, &peer.identity), expected);
}

#[derive(Clone, Copy)]
enum EndRepair {
    Stop,
    Unavailable,
    Deadline,
}

#[tokio::test(flavor = "current_thread")]
async fn pending_repair_is_session_owned_and_pongs_cannot_mask_failure_or_stop() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    for mode in [EndRepair::Stop, EndRepair::Unavailable, EndRepair::Deadline] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let mut engine = Engine::open(":memory:").unwrap();
        engine.create_account("Owner", "owner", &url).unwrap();
        engine.set_registered(true).unwrap();
        let _ = engine.mls_key_packages_to_publish().unwrap();
        engine.mls_key_packages_published(Ok(()));
        let engine = Arc::new(Mutex::new(engine));
        let (cmd, mut commands) = mpsc::unbounded_channel();
        let (status, _) = watch::channel(NetStatus {
            state: ConnState::Connecting,
            synced: false,
            error: None,
            registered: true,
            authenticated_relay: None,
        });
        let ctx = Ctx {
            engine: EngineLease::new(engine.clone()),
            listener: None,
            status,
            lang: crate::i18n::Lang::En,
            maintenance_work: None,
        };
        let (finished, closed) = oneshot::channel();
        let server = async {
            let mut socket = catch_up_socket(&listener).await;
            reply(&mut socket, ServerFrame::SyncDone).await;
            let mut marks = Vec::new();
            loop {
                match request(&mut socket).await {
                    ClientFrame::Sync { cursors, all } => {
                        assert!(all);
                        assert!(cursors.is_empty());
                        break;
                    }
                    other => ordinary(&mut socket, other, &mut marks).await,
                }
            }
            match mode {
                EndRepair::Stop => cmd.send(Cmd::Stop).unwrap(),
                EndRepair::Unavailable => {
                    reply(
                        &mut socket,
                        ServerFrame::error(ErrorCode::Unavailable, "repair store unavailable"),
                    )
                    .await
                }
                EndRepair::Deadline => {}
            }
            let mut closed = Box::pin(closed);
            let mut heartbeat = tokio::time::interval(Duration::from_millis(20));
            loop {
                tokio::select! {
                    done = &mut closed => { done.unwrap(); break; }
                    _ = heartbeat.tick() => {
                        // No SyncDone: transport liveness cannot extend the repair budget.
                        let _ = socket.send(Message::Binary(ServerFrame::Pong { server_time_ms: None }.encode().into())).await;
                    }
                    frame = socket.next() => match frame {
                        Some(Ok(Message::Binary(bytes))) => {
                            let frame = ClientFrame::decode(&bytes).unwrap();
                            assert!(!matches!(frame, ClientFrame::Sync { .. }), "one repair in flight only");
                        }
                        Some(Ok(_)) => {}
                        Some(Err(_)) | None => { closed.await.unwrap(); break; }
                    }
                }
            }
        };
        let mut backoff = Duration::from_millis(500);
        let client = async {
            let exit = session_with_repair_policy(
                &ctx,
                &mut commands,
                &mut backoff,
                CursorRepairPolicy {
                    interval: Duration::from_millis(40),
                    timeout: Duration::from_millis(120),
                    initial_inactivity: Duration::from_secs(5),
                },
            )
            .await;
            finished.send(()).unwrap();
            exit
        };
        let (exit, ()) = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(client, server)
        })
        .await
        .expect("repair failure or stop retained the session");
        match mode {
            EndRepair::Stop => assert!(matches!(exit, Exit::Stop)),
            EndRepair::Unavailable => {
                assert!(matches!(exit, Exit::Retry(reason) if reason == "repair store unavailable"))
            }
            EndRepair::Deadline => assert!(
                matches!(exit, Exit::Retry(reason) if reason == "cursor reconciliation timed out")
            ),
        }
        ctx.engine.revoke();
    }
}

#[tokio::test(flavor = "current_thread")]
async fn the_actual_dispatch_guard_cancels_a_stalled_await_and_drops_its_owner() {
    struct Held(Arc<AtomicUsize>);
    impl Drop for Held {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let drops = Arc::new(AtomicUsize::new(0));
    let started = Arc::new(AtomicUsize::new(0));
    let held = Held(drops.clone());
    let (deadline, receiver) = watch::channel(None);
    let observed = started.clone();
    let work = async move {
        let _held = held;
        observed.fetch_add(1, Ordering::SeqCst);
        let _ = deadline.send_replace(Some(
            tokio::time::Instant::now() + Duration::from_millis(20),
        ));
        // Models any dispatcher write/flush future that never yields a result.
        std::future::pending::<Exit>().await
    };
    let (exit, expired) = tokio::time::timeout(
        Duration::from_secs(1),
        super::super::cursor_repair::drive(work, receiver),
    )
    .await
    .expect("the deadline guard must run outside the suspended dispatcher");
    assert!(matches!(exit, Exit::Retry(reason) if reason == "cursor reconciliation timed out"));
    assert!(expired);
    assert_eq!(started.load(Ordering::SeqCst), 1);
    assert_eq!(
        drops.load(Ordering::SeqCst),
        1,
        "the suspended owner's future was dropped exactly once"
    );
}

#[test]
fn a_ready_work_exit_cannot_reenter_socket_close_after_the_repair_budget() {
    let at = tokio::time::Instant::now();
    let policy = policy();
    let mut repair = CursorRepair::new(policy, at);
    // An unfinished initial catch-up also drops rather than awaits socket close.
    assert!(repair.abandon_transport(false, at));
    repair.completed(at);
    let until = at + policy.interval + policy.timeout;
    // Deterministically model work winning the fair select, so expired=false
    // and begin() has not run, while the watchdog can also already be ready.
    assert!(!repair.in_flight());
    assert!(!repair.abandon_transport(false, until - Duration::from_nanos(1)));
    assert!(repair.abandon_transport(false, until));
    assert!(repair.abandon_transport(false, until + Duration::from_nanos(1)));
    assert!(repair.abandon_transport(true, at));
    // If that work processed the real full SyncDone instead, the new deadline
    // is authoritative; an obsolete old timer must not abort the refreshed scope.
    assert!(repair.begin(at + policy.interval));
    assert!(repair.abandon_transport(false, at + policy.interval));
    repair.completed(until + Duration::from_nanos(1));
    assert!(!repair.in_flight());
    assert_eq!(
        repair.abort_deadline(),
        Some(until + Duration::from_nanos(1) + policy.interval + policy.timeout)
    );
    assert!(!repair.abandon_transport(false, until + Duration::from_nanos(1)));
}

#[tokio::test(flavor = "current_thread")]
async fn a_close_started_before_the_deadline_cannot_outlive_the_same_repair_budget() {
    struct Held(Arc<AtomicUsize>);
    impl Drop for Held {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let drops = Arc::new(AtomicUsize::new(0));
    let starts = Arc::new(AtomicUsize::new(0));
    let held = Held(drops.clone());
    let observed = starts.clone();
    let until = tokio::time::Instant::now() + Duration::from_millis(20);
    let close = async move {
        let _held = held;
        observed.fetch_add(1, Ordering::SeqCst);
        std::future::pending::<()>().await
    };
    tokio::time::timeout(
        Duration::from_secs(1),
        super::super::cursor_repair::close_transport(close, Some(until), false),
    )
    .await
    .expect("post-dispatch close must remain inside the existing repair budget");
    assert_eq!(
        starts.load(Ordering::SeqCst),
        1,
        "the close really began before expiry"
    );
    assert_eq!(
        drops.load(Ordering::SeqCst),
        1,
        "expiry drops its exact transport owner once"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn expired_budget_and_stop_never_poll_another_transport_close() {
    struct Held(Arc<AtomicUsize>);
    impl Drop for Held {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let now = tokio::time::Instant::now();
    for (deadline, abandon) in [
        (Some(now - Duration::from_millis(1)), false),
        (Some(now + Duration::from_secs(60)), true),
        (None, true),
    ] {
        let drops = Arc::new(AtomicUsize::new(0));
        let starts = Arc::new(AtomicUsize::new(0));
        let held = Held(drops.clone());
        let observed = starts.clone();
        let close = async move {
            let _held = held;
            observed.fetch_add(1, Ordering::SeqCst);
            std::future::pending::<()>().await
        };
        tokio::time::timeout(
            Duration::from_secs(1),
            super::super::cursor_repair::close_transport(close, deadline, abandon),
        )
        .await
        .expect("Stop or an expired budget cannot start another write");
        assert_eq!(starts.load(Ordering::SeqCst), 0);
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn legitimate_full_completion_retargets_the_cleanup_budget_before_close() {
    let now = tokio::time::Instant::now();
    let policy = policy();
    let old = now - policy.interval - policy.timeout - Duration::from_millis(10);
    let mut repair = CursorRepair::new(policy, old);
    repair.completed(old);
    assert!(repair.begin(old + policy.interval));
    let obsolete = repair.abort_deadline().unwrap();
    assert!(obsolete < now);
    repair.completed(now);
    let current = repair.abort_deadline();
    assert_eq!(current, Some(now + policy.interval + policy.timeout));
    let abandon = repair.abandon_transport(false, now);
    assert!(!abandon);
    let starts = Arc::new(AtomicUsize::new(0));
    let observed = starts.clone();
    super::super::cursor_repair::close_transport(
        async move {
            observed.fetch_add(1, Ordering::SeqCst);
        },
        current,
        abandon,
    )
    .await;
    assert_eq!(
        starts.load(Ordering::SeqCst),
        1,
        "current full completion permits the normal ready close"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn healthy_socket_discovers_new_authorized_space_when_joined_and_all_live_events_are_lost() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let directory = Directory(std::env::temp_dir().join(types::new_id("new-space-lost-joined")));
    std::fs::create_dir(&directory.0).unwrap();
    let path = directory.0.join("reader.sqlite");
    let canonical_path = directory.0.join("canonical.sqlite");
    let root = Signer::from_secret(&[124; 32]);
    let device = Signer::from_secret(&[125; 32]);
    let peer = Author::root(Signer::from_secret(&[126; 32]));
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
                    bio: "Synthetic missing-Joined fixture".into(),
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
    assert!(
        engine.cursors().is_empty(),
        "the initial full Sync has no known Space"
    );
    assert!(engine.store.events(SPACE).unwrap().is_empty());
    let canonical = store::Store::open(canonical_path.to_str().unwrap()).unwrap();
    assert!(canonical.events(SPACE).unwrap().is_empty());
    let engine = Arc::new(Mutex::new(engine));
    let (cmd, mut commands) = mpsc::unbounded_channel();
    let (status, _) = watch::channel(NetStatus {
        state: ConnState::Connecting,
        synced: false,
        error: None,
        registered: true,
        authenticated_relay: None,
    });
    let ctx = Ctx {
        engine: EngineLease::new(engine.clone()),
        listener: None,
        status,
        lang: crate::i18n::Lang::En,
        maintenance_work: None,
    };
    let (finished, closed) = oneshot::channel();
    let server = async {
        let mut socket = catch_up_socket(&listener).await;
        reply(&mut socket, ServerFrame::SyncDone).await;
        let mut marks = Vec::new();
        // Reuse the same non-content request barrier, but permit the legitimate
        // empty-cursor periodic scan if this initial handshake is scheduled slowly.
        let (answer_to, answer) = oneshot::channel();
        cmd.send(Cmd::Req {
            op: Op::FetchLink {
                id: "new-space-initial-complete".into(),
            },
            reply: answer_to,
        })
        .unwrap();
        loop {
            let frame = request(&mut socket).await;
            match &frame {
                ClientFrame::Req {
                    id,
                    op: Op::FetchLink { id: link },
                } if link == "new-space-initial-complete" => {
                    reply(
                        &mut socket,
                        ServerFrame::Res {
                            id: *id,
                            result: Ok(Reply::Link(None)),
                        },
                    )
                    .await;
                    break;
                }
                ClientFrame::Sync { cursors, all } => {
                    assert!(*all);
                    assert!(cursors.is_empty());
                    reply(&mut socket, ServerFrame::SyncDone).await;
                }
                _ => ordinary(&mut socket, frame, &mut marks).await,
            }
        }
        assert!(matches!(answer.await.unwrap(), Ok(Reply::Link(None))));
        assert!(ctx.status.borrow().synced);
        assert!(lock(&engine).cursors().is_empty());
        assert!(marks.is_empty());
        // Only now does a new durable room exist. Peer creates it and grants
        // this reader membership; no local create/ingest/outbox or refresh wakes it.
        let mut head = None;
        for (id, body) in [
            (
                "repair-create",
                EventBody::SpaceCreated {
                    title: "Missing Joined".into(),
                    kind: SpaceKind::Group,
                    privacy: Privacy::Closed,
                },
            ),
            (
                "repair-member",
                EventBody::MemberAdded {
                    identity: root.id(),
                    role: Role::Member,
                },
            ),
        ] {
            let event = sequence(
                Envelope::plain(&peer.sign_event(SPACE, id, AT, head.clone(), body)),
                &mut head,
            );
            canonical
                .append_event(&event.plain_event().unwrap())
                .unwrap();
        }
        let missing = sequence(
            Envelope::plain(&peer.sign_event(
                SPACE,
                EVENT,
                AT + 2,
                head.clone(),
                EventBody::MessagePosted {
                    message: "body-final-unannounced".into(),
                    text: TEXT.into(),
                    attaches: None,
                    reply: None,
                    media: None,
                    forwarded: false,
                },
            )),
            &mut head,
        );
        canonical
            .append_event(&missing.plain_event().unwrap())
            .unwrap();
        drop(canonical);
        let canonical = store::Store::open(canonical_path.to_str().unwrap()).unwrap();
        assert_eq!(canonical.events(SPACE).unwrap().len(), 3);
        assert!(lock(&engine).store.events(SPACE).unwrap().is_empty());
        assert!(lock(&engine).cursors().is_empty());
        // Suppress BOTH Joined and every live Event for this new room. There is
        // no later content or explicit refresh: only normal healthy transport.
        let mut heartbeats = tokio::time::interval(Duration::from_millis(20));
        let (cursors, all) = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                tokio::select! {
                    frame = request(&mut socket) => match frame {
                        ClientFrame::Sync { cursors, all } => break (cursors, all),
                        other => ordinary(&mut socket, other, &mut marks).await,
                    },
                    _ = heartbeats.tick() => reply(&mut socket, ServerFrame::Pong { server_time_ms: None }).await,
                }
            }
        }).await.expect("healthy Pongs must not conceal a new Space when Joined and every live Event are lost");
        assert!(
            all,
            "unknown authorized Spaces require the full membership scan"
        );
        assert!(
            cursors.is_empty(),
            "no known cursor can name the completely missed room"
        );
        assert!(
            marks.is_empty(),
            "no delivery watermark exists before the room is ingested"
        );
        // This is the mock relay's authorized full-scan result, backed by its
        // actual reopened SQLite rows. Real membership enforcement is separate.
        for row in canonical.events(SPACE).unwrap() {
            let recovered = Sequenced {
                seq: row.seq,
                prev: row.prev.clone(),
                hash: row.hash.clone(),
                env: Envelope::plain(&row),
            };
            reply(
                &mut socket,
                ServerFrame::Event {
                    ev: recovered.clone(),
                },
            )
            .await;
            reply(&mut socket, ServerFrame::Event { ev: recovered }).await;
        }
        reply(&mut socket, ServerFrame::SyncDone).await;
        barrier(&cmd, &mut socket, "repaired-content", &mut marks).await;
        let expected = assert_history(&lock(&engine), &peer.identity);
        assert!(
            lock(&engine).is_member(SPACE, &root.id()),
            "the real projection applied the signed membership grant"
        );
        let current = lock(&engine).cursors();
        assert_eq!(current.len(), 1);
        assert_eq!(current[0].space, SPACE);
        assert_eq!(current[0].next_seq, 3);
        tokio::time::timeout(Duration::from_secs(2), async {
            while !marks.contains(&2) {
                match request(&mut socket).await {
                    ClientFrame::Sync { cursors, all } => {
                        assert!(all);
                        assert_eq!(cursors.len(), 1);
                        assert_eq!(cursors[0].space, SPACE);
                        assert_eq!(cursors[0].next_seq, 3);
                        reply(&mut socket, ServerFrame::SyncDone).await;
                    }
                    other => ordinary(&mut socket, other, &mut marks).await,
                }
            }
        })
        .await
        .expect("newly discovered readable content must announce its durable cursor");
        assert_eq!(marks.iter().copied().max(), Some(2));
        cmd.send(Cmd::Stop).unwrap();
        closed.await.unwrap();
        assert!(
            futures_util::FutureExt::now_or_never(listener.accept()).is_none(),
            "recovery used the original healthy socket"
        );
        (expected, missing)
    };
    let mut backoff = Duration::from_millis(500);
    let client = async {
        let exit = session_with_repair_policy(&ctx, &mut commands, &mut backoff, policy()).await;
        finished.send(()).unwrap();
        exit
    };
    let (exit, (expected, missing)) = tokio::time::timeout(Duration::from_secs(12), async {
        tokio::join!(client, server)
    })
    .await
    .expect("new Space discovery stalled without its lost Joined notification");
    assert!(matches!(exit, Exit::Stop));
    ctx.engine.revoke();
    assert_eq!(Arc::strong_count(&engine), 1);
    drop(ctx);
    drop(engine);
    let mut reopened = Engine::open(path.to_str().unwrap()).unwrap();
    assert!(reopened.unlock(Some(device.secret().to_vec())).unwrap());
    assert_eq!(assert_history(&reopened, &peer.identity), expected);
    assert_eq!(reopened.ingest(missing), Ingest::Duplicate);
    assert_eq!(assert_history(&reopened, &peer.identity), expected);
}
