//! # The network task
//!
//! One WebSocket to the relay per device, owned by a small tokio runtime inside the
//! core. It logs in with the device key, catches up (`Sync`), flushes the outbox, then
//! streams. Disconnects back off (0.5 s → 30 s, jittered) and resume where they left off:
//! the cursors are just "next seq per Space" from the local log.
//!
//! Swift never sees sockets: it gets `CoreListener` callbacks (coalesced every 50 ms)
//! and calls the same synchronous engine API as before.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Duration,
};

use futures_util::{SinkExt, StreamExt};
use roda_proto::{
    auth_message, blob_put_message, ClientFrame, EphemeralKind, ErrorCode, Op, Reply, ServerFrame,
    CAPABILITIES, PROTOCOL_VERSION,
};
use roda_types::Identity;
use tokio::sync::{mpsc, oneshot, watch};
use tokio_tungstenite::tungstenite::Message;

use crate::engine::Engine;
use crate::sync::Ingest;
use crate::CoreListener;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnState {
    Offline,
    Connecting,
    Online,
}

#[derive(Clone, Debug)]
pub struct NetStatus {
    pub state: ConnState,
    /// A full catch-up finished on the current connection.
    pub synced: bool,
    pub error: Option<String>,
    pub registered: bool,
}

pub enum Cmd {
    Flush,
    Req {
        op: Op,
        reply: oneshot::Sender<Result<Reply, String>>,
    },
    Ephemeral {
        space: String,
        kind: EphemeralKind,
    },
    Stop,
}

pub struct Net {
    rt: Option<tokio::runtime::Runtime>,
    cmd: mpsc::UnboundedSender<Cmd>,
    status: watch::Receiver<NetStatus>,
}

type Shared = Arc<Mutex<Engine>>;

fn lock(e: &Shared) -> std::sync::MutexGuard<'_, Engine> {
    e.lock().unwrap_or_else(|p| p.into_inner())
}

/// The relay's HTTP base (blobs): `wss://x` → `https://x`, no trailing slash.
pub fn http_base(relay: &str) -> String {
    let base = relay.trim_end_matches('/');
    if let Some(rest) = base.strip_prefix("wss://") {
        format!("https://{rest}")
    } else if let Some(rest) = base.strip_prefix("ws://") {
        format!("http://{rest}")
    } else if base.starts_with("http://") || base.starts_with("https://") {
        base.to_string()
    } else {
        format!("https://{base}")
    }
}

pub fn ws_url(relay: &str) -> String {
    let base = relay.trim_end_matches('/');
    let base = if let Some(rest) = base.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = base.strip_prefix("http://") {
        format!("ws://{rest}")
    } else if base.starts_with("ws://") || base.starts_with("wss://") {
        base.to_string()
    } else {
        format!("wss://{base}")
    };
    format!("{base}/v1/sync")
}

impl Net {
    pub fn start(
        engine: Shared,
        listener: Option<Arc<dyn CoreListener>>,
        lang: crate::i18n::Lang,
    ) -> Result<Net, String> {
        // One TLS crypto provider for the WebSocket and HTTPS (ring; fails harmlessly if set).
        let _ = rustls::crypto::ring::default_provider().install_default();
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("zoen-net")
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (status_tx, status_rx) = watch::channel(NetStatus {
            state: ConnState::Offline,
            synced: false,
            error: None,
            registered: false,
        });
        {
            let poke_tx = cmd_tx.clone();
            lock(&engine).net.poke = Some(Box::new(move || {
                let _ = poke_tx.send(Cmd::Flush);
            }));
        }
        rt.spawn(run(
            Ctx {
                engine,
                listener,
                status: status_tx,
                lang,
            },
            cmd_rx,
        ));
        Ok(Net {
            rt: Some(rt),
            cmd: cmd_tx,
            status: status_rx,
        })
    }

    pub fn status(&self) -> NetStatus {
        self.status.borrow().clone()
    }

    pub fn flush(&self) {
        let _ = self.cmd.send(Cmd::Flush);
    }

    pub fn ephemeral(&self, space: String, kind: EphemeralKind) {
        let _ = self.cmd.send(Cmd::Ephemeral { space, kind });
    }

