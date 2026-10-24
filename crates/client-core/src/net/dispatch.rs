//! Ordered server frames borrow the one live session's local bookkeeping.

use super::*;

/// A per-call borrow, never another cursor, receipt or connection owner.
pub(super) struct FrameContext<'a> {
    pub(super) ctx: &'a Ctx,
    pub(super) sink: &'a mut Sink,
    pub(super) sent: &'a mut HashMap<String, Option<u64>>,
    pub(super) backoff: &'a mut Duration,
    pub(super) cursor_repair: &'a mut CursorRepair,
    pub(super) repair_deadline: &'a watch::Sender<Option<tokio::time::Instant>>,
    pub(super) dirty: &'a mut HashSet<String>,
    pub(super) deferred_receipts: &'a mut DeferredReceipts,
    pub(super) waiting: &'a mut HashMap<u64, Waiting>,
    pub(super) profiles_inflight: &'a mut Option<tokio::time::Instant>,
    pub(super) traffic: &'a mut ProfileTraffic,
    pub(super) opening: &'a mut Option<tokio::time::Instant>,
    pub(super) claim_clock: &'a mut Option<ClaimClockSample>,
    pub(super) clock_negotiated: bool,
    pub(super) media_kick: &'a tokio::sync::Notify,
}

pub(super) async fn dispatch_frame(
    frame: ServerFrame,
    context: FrameContext<'_>,
) -> Result<(), Exit> {
    let FrameContext {
        ctx,
        sink,
        sent,
        backoff,
        cursor_repair,
        repair_deadline,
        dirty,
        deferred_receipts,
        waiting,
        profiles_inflight,
        traffic,
        opening,
        claim_clock,
        clock_negotiated,
        media_kick,
    } = context;
    macro_rules! engine {
        ($work:expr) => {
            match ctx.engine($work) {
                Some(value) => value,
                None => return Err(Exit::Stop),
            }
        };
    }
    match frame {
        ServerFrame::Event { ev } => {
            let space = ev.env.space().to_string();
            let client_id = ev.env.client_id().to_string();
            let (r, advanced) = engine!(|engine| {
                let before = engine.next_seq(&space);
                let result = engine.ingest(ev);
                let advanced = matches!(&result, Ingest::Applied | Ingest::Confirmed)
                    && engine.next_seq(&space) > before;
                (result, advanced)
            });
            if advanced && cursor_repair.progressed(tokio::time::Instant::now()) {
                let _ = repair_deadline.send_replace(cursor_repair.abort_deadline());
            }
            if r == Ingest::Confirmed {
                let tracked = sent.remove(&client_id).is_some();
                tracing_like(&format!(
                    "publication confirmed tracked={tracked} in_flight={}",
                    sent.len()
                ));
            }
            match r {
                Ingest::Confirmed => {
                    // Release Welcomes behind commits and descendants behind
                    // an offline genesis as soon as their predecessor lands.
                    dirty.insert(space);
                    media_kick.notify_one();
                    if let Err(e) = flush(ctx, sink, sent).await {
                        return Err(Exit::Retry(e));
                    }
                }
                Ingest::Applied => {
                    dirty.insert(space);
                    media_kick.notify_one();
                }
                Ingest::Duplicate => {}
                Ingest::Gap { next } => {
                    let c = vec![protocol::Cursor {
                        space,
                        next_seq: next,
                    }];
                    if let Err(e) = send(
                        sink,
                        &ClientFrame::Sync {
                            cursors: c,
                            all: false,
                        },
                    )
                    .await
                    {
                        return Err(Exit::Retry(e));
                    }
                    if !cursor_repair.submitted() {
                        return Err(Exit::Retry("too many outstanding sync requests".into()));
                    }
                }
                Ingest::Invalid(reason) => {
                    // A relay that serves a bad chain or signature is not trusted for this
                    // Space until a fresh sync; the event isn't stored.
                    tracing_like(&format!("refused event in {space}: {reason}"));
                }
            }
            if let Err(e) = engine!(|engine| deferred_receipts.flush(engine, dirty)) {
                return Err(Exit::Retry(e.to_string()));
            }
        }
        ServerFrame::Accepted { client_id, .. } => {
            tracing_like(&format!(
                "publication accepted tracked={} in_flight={}",
                sent.contains_key(&client_id),
                sent.len()
            ));
        }
        ServerFrame::Rejected {
            space,
            client_id,
            reason,
            permanent,
        } => {
            let sent_epoch = sent.remove(&client_id).flatten();
            let outcome =
                engine!(|engine| engine.reject(&client_id, &reason, permanent, sent_epoch));
            tracing_like(&format!(
                "publication {client_id} refused: {reason}; {outcome:?}"
            ));
            if outcome.user_failed() {
                dirty.insert(space);
            }
            if !permanent {
                // Transient (rate limit, db hiccup): retry shortly.
                let poke = engine!(|engine| engine.net.poke.is_some());
                if poke {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }
            if matches!(
                outcome,
                RejectionOutcome::Retry | RejectionOutcome::UserFailed
            ) {
                // Retry may already have a current sealed copy. A terminal
                // dependency failure can release later Space heads immediately.
                if let Err(e) = flush(ctx, sink, sent).await {
                    return Err(Exit::Retry(e));
                }
            }
            report_rejection(ctx, &outcome);
        }
        ServerFrame::Ephemeral { space, from, kind } => {
            let mark = match kind {
                EphemeralKind::Delivered { seq } => Some((seq, 0)),
                EphemeralKind::Read { seq } => Some((0, seq)),
                _ => None,
            };
            if let Some((d, r)) = mark {
                match engine!(|engine| deferred_receipts.receive(engine, &space, &from, d, r)) {
                    Ok(true) => {
                        dirty.insert(space.clone());
                    }
                    Ok(false) => {}
                    Err(e) => return Err(Exit::Retry(e.to_string())),
                }
            }
            if let Some(l) = &ctx.listener {
                let (k, detail) = match kind {
                    EphemeralKind::Typing => ("typing".to_string(), String::new()),
                    EphemeralKind::StoppedTyping => ("stopped".to_string(), String::new()),
                    EphemeralKind::Status { status } => ("status".to_string(), status),
                    EphemeralKind::Read { seq } => ("read".to_string(), seq.to_string()),
                    EphemeralKind::Delivered { seq } => ("delivered".to_string(), seq.to_string()),
                };
                l.on_ephemeral(space, from, k, detail);
            }
        }
        ServerFrame::Presence { identity, online } => {
            engine!(|engine| engine.set_presence(&identity, online));
            if let Some(l) = &ctx.listener {
                l.on_presence(identity, online);
            }
        }
        ServerFrame::Joined { space } => {
            let next = engine!(|engine| engine.next_seq(&space));
            let c = vec![protocol::Cursor {
                space,
                next_seq: next,
            }];
            if let Err(e) = send(
                sink,
                &ClientFrame::Sync {
                    cursors: c,
                    all: false,
                },
            )
            .await
            {
                return Err(Exit::Retry(e));
            }
            if !cursor_repair.submitted() {
                return Err(Exit::Retry("too many outstanding sync requests".into()));
            }
        }
        ServerFrame::SyncDone => {
            cursor_repair.completed(tokio::time::Instant::now());
            let _ = repair_deadline.send_replace(cursor_repair.abort_deadline());
            if let Err(e) = engine!(|engine| deferred_receipts.flush(engine, dirty)) {
                return Err(Exit::Retry(e.to_string()));
            }
            let first = !ctx.status.borrow().synced;
            if first {
                // Authentication can succeed while catch-up dependencies fail.
                // Reset retries only after this connection actually caught up.
                *backoff = Duration::from_millis(500);
                ctx.status.send_modify(|s| s.synced = true);
                engine!(|engine| engine.mls_refresh_checkpoints());
                engine!(|engine| engine.net.link.ask_devices = true);
                // Ephemeral sends may have been lost before this socket existed.
                // Rebuild receipts after catch-up, even with no new remote events.
                let receipts = engine!(|engine| engine.receipts_to_replay());
                let mut failed = None;
                for (space, kind) in receipts {
                    if let Err(e) = send(
                        sink,
                        &ClientFrame::Ephemeral {
                            space: space.clone(),
                            kind: kind.clone(),
                        },
                    )
                    .await
                    {
                        failed = Some(e);
                        break;
                    }
                    if let EphemeralKind::Delivered { seq } = kind {
                        engine!(|engine| engine.delivered_announced(&space, seq));
                    }
                }
                if let Some(e) = failed {
                    return Err(Exit::Retry(e));
                }
            }
            if let Err(e) = engine!(|engine| engine.mls_ask_rejoin()) {
                tracing_like(&format!("rejoin: {e}"));
            }
            // Anything still pending after a full catch-up goes out again (it
            // may have needed the Space to exist first).
            if let Err(e) = flush(ctx, sink, sent).await {
                return Err(Exit::Retry(e));
            }
            ctx.notify_connection();
        }
        ServerFrame::Res { id, result } => {
            responses::dispatch_reply(
                id,
                result,
                responses::ReplyContext {
                    ctx,
                    sink,
                    sent,
                    waiting,
                    profiles_inflight,
                    traffic,
                    opening,
                    dirty,
                    media_kick,
                },
            )
            .await?;
        }
        ServerFrame::ProfileChanged { identity, .. } => {
            let own = engine!(|engine| {
                engine.profile_changed(&identity);
                engine.me.as_deref() == Some(identity.as_str())
            });
            tracing_like(&format!("sealed profile invalidated own={own}"));
        }
        ServerFrame::KeyPackagesLow { device, remaining } => {
            engine!(|engine| engine.mls_key_packages_low(&device, remaining))
        }
        ServerFrame::DeviceMessage { from, to, sealed } => {
            if let Err(e) = engine!(
                |engine| deferred_receipts.device_message(engine, &from, &to, &sealed, dirty)
            ) {
                return Err(Exit::Retry(e.to_string()));
            }
            if engine!(|engine| engine.take_initial_history_ready()) {
                if let Some(listener) = &ctx.listener {
                    listener.on_ephemeral(
                        String::new(),
                        from,
                        "history_ready".to_owned(),
                        String::new(),
                    );
                }
            }
        }
        ServerFrame::Pong { server_time_ms } => {
            if clock_negotiated {
                *claim_clock = server_time_ms
                    .and_then(KeyPackageClaimClock::from_server_ms)
                    .map(ClaimClockSample::new);
            }
        }
        // A failed initial/repair Sync has no SyncDone and leaves the socket open.
        // Reconnect to retry it; the normal run loop bounds retry frequency.
        ServerFrame::Error {
            code: ErrorCode::Unavailable,
            message,
        } if !ctx.status.borrow().synced || cursor_repair.in_flight() => {
            return Err(Exit::Retry(message))
        }
        ServerFrame::Error { message, .. } => tracing_like(&format!("relay: {message}")),
        ServerFrame::Challenge { .. } | ServerFrame::Ready { .. } => {}
    }
    Ok(())
}
