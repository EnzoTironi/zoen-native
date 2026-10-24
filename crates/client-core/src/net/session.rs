//! One authenticated ordered dispatcher with cursor repair ownership.

use super::{
    auth_message, complete_reads, cursor_repair, dispatch, flush, http_base, maintenance,
    media_worker, mpsc, recv, refused, register, send, tracing_like, watch, ws_url, AbortOnDrop,
    Arc, AuthenticatedRelay, ClaimClockSample, ClientFrame, Cmd, ConnState, Ctx, CursorRepair,
    CursorRepairPolicy, DeferredReceipts, Duration, Exit, HashMap, HashSet, KeyPackageClaimClock,
    Message, ProfileTraffic, ServerFrame, SinkExt, StreamExt, Waiting, CAPABILITIES,
    MAINTENANCE_PERIOD, MIN_PROTOCOL_VERSION, PROTOCOL_VERSION,
};

pub(super) async fn session(
    ctx: &Ctx,
    cmd_rx: &mut mpsc::UnboundedReceiver<Cmd>,
    backoff: &mut Duration,
) -> Exit {
    session_with_repair_policy(ctx, cmd_rx, backoff, CursorRepairPolicy::DEFAULT).await
}

pub(super) async fn session_with_repair_policy(
    ctx: &Ctx,
    cmd_rx: &mut mpsc::UnboundedReceiver<Cmd>,
    backoff: &mut Duration,
    repair_policy: CursorRepairPolicy,
) -> Exit {
    macro_rules! engine {
        ($work:expr) => {
            match ctx.engine($work) {
                Some(value) => value,
                None => return Exit::Stop,
            }
        };
    }

    let Some((identity, key, cert, profile, registered, relay_url)) =
        engine!(|engine| engine.net_credentials())
    else {
        return Exit::Blocked("locked: no account key on this device".into());
    };
    let url = ws_url(&relay_url);
    let ws = match tokio::time::timeout(
        Duration::from_secs(10),
        // Small frames must go out now, not wait for Nagle's delayed ACK.
        tokio_tungstenite::connect_async_with_config(url.as_str(), None, true),
    )
    .await
    {
        Ok(Ok((ws, _))) => ws,
        Ok(Err(e)) => return Exit::Retry(format!("can't reach the relay: {e}")),
        Err(_) => return Exit::Retry("can't reach the relay: timed out".into()),
    };
    let (mut sink, mut stream) = ws.split();

    // ── login ──
    let hello = ClientFrame::Hello {
        protocol: PROTOCOL_VERSION,
        capabilities: CAPABILITIES
            .iter()
            .map(std::string::ToString::to_string)
            .collect(),
        identity: identity.clone(),
        device: key.id(),
        cert,
    };
    if let Err(e) = send(&mut sink, &hello).await {
        return Exit::Retry(e);
    }
    let (nonce, relay, profiles, advertised_clock, complete_reads) = match recv(&mut stream).await {
        Ok(ServerFrame::Challenge {
            nonce,
            relay,
            capabilities,
            protocol,
            server_time_ms,
        }) if protocol >= MIN_PROTOCOL_VERSION => (
            nonce,
            relay,
            capabilities.iter().any(|c| c == "profiles"),
            capabilities
                .iter()
                .any(|c| c == protocol::KEY_PACKAGE_CLAIM_CAPABILITY)
                .then(|| {
                    server_time_ms
                        .and_then(KeyPackageClaimClock::from_server_ms)
                        .map(ClaimClockSample::new)
                })
                .flatten(),
            capabilities
                .iter()
                .any(|c| c == protocol::complete_reads::CAPABILITY),
        ),
        Ok(ServerFrame::Challenge { protocol, .. }) => {
            return Exit::Blocked(format!(
                "relay protocol {protocol} needs an upgrade before encrypted histories can sync"
            ));
        }
        Ok(ServerFrame::Error { code, message }) => return refused(code, message, backoff),
        Ok(other) => return Exit::Retry(format!("unexpected {other:?}")),
        Err(e) => return Exit::Retry(e),
    };
    if let Err(e) = send(
        &mut sink,
        &ClientFrame::Auth {
            sig: key.sign(&auth_message(&nonce, &relay)),
        },
    )
    .await
    {
        return Exit::Retry(e);
    }
    // Presence for people already online may race ahead of `ready`; keep it.
    let mut early: Vec<ServerFrame> = Vec::new();
    let relay_knows_me = loop {
        match recv(&mut stream).await {
            Ok(ServerFrame::Ready { registered, .. }) => break registered,
            Ok(ServerFrame::Error { code, message }) => return refused(code, message, backoff),
            Ok(f @ ServerFrame::Presence { .. }) if early.len() < 1024 => early.push(f),
            Ok(other) => return Exit::Retry(format!("unexpected {other:?}")),
            Err(e) => return Exit::Retry(e),
        }
    };
    // A Challenge sample belongs to this authenticated socket only. A later Pong
    // cannot enable a capability or clock that its Challenge did not advertise.
    let read_owner = complete_reads.then(|| (identity.clone(), key.id()));
    let clock_negotiated = advertised_clock.is_some();
    let mut claim_clock = advertised_clock;
    if !relay_knows_me || !registered {
        if let Err(error) =
            engine!(|engine| engine.agent_enrollment_registration_allowed(relay_knows_me))
        {
            return Exit::Blocked(error.to_string());
        }
        if let Err(e) = register(&mut sink, &mut stream, profile, &mut early).await {
            return if e == "handle_taken" {
                Exit::Blocked(e)
            } else {
                Exit::Retry(e)
            };
        }
        let _ = engine!(|engine| engine.set_registered(true));
    }
    // Publish the name only after this socket reached Ready and any registration
    // succeeded. A Challenge alone must not establish an HTTP signature domain.
    let _ = engine!(|engine| engine.store.set_meta("relay_name", &relay));
    engine!(|engine| engine.profiles_session_start(profiles));
    // Encrypted media moves over HTTP beside the socket, for as long as this session lives.
    let media_kick = Arc::new(tokio::sync::Notify::new());
    let _media = AbortOnDrop(tokio::spawn(crate::i18n::scope(
        ctx.lang,
        media_worker(
            ctx.engine.clone(),
            ctx.listener.clone(),
            http_base(&relay_url),
            relay.clone(),
            key.clone(),
            media_kick.clone(),
        ),
    )));
    ctx.set(|s| {
        s.state = ConnState::Online;
        s.error = None;
        s.registered = true;
        s.authenticated_relay = (!relay.is_empty()).then(|| AuthenticatedRelay {
            base: http_base(&relay_url),
            name: relay.clone(),
            identity: identity.clone(),
            device: key.id(),
        });
    });

    // ── catch up, then flush what we wrote offline ──
    let mut cursor_repair = CursorRepair::new(repair_policy, tokio::time::Instant::now());
    // Preserve the actual sent epoch even if maintenance reseals the outbox while
    // this publication is still waiting for the relay's answer.
    let mut sent: HashMap<String, Option<u64>> = HashMap::new();
    let mut waiting: HashMap<u64, Waiting> = HashMap::new();
    let mut next_id: u64 = 1;
    let mut deferred_reads = complete_reads::Deferred::new();
    let mut dirty: HashSet<String> = HashSet::new();
    let mut deferred_receipts = DeferredReceipts::default();
    let tick = tokio::time::sleep(Duration::ZERO);
    tokio::pin!(tick);
    let mut ping = tokio::time::interval(Duration::from_secs(25));
    let mut last_rx = tokio::time::Instant::now();
    let mut profiles_inflight: Option<tokio::time::Instant> = None;
    let mut traffic = ProfileTraffic::default();
    let mut opening: Option<tokio::time::Instant> = None;
    let mut claim_refusal_reported = false;

    // Registration can receive low-stock notices, events and presence before its reply.
    // Run them through the same ordered dispatcher as the following socket frames.
    let mut stream = futures_util::stream::iter(
        early
            .into_iter()
            .map(|f| Ok(Message::Binary(f.encode().into()))),
    )
    .chain(stream);

    let (repair_deadline, repair_deadline_rx) = watch::channel(cursor_repair.abort_deadline());
    let (exit, repair_expired) = cursor_repair::drive(async {
    // Initial admission belongs to this same guard. A blocked first Sync write
    // must not escape the inactivity owner by preceding the dispatcher.
    let cursors = engine!(|engine| engine.cursors());
    if let Err(e) = send(&mut sink, &ClientFrame::Sync { cursors, all: true }).await {
        return Exit::Retry(e);
    }
    if let Err(e) = flush(ctx, &mut sink, &mut sent).await {
        return Exit::Retry(e);
    }
    loop {
        if let Err(e) = complete_reads::drain(&mut sink,&mut waiting,&mut next_id,&mut deferred_reads).await { break Exit::Retry(e); }
        tokio::select! {
            () = async {
                match complete_reads::deadline(&waiting) {
                    Some(deadline) => tokio::time::sleep_until(deadline).await,
                    None => std::future::pending().await,
                }
            } => {
                // A silent missing page cannot hold this connection's read lane
                // forever. Closing releases the server snapshot and settles every
                // waiter without publishing or installing an incomplete set.
                break Exit::Retry("complete_read_expired".into());
            }
            frame = stream.next() => {
                let frame = match frame {
                    None => break Exit::Retry("relay closed the connection".into()),
                    Some(Err(e)) => break Exit::Retry(e.to_string()),
                    Some(Ok(Message::Binary(b))) => match ServerFrame::decode(&b) {
                        Ok(f) => f,
                        Err(e) => {
                            tracing_like(&format!("unreadable frame ({e}), {} bytes", b.len()));
                            continue;
                        }
                    },
                    Some(Ok(Message::Close(_))) => break Exit::Retry("relay closed the connection".into()),
                    Some(Ok(_)) => continue,
                };
                last_rx = tokio::time::Instant::now();
                if let Err(exit) = dispatch::dispatch_frame(frame, dispatch::FrameContext {
                    ctx, sink: &mut sink, sent: &mut sent, backoff,
                    cursor_repair: &mut cursor_repair, repair_deadline: &repair_deadline,
                    dirty: &mut dirty, deferred_receipts: &mut deferred_receipts,
                    waiting: &mut waiting, profiles_inflight: &mut profiles_inflight,
                    traffic: &mut traffic, opening: &mut opening,
                    claim_clock: &mut claim_clock, clock_negotiated,
                    media_kick: &media_kick,
                }).await { break exit; }

            }
            cmd = cmd_rx.recv() => match cmd {
                None | Some(Cmd::Stop) => break Exit::Stop,
                Some(Cmd::Flush) => {
                    // A profile edit needs a fresh login to re-register.
                    let needs_register = engine!(|engine| engine.account().is_some_and(|a| !a.registered));
                    if needs_register { break Exit::Retry("re-registering".into()) }
                    if let Err(e) = flush(ctx, &mut sink, &mut sent).await { break Exit::Retry(e) }
                    media_kick.notify_one();
                    ctx.notify_connection();
                }
                Some(Cmd::Req { op, reply }) => {
                    let (op, waiter) = complete_reads::start(op, Waiting::External(reply), read_owner.as_ref());
                    if let Err(e)=complete_reads::submit(op,waiter,&mut sink,&mut waiting,&mut next_id,&mut deferred_reads).await { break Exit::Retry(e); }
                }
                Some(Cmd::Ephemeral { space, kind }) => {
                    if let Err(e) = send(&mut sink, &ClientFrame::Ephemeral { space, kind }).await { break Exit::Retry(e) }
                }
            },
            () = &mut tick => {
                #[cfg(test)]
                if let Some(work) = &ctx.maintenance_work { work(); }
                if !ctx.status.borrow().synced {
                    // Catch-up can replace the cached group's epoch and membership.
                    // Keep offline plaintext intact until that ordered history lands.
                    tick.as_mut().reset(tokio::time::Instant::now() + MAINTENANCE_PERIOD);
                    continue;
                }
                if let Err(exit) = maintenance::maintain(maintenance::MaintenanceContext {
                    ctx, sink: &mut sink, sent: &mut sent, waiting: &mut waiting,
                    next_id: &mut next_id, dirty: &mut dirty,
                    profiles_inflight: &mut profiles_inflight, traffic: &mut traffic,
                    opening: &mut opening, claim_clock: &claim_clock,
                    claim_refusal_reported: &mut claim_refusal_reported,
                    media_kick: &media_kick,
                    read_owner: read_owner.as_ref(),
                    deferred_reads: &mut deferred_reads,
                }).await { break exit; }
                // A slow pass must leave time to drive the socket, rather than build a
                // burst of overdue scans over the same pending work.
                tick.as_mut().reset(tokio::time::Instant::now() + MAINTENANCE_PERIOD);
            }
            () = async {
                match cursor_repair.admission() {
                    Some(next) => tokio::time::sleep_until(next).await,
                    None => std::future::pending().await,
                }
            } => {
                let cursors = engine!(|engine| engine.cursors());
                if !cursor_repair.begin(tokio::time::Instant::now()) {
                    break Exit::Retry("too many outstanding sync requests".into());
                }
                let _ = repair_deadline.send_replace(cursor_repair.abort_deadline());
                if let Err(e) = send(&mut sink, &ClientFrame::Sync { cursors, all: true }).await {
                    break Exit::Retry(e);
                }
            }
            _ = ping.tick() => {
                if last_rx.elapsed() > Duration::from_secs(60) { break Exit::Retry("relay went quiet".into()) }
                if let Err(e) = send(&mut sink, &ClientFrame::Ping).await { break Exit::Retry(e) }
            }
        }
    }
    }, repair_deadline_rx).await;
    // A repair that failed/stopped is aborted, not flushed through another
    // unbounded socket close; durable outbox/cursors remain the retry authority.
    let repair_abandoned =
        cursor_repair.abandon_transport(repair_expired, tokio::time::Instant::now());
    // Include unsent claims if writing a batch failed before their waiting entry existed.
    engine!(super::super::engine::Engine::mls_claims_disconnected);
    for (_, w) in deferred_reads {
        w.fail("offline");
    }
    for (_, w) in waiting {
        w.fail("offline");
    }
    if !dirty.is_empty() {
        if let Some(l) = &ctx.listener {
            l.on_change(dirty.into_iter().filter(|s| !s.is_empty()).collect());
        }
    }
    cursor_repair::close_transport(
        sink.close(),
        cursor_repair.abort_deadline(),
        repair_abandoned || matches!(&exit, Exit::Stop),
    )
    .await;
    exit
}