    /// A cloneable handle for one request (so callers don't hold locks across awaits).
    pub fn request_handle(&self) -> RequestHandle {
        RequestHandle {
            cmd: self.cmd.clone(),
        }
    }
}

impl Drop for Net {
    fn drop(&mut self) {
        let _ = self.cmd.send(Cmd::Stop);
        if let Some(rt) = self.rt.take() {
            // Safe from inside another async runtime too.
            rt.shutdown_background();
        }
    }
}

pub struct RequestHandle {
    cmd: mpsc::UnboundedSender<Cmd>,
}

impl RequestHandle {
    pub async fn send(self, op: Op) -> Result<Reply, String> {
        let (tx, rx) = oneshot::channel();
        self.cmd
            .send(Cmd::Req { op, reply: tx })
            .map_err(|_| "offline".to_string())?;
        match tokio::time::timeout(Duration::from_secs(15), rx).await {
            Ok(Ok(r)) => r,
            Ok(Err(_)) => Err("offline".into()),
            Err(_) => Err("the relay didn't answer in time".into()),
        }
    }
}

struct Ctx {
    engine: Shared,
    listener: Option<Arc<dyn CoreListener>>,
    status: watch::Sender<NetStatus>,
    lang: crate::i18n::Lang,
}

impl Ctx {
    fn set(&self, f: impl FnOnce(&mut NetStatus)) {
        self.status.send_modify(f);
        self.notify_connection();
    }

    fn notify_connection(&self) {
        if let Some(l) = &self.listener {
            let st = self.status.borrow().clone();
            let pending = lock(&self.engine).outbox_len();
            l.on_connection(crate::ConnectionDto::from_status(&st, pending));
        }
    }

    fn engine(&self) -> std::sync::MutexGuard<'_, Engine> {
        crate::i18n::set(self.lang);
        lock(&self.engine)
    }
}

enum Exit {
    Stop,
    /// Something only the user can fix (e.g. the handle is taken): wait for a poke.
    Blocked(String),
    Retry(String),
}

