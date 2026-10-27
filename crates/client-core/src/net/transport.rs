//! Socket I/O, reconnect outcomes and ordered publication writes.

use super::{
    mpsc, session_with_retries, tracing_like, ClientFrame, Cmd, ConnState, Ctx, Duration,
    ErrorCode, Exit, HashMap, Identity, Message, Op, PublicationRetries, ServerFrame, Sink,
    SinkExt, Stream, StreamExt,
};

pub(super) async fn run(ctx: Ctx, mut cmd_rx: mpsc::UnboundedReceiver<Cmd>) {
    let mut backoff = Duration::from_millis(500);
    let mut retries = PublicationRetries::default();
    loop {
        ctx.set(|s| {
            s.state = ConnState::Connecting;
            s.synced = false;
        });
        let exit = session_with_retries(&ctx, &mut cmd_rx, &mut backoff, &mut retries).await;
        let blocked = match exit {
            Exit::Stop => {
                ctx.set(|s| s.state = ConnState::Offline);
                return;
            }
            Exit::Blocked(e) => {
                ctx.set(|s| {
                    s.state = ConnState::Offline;
                    s.error = Some(e);
                });
                true
            }
            Exit::Retry(e) => {
                tracing_like(&format!("disconnected: {e}"));
                ctx.set(|s| {
                    s.state = ConnState::Offline;
                    s.synced = false;
                    s.error = Some(e);
                });
                false
            }
        };
        // Jittered backoff; a poke (new message, profile fix) retries sooner.
        let jitter = Duration::from_millis(u64::from(rand_u8()) * 2);
        let sleep = tokio::time::sleep(backoff + jitter);
        tokio::pin!(sleep);
        loop {
            tokio::select! {
                () = &mut sleep, if !blocked => break,
                cmd = cmd_rx.recv() => match cmd {
                    None | Some(Cmd::Stop) => { ctx.set(|s| s.state = ConnState::Offline); return }
                    Some(Cmd::Req { reply, .. }) => { let _ = reply.send(Err("offline".into())); }
                    Some(Cmd::Flush) if blocked => break,
                    Some(_) => {}
                }
            }
        }
        backoff = (backoff * 2).min(Duration::from_secs(30));
    }
}

/// A handshake refusal: a dependency hiccup or a rate limit retries (no sooner than the relay
/// asked); anything else waits for the user.
pub(super) fn refused(code: ErrorCode, message: String, backoff: &mut Duration) -> Exit {
    match code {
        ErrorCode::RateLimited => {
            if let Some(wait) = protocol::retry_hint(&message) {
                *backoff = (*backoff).max(wait);
            }
            Exit::Retry(message)
        }
        ErrorCode::Unavailable => Exit::Retry(message),
        ErrorCode::Other | ErrorCode::UpgradeRequired | ErrorCode::Unauthorized => {
            Exit::Blocked(message)
        }
    }
}

pub(super) fn rand_u8() -> u8 {
    let mut b = [0u8; 1];
    let _ = getrandom::getrandom(&mut b);
    b[0]
}

pub(super) async fn send(sink: &mut Sink, f: &ClientFrame) -> Result<(), String> {
    sink.send(Message::Binary(f.encode().into()))
        .await
        .map_err(|e| e.to_string())
}

pub(super) async fn recv(stream: &mut Stream) -> Result<ServerFrame, String> {
    loop {
        match tokio::time::timeout(Duration::from_secs(15), stream.next()).await {
            Err(_) => return Err("relay timed out".into()),
            Ok(None) => return Err("relay closed the connection".into()),
            Ok(Some(Err(e))) => return Err(e.to_string()),
            Ok(Some(Ok(Message::Binary(b)))) => {
                return ServerFrame::decode(&b).map_err(|e| e.to_string());
            }
            Ok(Some(Ok(Message::Close(_)))) => return Err("relay closed the connection".into()),
            Ok(Some(Ok(_))) => continue,
        }
    }
}

pub(super) async fn register(
    sink: &mut Sink,
    stream: &mut Stream,
    profile: Identity,
    early: &mut Vec<ServerFrame>,
) -> Result<(), String> {
    send(
        sink,
        &ClientFrame::Req {
            id: 0,
            op: Op::Register { profile },
        },
    )
    .await?;
    loop {
        match recv(stream).await? {
            ServerFrame::Res { id: 0, result } => return result.map(|_| ()),
            ServerFrame::Error { message, .. } => return Err(message),
            frame if early.len() < 1024 => early.push(frame),
            _ => return Err("too many frames before the registration reply".into()),
        }
    }
}

pub(super) async fn flush(
    ctx: &Ctx,
    sink: &mut Sink,
    sent: &mut HashMap<String, Option<u64>>,
    retries: &mut PublicationRetries,
) -> Result<(), String> {
    if !ctx.status.borrow().synced {
        return Ok(());
    }
    let (envs, pending, mut unsettled) = ctx
        .engine(|engine| {
            let mut unsettled = retries.waiting_spaces(&engine.net.pending);
            unsettled.extend(
                sent.keys()
                    .filter_map(|id| engine.net.pending.get(id).cloned()),
            );
            (
                engine.outbox_envelopes_except(sent),
                engine.net.pending.len(),
                unsettled,
            )
        })
        .ok_or_else(|| "offline".to_string())?;
    if pending != 0 {
        tracing_like(&format!(
            "publication offer pending={pending} offered={} in_flight={}",
            envs.len(),
            sent.len()
        ));
    }
    for env in envs {
        // A later envelope can be accepted after an earlier transient refusal.
        // Keep each Space behind its unsettled signed head; unrelated Spaces
        // still share this pass. Accepted notices alone cannot release a lane;
        // confirmation or a terminal refusal resolves its older intent.
        if unsettled.contains(env.space()) {
            continue;
        }
        if let std::collections::hash_map::Entry::Vacant(entry) =
            sent.entry(env.client_id().to_string())
        {
            let epoch = env.sealed_data().and_then(|(kind, data)| match kind {
                event_log::content::SealedKind::Application => ::mls::application_epoch(data),
                _ => None,
            });
            unsettled.insert(env.space().to_string());
            entry.insert(epoch);
            send(sink, &ClientFrame::Publish { env }).await?;
            tracing_like(&format!("publication submitted in_flight={}", sent.len()));
        }
    }
    Ok(())
}