async fn run(ctx: Ctx, mut cmd_rx: mpsc::UnboundedReceiver<Cmd>) {
    let mut backoff = Duration::from_millis(500);
    loop {
        ctx.set(|s| {
            s.state = ConnState::Connecting;
            s.synced = false;
        });
        let exit = session(&ctx, &mut cmd_rx, &mut backoff).await;
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
                _ = &mut sleep, if !blocked => break,
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
fn refused(code: ErrorCode, message: String, backoff: &mut Duration) -> Exit {
    match code {
        ErrorCode::RateLimited => {
            if let Some(wait) = roda_proto::retry_hint(&message) {
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

fn rand_u8() -> u8 {
    let mut b = [0u8; 1];
    let _ = getrandom::getrandom(&mut b);
    b[0]
}

type Sink = futures_util::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    Message,
>;
type Stream = futures_util::stream::SplitStream<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
>;

async fn send(sink: &mut Sink, f: &ClientFrame) -> Result<(), String> {
    sink.send(Message::Binary(f.encode().into()))
        .await
        .map_err(|e| e.to_string())
}

async fn recv(stream: &mut Stream) -> Result<ServerFrame, String> {
    loop {
        match tokio::time::timeout(Duration::from_secs(15), stream.next()).await {
            Err(_) => return Err("relay timed out".into()),
            Ok(None) => return Err("relay closed the connection".into()),
            Ok(Some(Err(e))) => return Err(e.to_string()),
            Ok(Some(Ok(Message::Binary(b)))) => {
                return ServerFrame::decode(&b).map_err(|e| e.to_string())
            }
            Ok(Some(Ok(Message::Close(_)))) => return Err("relay closed the connection".into()),
            Ok(Some(Ok(_))) => continue,
        }
    }
}

enum Waiting {
    External(oneshot::Sender<Result<Reply, String>>),
    /// The ids asked for, so ones the relay doesn't know aren't asked again forever.
    Profiles(Vec<String>),
    AgreementPublished(String),
    ProfileUploaded(u64),
    AgreementKeys(Vec<String>),
    SealedProfiles(Vec<String>),
    KeyPackagesPublished,
    /// Key packages claimed for this Space's newcomers.
    Claimed(String),
}

/// Encrypted-profile requests: free (`None`), or in flight / backing off until a deadline.
#[derive(Default)]
struct ProfileTraffic {
    agreement: Option<tokio::time::Instant>,
    upload: Option<tokio::time::Instant>,
    keys: Option<tokio::time::Instant>,
    profiles: Option<tokio::time::Instant>,
    key_packages: Option<tokio::time::Instant>,
}

/// Free to send when nothing is in flight and any backoff has passed.
fn free(slot: &mut Option<tokio::time::Instant>) -> bool {
    match slot {
        Some(t) if *t > tokio::time::Instant::now() => false,
        _ => {
            *slot = None;
            true
        }
    }
}

fn settle(slot: &mut Option<tokio::time::Instant>, ok: bool) {
    *slot = (!ok).then(|| tokio::time::Instant::now() + Duration::from_secs(5));
}

const PROFILE_TIMEOUT: Duration = Duration::from_secs(30);

async fn session(
    ctx: &Ctx,
    cmd_rx: &mut mpsc::UnboundedReceiver<Cmd>,
    backoff: &mut Duration,
) -> Exit {
    let Some((identity, key, cert, profile, registered, relay_url)) =
        ctx.engine().net_credentials()
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
        capabilities: CAPABILITIES.iter().map(|c| c.to_string()).collect(),
        identity: identity.clone(),
        device: key.id(),
        cert,
    };
    if let Err(e) = send(&mut sink, &hello).await {
        return Exit::Retry(e);
    }
    let (nonce, relay, profiles) = match recv(&mut stream).await {
        Ok(ServerFrame::Challenge {
            nonce,
            relay,
            capabilities,
            ..
        }) => (nonce, relay, capabilities.iter().any(|c| c == "profiles")),
        Ok(ServerFrame::Error { code, message }) => return refused(code, message, backoff),
        Ok(other) => return Exit::Retry(format!("unexpected {other:?}")),
        Err(e) => return Exit::Retry(e),
    };
    // HTTP writes beside the socket (backups) sign for this name too.
    let _ = ctx.engine().store.set_meta("relay_name", &relay);
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
    for f in early {
        if let ServerFrame::Presence { identity, online } = f {
            ctx.engine().set_presence(&identity, online);
            if let Some(l) = &ctx.listener {
                l.on_presence(identity, online);
            }
        }
    }
    if !relay_knows_me || !registered {
        if let Err(e) = register(&mut sink, &mut stream, profile).await {
            return if e == "handle_taken" {
                Exit::Blocked(e)
            } else {
                Exit::Retry(e)
            };
        }
        let _ = ctx.engine().set_registered(true);
    }
    *backoff = Duration::from_millis(500);
    ctx.engine().profiles_session_start(profiles);
    // Encrypted media moves over HTTP beside the socket, for as long as this session lives.
    let media_kick = Arc::new(tokio::sync::Notify::new());
    let _media = AbortOnDrop(tokio::spawn(media_worker(
        ctx.engine.clone(),
        ctx.listener.clone(),
        http_base(&relay_url),
        relay.clone(),
        key.clone(),
        media_kick.clone(),
    )));
    ctx.set(|s| {
        s.state = ConnState::Online;
        s.error = None;
        s.registered = true;
    });

    // ── catch up, then flush what we wrote offline ──
    let cursors = ctx.engine().cursors();
    if let Err(e) = send(&mut sink, &ClientFrame::Sync { cursors, all: true }).await {
        return Exit::Retry(e);
    }
    let mut sent: HashSet<String> = HashSet::new();
    if let Err(e) = flush(ctx, &mut sink, &mut sent).await {
        return Exit::Retry(e);
    }

    let mut waiting: HashMap<u64, Waiting> = HashMap::new();
    let mut next_id: u64 = 1;
    let mut dirty: HashSet<String> = HashSet::new();
    let mut tick = tokio::time::interval(Duration::from_millis(50));
    let mut ping = tokio::time::interval(Duration::from_secs(25));
    let mut last_rx = tokio::time::Instant::now();
    let mut profiles_inflight: Option<tokio::time::Instant> = None;
    let mut traffic = ProfileTraffic::default();
    let mut opening: Option<tokio::time::Instant> = None;

    let exit = loop {
        tokio::select! {
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
                match frame {
                    ServerFrame::Event { ev } => {
                        let space = ev.env.space().to_string();
                        let commit = ev.env.sealed_kind() == Some(roda_log::content::SealedKind::Commit);
                        let r = ctx.engine().ingest(ev);
                        match r {
                            Ingest::Confirmed if commit => {
                                // Our commit is in: its Welcome, held until now, goes out.
                                dirty.insert(space);
                                if let Err(e) = flush(ctx, &mut sink, &mut sent).await { break Exit::Retry(e) }
                            }
                            Ingest::Applied | Ingest::Confirmed => { dirty.insert(space); media_kick.notify_one(); }
                            Ingest::Duplicate => {}
                            Ingest::Gap { next } => {
                                let c = vec![roda_proto::Cursor { space, next_seq: next }];
                                if let Err(e) = send(&mut sink, &ClientFrame::Sync { cursors: c, all: false }).await { break Exit::Retry(e) }
                            }
                            Ingest::Invalid(reason) => {
                                // A relay that serves a bad chain or signature is not trusted for this
                                // Space until a fresh sync; the event isn't stored.
                                tracing_like(&format!("refused event in {space}: {reason}"));
                            }
                        }
                    }
                    ServerFrame::Accepted { .. } => {}
                    ServerFrame::Rejected { space, client_id, reason, permanent } => {
                        sent.remove(&client_id);
                        if ctx.engine().reject(&client_id, &reason, permanent) { dirty.insert(space); }
                        if !permanent {
                            // Transient (rate limit, db hiccup): retry shortly.
                            let poke = ctx.engine().net.poke.is_some();
                            if poke { tokio::time::sleep(Duration::from_millis(500)).await; }
                            if let Err(e) = flush(ctx, &mut sink, &mut sent).await { break Exit::Retry(e) }
                        }
                        if let Some(l) = &ctx.listener { if permanent { l.on_error(reason); } }
                    }
                    ServerFrame::Ephemeral { space, from, kind } => {
                        if let Some(l) = &ctx.listener {
                            let (k, detail) = match kind {
                                EphemeralKind::Typing => ("typing".to_string(), String::new()),
                                EphemeralKind::StoppedTyping => ("stopped".to_string(), String::new()),
                                EphemeralKind::Status { status } => ("status".to_string(), status),
                                EphemeralKind::Read { seq } => ("read".to_string(), seq.to_string()),
                            };
                            l.on_ephemeral(space, from, k, detail);
                        }
                    }
                    ServerFrame::Presence { identity, online } => {
                        ctx.engine().set_presence(&identity, online);
                        if let Some(l) = &ctx.listener { l.on_presence(identity, online); }
                    }
                    ServerFrame::Joined { space } => {
                        let next = ctx.engine().next_seq(&space);
                        let c = vec![roda_proto::Cursor { space, next_seq: next }];
                        if let Err(e) = send(&mut sink, &ClientFrame::Sync { cursors: c, all: false }).await { break Exit::Retry(e) }
                    }
                    ServerFrame::SyncDone => {
                        let first = !ctx.status.borrow().synced;
                        if first {
                            ctx.status.send_modify(|s| s.synced = true);
                        }
                        // Anything still pending after a full catch-up goes out again (it
                        // may have needed the Space to exist first).
                        if let Err(e) = flush(ctx, &mut sink, &mut sent).await { break Exit::Retry(e) }
                        ctx.notify_connection();
                    }
                    ServerFrame::Res { id, result } => {
                        match waiting.remove(&id) {
                            Some(Waiting::External(tx)) => { let _ = tx.send(result); }
                            Some(Waiting::Profiles(asked)) => {
                                profiles_inflight = None;
                                let mut eng = ctx.engine();
                                if let Ok(Reply::Profiles(list)) = result {
                                    if !list.is_empty() && eng.put_profiles(list).is_ok() {
                                        dirty.insert(String::new());
                                    }
                                }
                                eng.profiles_answered(&asked);
                            }
                            Some(Waiting::AgreementPublished(public)) => {
                                let ok = matches!(result, Ok(Reply::Done));
                                if ok { ctx.engine().agreement_published(&public); } else { tracing_like(&format!("agreement key refused: {result:?}")); }
                                settle(&mut traffic.agreement, ok);
                            }
                            Some(Waiting::ProfileUploaded(version)) => {
                                let ok = matches!(result, Ok(Reply::Done));
                                let answer = result.map(|_| ());
                                ctx.engine().profile_upload_answered(version, answer);
                                settle(&mut traffic.upload, ok);
                            }
                            Some(Waiting::AgreementKeys(asked)) => {
                                let ok = if let Ok(Reply::AgreementKeys(list)) = result {
                                    ctx.engine().agreement_keys_arrived(list, &asked);
                                    true
                                } else { false };
                                settle(&mut traffic.keys, ok);
                            }
                            Some(Waiting::SealedProfiles(asked)) => {
                                opening = None;
                                let ok = if let Ok(Reply::SealedProfiles(list)) = result {
                                    let changed = ctx.engine().sealed_profiles_arrived(list, &asked);
                                    if !changed.is_empty() {
                                        dirty.insert(String::new());
                                        media_kick.notify_one();
                                        if let Some(l) = &ctx.listener { for id in changed { l.on_profile_changed(id); } }
                                    }
                                    true
                                } else { false };
                                settle(&mut traffic.profiles, ok);
                            }
                            Some(Waiting::KeyPackagesPublished) => {
                                let ok = matches!(result, Ok(Reply::Done));
                                ctx.engine().mls_key_packages_published(result.map(|_| ()));
                                settle(&mut traffic.key_packages, ok);
                            }
                            Some(Waiting::Claimed(space)) => {
                                let packages = match result {
                                    Ok(Reply::KeyPackages(list)) => Ok(list),
                                    Ok(other) => Err(format!("unexpected {other:?}")),
                                    Err(e) => Err(e),
                                };
                                ctx.engine().mls_claimed(&space, packages);
                                if let Err(e) = flush(ctx, &mut sink, &mut sent).await { break Exit::Retry(e) }
                            }
                            None => {}
                        }
                    }
                    ServerFrame::ProfileChanged { identity, .. } => ctx.engine().profile_changed(&identity),
                    ServerFrame::KeyPackagesLow { device, remaining } => ctx.engine().mls_key_packages_low(&device, remaining),
                    ServerFrame::Pong => {}
                    ServerFrame::Error { message, .. } => tracing_like(&format!("relay: {message}")),
                    ServerFrame::Challenge { .. } | ServerFrame::Ready { .. } => {}
                }
            }
            cmd = cmd_rx.recv() => match cmd {
                None | Some(Cmd::Stop) => break Exit::Stop,
                Some(Cmd::Flush) => {
                    // A profile edit needs a fresh login to re-register.
                    let needs_register = ctx.engine().account().map(|a| !a.registered).unwrap_or(false);
                    if needs_register { break Exit::Retry("re-registering".into()) }
                    if let Err(e) = flush(ctx, &mut sink, &mut sent).await { break Exit::Retry(e) }
                    media_kick.notify_one();
                    ctx.notify_connection();
                }
                Some(Cmd::Req { op, reply }) => {
                    let id = next_id; next_id += 1;
                    if let Err(e) = send(&mut sink, &ClientFrame::Req { id, op }).await { let _ = reply.send(Err(e.clone())); break Exit::Retry(e) }
                    waiting.insert(id, Waiting::External(reply));
                }
                Some(Cmd::Ephemeral { space, kind }) => {
                    if let Err(e) = send(&mut sink, &ClientFrame::Ephemeral { space, kind }).await { break Exit::Retry(e) }
                }
            },
            _ = tick.tick() => {
                if profiles_inflight.is_none() {
                    let unknown = ctx.engine().take_unknown();
                    if !unknown.is_empty() {
                        let id = next_id; next_id += 1;
                        if let Err(e) = send(&mut sink, &ClientFrame::Req { id, op: Op::Profiles { ids: unknown.clone() } }).await { break Exit::Retry(e) }
                        waiting.insert(id, Waiting::Profiles(unknown));
                        profiles_inflight = Some(tokio::time::Instant::now());
                    }
                }
                let shared = ctx.engine().profile_share_pass();
                match shared {
                    Ok(true) => if let Err(e) = flush(ctx, &mut sink, &mut sent).await { break Exit::Retry(e) },
                    Ok(false) => {}
                    Err(e) => tracing_like(&format!("profile shares: {e}")),
                }
                let mut reqs: Vec<(Op, Waiting)> = Vec::new();
                if free(&mut traffic.agreement) {
                    let publish = ctx.engine().agreement_to_publish();
                    if let Some((public, signed)) = publish {
                        traffic.agreement = Some(tokio::time::Instant::now() + PROFILE_TIMEOUT);
                        reqs.push((Op::PublishAgreementKey { public: public.clone(), signed }, Waiting::AgreementPublished(public)));
                    }
                }
                if free(&mut traffic.upload) {
                    let upload = ctx.engine().profile_to_upload();
                    if let Some(profile) = upload {
                        traffic.upload = Some(tokio::time::Instant::now() + PROFILE_TIMEOUT);
                        let version = profile.version;
                        reqs.push((Op::PutProfile { profile }, Waiting::ProfileUploaded(version)));
                    }
                }
                if free(&mut traffic.keys) {
                    let ids = ctx.engine().take_need_agreement();
                    if !ids.is_empty() {
                        traffic.keys = Some(tokio::time::Instant::now() + PROFILE_TIMEOUT);
                        reqs.push((Op::AgreementKeys { ids: ids.clone() }, Waiting::AgreementKeys(ids)));
                    }
                }
                if free(&mut traffic.profiles) {
                    let ids = ctx.engine().take_need_profiles();
                    if !ids.is_empty() {
                        traffic.profiles = Some(tokio::time::Instant::now() + PROFILE_TIMEOUT);
                        opening = Some(tokio::time::Instant::now());
                        reqs.push((Op::GetProfiles { ids: ids.clone() }, Waiting::SealedProfiles(ids)));
                    }
                }
                if free(&mut traffic.key_packages) {
                    let publish = ctx.engine().mls_key_packages_to_publish();
                    if let Some((packages, last_resort)) = publish {
                        traffic.key_packages = Some(tokio::time::Instant::now() + PROFILE_TIMEOUT);
                        reqs.push((Op::PublishKeyPackages { packages, last_resort }, Waiting::KeyPackagesPublished));
                    }
                }
                let claim = ctx.engine().mls_to_claim();
                if let Some((space, ids)) = claim {
                    reqs.push((Op::ClaimKeyPackages { ids }, Waiting::Claimed(space)));
                }
                let checkpoints = ctx.engine().mls_checkpoints();
                match checkpoints {
                    Ok(true) => if let Err(e) = flush(ctx, &mut sink, &mut sent).await { break Exit::Retry(e) },
                    Ok(false) => {}
                    Err(e) => tracing_like(&format!("checkpoints: {e}")),
                }
                let sealed = ctx.engine().mls_seal_outbox();
                match sealed {
                    Ok(true) => if let Err(e) = flush(ctx, &mut sink, &mut sent).await { break Exit::Retry(e) },
                    Ok(false) => {}
                    Err(e) => tracing_like(&format!("sealing: {e}")),
                }
                let mut failed = None;
                for (op, w) in reqs {
                    let id = next_id; next_id += 1;
                    if let Err(e) = send(&mut sink, &ClientFrame::Req { id, op }).await { failed = Some(e); break }
                    waiting.insert(id, w);
                }
                if let Some(e) = failed { break Exit::Retry(e) }
                // Hold UI updates briefly while we learn who new authors are, so the
                // app doesn't flash "Unknown" (or an @handle) for someone about to get a name.
                let hold = |t: &Option<tokio::time::Instant>| t.is_some_and(|t| t.elapsed() < Duration::from_millis(1500));
                let naming = hold(&profiles_inflight) || hold(&opening);
                if !dirty.is_empty() && !naming {
                    let spaces: Vec<String> = dirty.drain().filter(|s| !s.is_empty()).collect();
                    if let Some(l) = &ctx.listener { l.on_change(spaces); }
                    ctx.notify_connection();
                }
            }
            _ = ping.tick() => {
                if last_rx.elapsed() > Duration::from_secs(60) { break Exit::Retry("relay went quiet".into()) }
                if let Err(e) = send(&mut sink, &ClientFrame::Ping).await { break Exit::Retry(e) }
            }
        }
    };
    for (_, w) in waiting {
        if let Waiting::External(tx) = w {
            let _ = tx.send(Err("offline".into()));
        }
    }
    if !dirty.is_empty() {
        if let Some(l) = &ctx.listener {
            l.on_change(dirty.into_iter().filter(|s| !s.is_empty()).collect());
        }
    }
    let _ = sink.close().await;
    exit
}

async fn register(sink: &mut Sink, stream: &mut Stream, profile: Identity) -> Result<(), String> {
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
            _ => continue,
        }
    }
}

async fn flush(ctx: &Ctx, sink: &mut Sink, sent: &mut HashSet<String>) -> Result<(), String> {
    let envs = ctx.engine().outbox_envelopes_except(sent);
    for env in envs {
        if sent.insert(env.client_id().to_string()) {
            send(sink, &ClientFrame::Publish { env }).await?;
        }
    }
    Ok(())
}

struct AbortOnDrop(tokio::task::JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

fn now_ms() -> i64 {
    crate::engine::now_ms()
}

/// Uploads queued encrypted copies and downloads the ones the chats need, retrying with
/// backoff (a peer's event can arrive before their upload finishes).
async fn media_worker(
    engine: Shared,
    listener: Option<Arc<dyn CoreListener>>,
    base: String,
    relay: String,
    key: roda_log::Signer,
    kick: Arc<tokio::sync::Notify>,
) {
    let Ok(http) = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
    else {
        tracing_like("media: no HTTP client");
        return;
    };
    let mut retry_at: HashMap<String, (tokio::time::Instant, u32)> = HashMap::new();
    loop {
        // ── up ──
        let uploads = lock(&engine).pending_uploads(8);
        for (sha, bytes) in uploads {
            let ts = now_ms();
            let sig = key.sign(&blob_put_message(&sha, ts, &relay));
            let res = http
                .put(format!("{base}/v1/blobs/{sha}"))
                .header("x-zoen-device", key.id())
                .header("x-zoen-ts", ts.to_string())
                .header("x-zoen-sig", sig)
                .body(bytes)
                .send()
                .await;
            match res {
                Ok(r) if r.status().is_success() => lock(&engine).upload_done(&sha),
                Ok(r) if matches!(r.status().as_u16(), 400 | 413) => {
                    // The relay will never take it (corrupt or too big): don't loop forever.
                    tracing_like(&format!(
                        "media: upload {} refused ({})",
                        &sha[..12],
                        r.status()
                    ));
                    lock(&engine).upload_done(&sha);
                }
                Ok(r) => {
                    tracing_like(&format!(
                        "media: upload {} failed ({})",
                        &sha[..12],
                        r.status()
                    ));
                    break;
                }
                Err(e) => {
                    tracing_like(&format!("media: upload failed: {e}"));
                    break;
                }
            }
        }

        // ── down ──
        let wanted = lock(&engine).wanted_media();
        let now = tokio::time::Instant::now();
        let mut changed: Vec<String> = Vec::new();
        for w in wanted {
            if retry_at.get(&w.blob).is_some_and(|(t, _)| *t > now) {
                continue;
            }
            let got = match http.get(format!("{base}/v1/blobs/{}", w.blob)).send().await {
                Ok(r) if r.status().is_success() => r.bytes().await.ok(),
                _ => None,
            };
            let ok = got.is_some_and(|b| lock(&engine).media_arrived(&w, &b));
            if ok {
                retry_at.remove(&w.blob);
                if let Some(id) = &w.profile {
                    if let Some(l) = &listener {
                        l.on_profile_changed(id.clone());
                    }
                } else if !changed.contains(&w.space) {
                    changed.push(w.space.clone());
                }
            } else {
                let tries = retry_at.get(&w.blob).map(|(_, n)| n + 1).unwrap_or(1);
                let wait = Duration::from_secs(2u64.saturating_pow(tries.min(8)))
                    .min(Duration::from_secs(300));
                retry_at.insert(w.blob.clone(), (now + wait, tries));
            }
        }
        if !changed.is_empty() {
            if let Some(l) = &listener {
                l.on_change(changed);
            }
        }

        tokio::select! {
            _ = kick.notified() => {}
            _ = tokio::time::sleep(Duration::from_secs(5)) => {}
        }
    }
}

pub(crate) fn tracing_like(msg: &str) {
    if std::env::var_os("ZOEN_NET_DEBUG").is_some() {
        eprintln!("[zoen-net] {msg}");
    }
}
