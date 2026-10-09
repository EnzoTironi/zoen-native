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
                #[cfg(test)]
                maintenance_work: None,
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
    #[cfg(test)]
    maintenance_work: Option<Arc<dyn Fn() + Send + Sync>>,
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
const MAINTENANCE_PERIOD: Duration = Duration::from_millis(50);

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
    if !relay_knows_me || !registered {
        if let Err(e) = register(&mut sink, &mut stream, profile, &mut early).await {
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
    // Preserve the actual sent epoch even if maintenance reseals the outbox while
    // this publication is still waiting for the relay's answer.
    let mut sent: HashMap<String, Option<u64>> = HashMap::new();
    if let Err(e) = flush(ctx, &mut sink, &mut sent).await {
        return Exit::Retry(e);
    }

    let mut waiting: HashMap<u64, Waiting> = HashMap::new();
    let mut next_id: u64 = 1;
    let mut dirty: HashSet<String> = HashSet::new();
    let tick = tokio::time::sleep(Duration::ZERO);
    tokio::pin!(tick);
    let mut ping = tokio::time::interval(Duration::from_secs(25));
    let mut last_rx = tokio::time::Instant::now();
    let mut profiles_inflight: Option<tokio::time::Instant> = None;
    let mut traffic = ProfileTraffic::default();
    let mut opening: Option<tokio::time::Instant> = None;

    // Registration can receive low-stock notices, events and presence before its reply.
    // Run them through the same ordered dispatcher as the following socket frames.
    let mut stream = futures_util::stream::iter(
        early
            .into_iter()
            .map(|f| Ok(Message::Binary(f.encode().into()))),
    )
    .chain(stream);

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
                        let client_id = ev.env.client_id().to_string();
                        let commit = ev.env.sealed_kind() == Some(roda_log::content::SealedKind::Commit);
                        let r = ctx.engine().ingest(ev);
                        if r == Ingest::Confirmed { sent.remove(&client_id); }
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
                        let sent_epoch = sent.remove(&client_id).flatten();
                        let recoverable = reason == roda_proto::STALE_SEAL || reason == roda_proto::SEAL_REQUIRED;
                        if ctx.engine().reject(&client_id, &reason, permanent, sent_epoch) { dirty.insert(space); }
                        if !permanent {
                            // Transient (rate limit, db hiccup): retry shortly.
                            let poke = ctx.engine().net.poke.is_some();
                            if poke { tokio::time::sleep(Duration::from_millis(500)).await; }
                        }
                        if !permanent || recoverable {
                            // The relay refuses the old encoding permanently, but the
                            // queued plaintext may already have a current sealed copy.
                            if let Err(e) = flush(ctx, &mut sink, &mut sent).await { break Exit::Retry(e) }
                        }
                        if let Some(l) = &ctx.listener { if permanent && !recoverable { l.on_error(reason); } }
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
            _ = &mut tick => {
                #[cfg(test)]
                if let Some(work) = &ctx.maintenance_work { work(); }
                if !ctx.status.borrow().synced {
                    // Catch-up can replace the cached group's epoch and membership.
                    // Keep offline plaintext intact until that ordered history lands.
                    tick.as_mut().reset(tokio::time::Instant::now() + MAINTENANCE_PERIOD);
                    continue;
                }
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
                // A slow pass must leave time to drive the socket, rather than build a
                // burst of overdue scans over the same pending work.
                tick.as_mut().reset(tokio::time::Instant::now() + MAINTENANCE_PERIOD);
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

async fn register(
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

async fn flush(
    ctx: &Ctx,
    sink: &mut Sink,
    sent: &mut HashMap<String, Option<u64>>,
) -> Result<(), String> {
    if !ctx.status.borrow().synced {
        return Ok(());
    }
    let envs = ctx.engine().outbox_envelopes_except(sent);
    for env in envs {
        if let std::collections::hash_map::Entry::Vacant(entry) =
            sent.entry(env.client_id().to_string())
        {
            let epoch = env.sealed_data().and_then(|(kind, data)| match kind {
                roda_log::content::SealedKind::Application => roda_mls::application_epoch(data),
                _ => None,
            });
            entry.insert(epoch);
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

#[cfg(test)]
mod tests {
    use super::*;
    use roda_log::{
        chain_hash, content::InnerEvent, content::Sealed, content::SealedKind, Author, Signer,
    };
    use roda_mls::{sealed::state_key, Device, Opened, SUITE_ID};
    use roda_proto::{Envelope, Sequenced};
    use roda_types::{EventBody, Privacy, Seen, SpaceKind, GENESIS_PREV};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc as channel,
    };
    use tokio_tungstenite::WebSocketStream;

    type RelaySocket = WebSocketStream<tokio::net::TcpStream>;

    async fn request(socket: &mut RelaySocket) -> ClientFrame {
        loop {
            let message = tokio::time::timeout(Duration::from_secs(5), socket.next())
                .await
                .expect("client stalled")
                .expect("client closed")
                .expect("client socket");
            if let Message::Binary(bytes) = message {
                return ClientFrame::decode(&bytes).expect("client frame");
            }
        }
    }

    async fn reply(socket: &mut RelaySocket, frame: ServerFrame) {
        socket
            .send(Message::Binary(frame.encode().into()))
            .await
            .unwrap();
    }

    fn sequence(env: Envelope, head: &mut Option<Seen>) -> Sequenced {
        let seq = head.as_ref().map_or(0, |h| h.seq + 1);
        let prev = head
            .as_ref()
            .map_or_else(|| GENESIS_PREV.to_string(), |h| h.hash.clone());
        let hash = chain_hash(env.space(), seq, &prev, &env.wire_hash());
        *head = Some(Seen {
            seq,
            hash: hash.clone(),
        });
        Sequenced {
            seq,
            prev,
            hash,
            env,
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn queued_messages_wait_for_catch_up_and_use_the_current_epoch() {
        queued_messages_round_trip(CatchUp::Initial).await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn late_stale_rejections_preserve_resealed_messages_at_the_current_epoch() {
        queued_messages_round_trip(CatchUp::Incremental).await;
    }

    enum CatchUp {
        Initial,
        Incremental,
    }

    async fn queued_messages_round_trip(catch_up: CatchUp) {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let mut engine = Engine::open(":memory:").unwrap();
        let (root_secret, device_secret, _) = engine.create_account("Ana", "ana", &url).unwrap();
        engine.set_registered(true).unwrap();
        let _ = engine.mls_key_packages_to_publish().unwrap();
        engine.mls_key_packages_published(Ok(()));
        let author = engine.net.author.clone().unwrap();
        let space = "sp_initial_catch_up";
        let mut head = None;
        let genesis = sequence(
            Envelope::plain(&author.sign_event(
                space,
                "created",
                1,
                None,
                EventBody::SpaceCreated {
                    title: "Offline messages".into(),
                    kind: SpaceKind::Group,
                    privacy: Privacy::EndToEnd,
                },
            )),
            &mut head,
        );
        assert_eq!(engine.ingest(genesis), Ingest::Applied);
        engine.create_mls_group(space).unwrap();

        // A second enrolled leaf of the same identity can read both offline messages
        // and supplies a genuine MLS commit that this session has not caught up with.
        let peer_author = Author::device(&Signer::from_secret(&root_secret), Signer::generate());
        let mut peer_store = rusqlite::Connection::open_in_memory().unwrap();
        roda_mls::migrate(&mut peer_store).unwrap();
        let peer_secret = peer_author.key.secret();
        let peer = Device::new(
            &peer_store,
            state_key(&peer_secret),
            &peer_author.identity,
            peer_secret,
            peer_author.cert.as_deref().unwrap(),
        )
        .unwrap();
        let packages = peer.key_packages(1, false).unwrap();
        let adding = Device::new(
            engine.store.conn(),
            state_key(&device_secret),
            &author.identity,
            device_secret,
            author.cert.as_deref().unwrap(),
        )
        .unwrap()
        .commit(space, &packages, &Default::default())
        .unwrap();
        let commit = sequence(
            Envelope::sealed(
                &author,
                space,
                "add-peer",
                2,
                head.as_ref(),
                Sealed::new(SealedKind::Commit, SUITE_ID, adding.commit),
            ),
            &mut head,
        );
        assert_eq!(engine.ingest(commit), Ingest::Applied);
        let roster = [author.identity.clone()].into_iter().collect();
        let welcome = adding.welcome.unwrap();
        assert!(peer.join(space, &welcome, &roster).unwrap());
        let welcome = sequence(
            Envelope::sealed(
                &author,
                space,
                "welcome-peer",
                3,
                head.as_ref(),
                Sealed::new(SealedKind::Welcome, SUITE_ID, welcome),
            ),
            &mut head,
        );
        assert_eq!(engine.ingest(welcome), Ingest::Applied);
        assert_eq!(engine.mls_status(space).unwrap().0, 1);

        let cached = engine
            .append_synced(
                space,
                &author,
                100,
                EventBody::MessagePosted {
                    message: "cached-message".into(),
                    text: "Written offline, already sealed".into(),
                    attaches: None,
                    reply: None,
                },
            )
            .unwrap();
        assert!(engine.mls_seal_outbox().unwrap());
        let cached_key = format!("mls.sealed:{}", cached.client_id);
        let cached_copy = engine.store.meta(&cached_key).unwrap().unwrap();
        let fresh = engine
            .append_synced(
                space,
                &author,
                101,
                EventBody::MessagePosted {
                    message: "plaintext-message".into(),
                    text: "Written offline, still plaintext".into(),
                    attaches: None,
                    reply: None,
                },
            )
            .unwrap();
        let fresh_key = format!("mls.sealed:{}", fresh.client_id);
        assert!(engine.store.meta(&fresh_key).unwrap().is_none());
        let advance = peer.commit(space, &[], &Default::default()).unwrap();
        assert!(matches!(
            peer.open(space, &advance.commit, &roster).unwrap(),
            Opened::Commit { epoch: 2 }
        ));
        let missed_commit = sequence(
            Envelope::sealed(
                &peer_author,
                space,
                "missed-commit",
                102,
                head.as_ref(),
                Sealed::new(SealedKind::Commit, SUITE_ID, advance.commit),
            ),
            &mut head,
        );
        let engine = Arc::new(Mutex::new(engine));
        let (cmd, mut commands) = mpsc::unbounded_channel();
        let (status, _) = watch::channel(NetStatus {
            state: ConnState::Connecting,
            synced: false,
            error: None,
            registered: true,
        });
        let (maintenance, mut maintenance_started) = mpsc::unbounded_channel();
        let ctx = Ctx {
            engine: engine.clone(),
            listener: None,
            status,
            lang: crate::i18n::Lang::En,
            maintenance_work: Some(Arc::new(move || {
                let _ = maintenance.send(());
            })),
        };
        let (client_stopped, stopped) = oneshot::channel();
        let client = async {
            let mut backoff = Duration::from_millis(500);
            assert!(matches!(
                session(&ctx, &mut commands, &mut backoff).await,
                Exit::Stop
            ));
            let _ = client_stopped.send(());
        };
        let relay = async {
            let (socket, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(socket).await.unwrap();
            let ClientFrame::Hello { identity, .. } = request(&mut socket).await else {
                panic!("hello")
            };
            reply(
                &mut socket,
                ServerFrame::Challenge {
                    nonce: "catch-up-test".into(),
                    relay: "catch-up-test".into(),
                    protocol: PROTOCOL_VERSION,
                    capabilities: Vec::new(),
                },
            )
            .await;
            assert!(matches!(
                request(&mut socket).await,
                ClientFrame::Auth { .. }
            ));
            reply(
                &mut socket,
                ServerFrame::Ready {
                    identity,
                    registered: true,
                },
            )
            .await;
            assert!(matches!(
                request(&mut socket).await,
                ClientFrame::Sync { all: true, .. }
            ));
            let mut current_copies = HashMap::new();
            if matches!(catch_up, CatchUp::Incremental) {
                reply(&mut socket, ServerFrame::SyncDone).await;
                let mut sent_at_old_epoch = HashSet::new();
                while sent_at_old_epoch.len() < 2 {
                    match request(&mut socket).await {
                        ClientFrame::Publish { env }
                            if env.sealed_kind() == Some(SealedKind::Application) =>
                        {
                            assert!(ctx.status.borrow().synced);
                            let (_, bytes) = env.sealed_data().unwrap();
                            assert_eq!(roda_mls::application_epoch(bytes), Some(1));
                            assert!(sent_at_old_epoch.insert(env.client_id().to_string()));
                        }
                        ClientFrame::Req { id, .. } => {
                            reply(
                                &mut socket,
                                ServerFrame::Res {
                                    id,
                                    result: Ok(Reply::Done),
                                },
                            )
                            .await;
                        }
                        ClientFrame::Ping => reply(&mut socket, ServerFrame::Pong).await,
                        ClientFrame::Publish { .. } => {}
                        other => panic!("unexpected {other:?}"),
                    }
                }
                assert!(sent_at_old_epoch.contains(&cached.client_id));
                assert!(sent_at_old_epoch.contains(&fresh.client_id));
                // Live catch-up overtakes the answers to both epoch-1 publications.
                reply(
                    &mut socket,
                    ServerFrame::Event {
                        ev: missed_commit.clone(),
                    },
                )
                .await;
                reply(&mut socket, ServerFrame::SyncDone).await;
                loop {
                    maintenance_started.recv().await.unwrap();
                    let engine = lock(&engine);
                    if [&cached_key, &fresh_key].iter().all(|key| {
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
                current_copies = lock(&engine)
                    .outbox_envelopes()
                    .into_iter()
                    .filter(|env| env.sealed_kind() == Some(SealedKind::Application))
                    .map(|env| (env.client_id().to_string(), env))
                    .collect();
                assert_eq!(current_copies.len(), 2);
                // The cache now contains new ciphertext, but each outstanding answer
                // still belongs to the old envelope actually written to the socket.
                for event in [&cached, &fresh] {
                    reply(
                        &mut socket,
                        ServerFrame::Rejected {
                            space: space.into(),
                            client_id: event.client_id.clone(),
                            reason: roda_proto::STALE_SEAL.into(),
                            permanent: true,
                        },
                    )
                    .await;
                }
            }
            if matches!(catch_up, CatchUp::Initial) {
                // Wait for a real maintenance turn before the FIFO request marker.
                // Its arrival proves the initial flush and explicit Flush have completed;
                // no sleep is used to infer the absence of a publication.
                maintenance_started.recv().await.unwrap();
                cmd.send(Cmd::Flush).unwrap();
                let (marker, marked) = oneshot::channel();
                let marker_box = "11".repeat(32);
                cmd.send(Cmd::Req {
                    op: Op::Lookup {
                        handle: marker_box.clone(),
                        prefix: true,
                    },
                    reply: marker,
                })
                .unwrap();
                loop {
                    match request(&mut socket).await {
                        ClientFrame::Publish { .. } => {
                            panic!("an offline write overtook the initial catch-up")
                        }
                        ClientFrame::Req {
                            id,
                            op:
                                Op::Lookup {
                                    handle,
                                    prefix: true,
                                },
                        } if handle == marker_box => {
                            reply(
                                &mut socket,
                                ServerFrame::Res {
                                    id,
                                    result: Ok(Reply::Profiles(Vec::new())),
                                },
                            )
                            .await;
                            break;
                        }
                        ClientFrame::Ping => reply(&mut socket, ServerFrame::Pong).await,
                        other => panic!("maintenance sent {other:?} before catch-up"),
                    }
                }
                assert!(
                    matches!(marked.await.unwrap(), Ok(Reply::Profiles(list)) if list.is_empty())
                );
                {
                    let engine = lock(&engine);
                    assert_eq!(engine.outbox_len(), 2);
                    assert_eq!(engine.store.meta(&cached_key).unwrap(), Some(cached_copy));
                    assert!(engine.store.meta(&fresh_key).unwrap().is_none());
                    assert!(engine.store.outbox_handshakes().unwrap().is_empty());
                    assert_eq!(engine.mls_status(space).unwrap().0, 1);
                }
                reply(&mut socket, ServerFrame::Event { ev: missed_commit }).await;
                reply(&mut socket, ServerFrame::SyncDone).await;
            }
            let mut readable = HashSet::new();
            while readable.len() < 2 {
                match request(&mut socket).await {
                    ClientFrame::Publish { env }
                        if env.sealed_kind() == Some(SealedKind::Application) =>
                    {
                        assert!(ctx.status.borrow().synced);
                        let original = [&cached, &fresh]
                            .into_iter()
                            .find(|event| event.client_id == env.client_id())
                            .expect("only the two queued messages are published");
                        if matches!(catch_up, CatchUp::Incremental) {
                            assert_eq!(
                                current_copies.get(env.client_id()),
                                Some(&env),
                                "a late rejection must preserve the newer sealed copy"
                            );
                        }
                        let (_, bytes) = env.sealed_data().unwrap();
                        assert_eq!(roda_mls::application_epoch(bytes), Some(2));
                        let Opened::Application { plaintext, .. } =
                            peer.open(space, bytes, &roster).unwrap()
                        else {
                            panic!("the peer must read the message at the caught-up epoch")
                        };
                        let inner = InnerEvent::decode(&plaintext).unwrap();
                        assert_eq!(inner.content, original.content);
                        assert_eq!(inner.sig, original.sig);
                        assert!(readable.insert(env.client_id().to_string()));
                        let ev = sequence(env, &mut head);
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
                        .await;
                    }
                    ClientFrame::Ping => reply(&mut socket, ServerFrame::Pong).await,
                    ClientFrame::Publish { .. } => {}
                    other => panic!("unexpected {other:?}"),
                }
            }
            // A response ordered after both event echoes confirms that the client
            // applied them and cleared their pending records before stopping.
            let (marker, marked) = oneshot::channel();
            let marker_box = "22".repeat(32);
            cmd.send(Cmd::Req {
                op: Op::Lookup {
                    handle: marker_box.clone(),
                    prefix: true,
                },
                reply: marker,
            })
            .unwrap();
            loop {
                match request(&mut socket).await {
                    ClientFrame::Req {
                        id,
                        op:
                            Op::Lookup {
                                handle,
                                prefix: true,
                            },
                    } if handle == marker_box => {
                        reply(
                            &mut socket,
                            ServerFrame::Res {
                                id,
                                result: Ok(Reply::Profiles(Vec::new())),
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
                    ClientFrame::Ping => reply(&mut socket, ServerFrame::Pong).await,
                    ClientFrame::Publish { .. } => {}
                    other => panic!("unexpected {other:?}"),
                }
            }
            assert!(matches!(marked.await.unwrap(), Ok(Reply::Profiles(list)) if list.is_empty()));
            {
                let engine = lock(&engine);
                assert!(!engine.net.pending.contains_key(&cached.client_id));
                assert!(!engine.net.pending.contains_key(&fresh.client_id));
                engine.logs[space].verify().unwrap();
            }
            cmd.send(Cmd::Stop).unwrap();
            stopped.await.unwrap();
        };
        tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(client, relay)
        })
        .await
        .expect("catch-up ordering stalled");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_control_notice_is_dispatched_between_slow_maintenance_passes() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let mut engine = Engine::open(":memory:").unwrap();
        engine.create_account("Bruno", "bruno", &url).unwrap();
        engine.set_registered(true).unwrap();
        // This is a top-up of a device whose initial batch was already acknowledged.
        let _ = engine.mls_key_packages_to_publish().unwrap();
        engine.mls_key_packages_published(Ok(()));
        assert!(engine.mls_settled(), "the fixture has no other MLS work");
        let device = engine.net.account.as_ref().unwrap().device.clone();
        let engine = Arc::new(Mutex::new(engine));
        let (cmd, mut commands) = mpsc::unbounded_channel();
        let (status, _) = watch::channel(NetStatus {
            state: ConnState::Online,
            synced: false,
            error: None,
            registered: true,
        });
        let passes = Arc::new(AtomicUsize::new(0));
        let (maintenance_started, during_maintenance) = channel::channel();
        let (notice_sent, notice_written) = channel::channel();
        let notice_written = Mutex::new(notice_written);
        // An independent executor puts the notice on the real socket while this
        // session's sole worker is busy. No server task can drive its I/O for it.
        let relay = std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async move {
                    let listener = tokio::net::TcpListener::from_std(listener).unwrap();
                    let (socket, _) = listener.accept().await.unwrap();
                    let mut socket = tokio_tungstenite::accept_async(socket).await.unwrap();
                    let ClientFrame::Hello { identity, .. } = request(&mut socket).await else {
                        panic!("hello")
                    };
                    reply(
                        &mut socket,
                        ServerFrame::Challenge {
                            nonce: "maintenance-test".into(),
                            relay: "maintenance-test".into(),
                            protocol: PROTOCOL_VERSION,
                            capabilities: Vec::new(),
                        },
                    )
                    .await;
                    assert!(matches!(
                        request(&mut socket).await,
                        ClientFrame::Auth { .. }
                    ));
                    reply(
                        &mut socket,
                        ServerFrame::Ready {
                            identity,
                            registered: true,
                        },
                    )
                    .await;
                    assert!(matches!(
                        request(&mut socket).await,
                        ClientFrame::Sync { all: true, .. }
                    ));
                    reply(&mut socket, ServerFrame::SyncDone).await;
                    during_maintenance
                        .recv_timeout(Duration::from_secs(5))
                        .unwrap();
                    reply(
                        &mut socket,
                        ServerFrame::KeyPackagesLow {
                            device,
                            remaining: 7,
                        },
                    )
                    .await;
                    notice_sent.send(()).unwrap();
                    loop {
                        match request(&mut socket).await {
                            ClientFrame::Req {
                                id,
                                op:
                                    Op::PublishKeyPackages {
                                        packages,
                                        last_resort,
                                    },
                            } => {
                                assert_eq!(
                                    packages.len(),
                                    25,
                                    "the notice for 7 remaining packages requests the exact top-up to 32"
                                );
                                assert!(
                                    last_resort.is_none(),
                                    "the existing last-resort package is retained"
                                );
                                reply(
                                    &mut socket,
                                    ServerFrame::Res {
                                        id,
                                        result: Ok(Reply::Done),
                                    },
                                )
                                .await;
                                cmd.send(Cmd::Stop).unwrap();
                                return;
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
                            ClientFrame::Ping => reply(&mut socket, ServerFrame::Pong).await,
                            _ => {}
                        }
                    }
                })
        });
        let work_passes = passes.clone();
        let work_engine = engine.clone();
        let ctx = Ctx {
            engine,
            listener: None,
            status,
            lang: crate::i18n::Lang::En,
            maintenance_work: Some(Arc::new(move || {
                let pass = work_passes.fetch_add(1, Ordering::SeqCst) + 1;
                if pass == 1 {
                    return;
                }
                if pass == 2 {
                    maintenance_started.send(()).unwrap();
                    notice_written
                        .lock()
                        .unwrap()
                        .recv_timeout(Duration::from_secs(5))
                        .unwrap();
                }
                if pass == 3 {
                    assert!(
                        !lock(&work_engine).mls_settled(),
                        "the queued low-stock notice must be dispatched before another slow pass"
                    );
                }
                // Synchronous engine work exceeds the 50ms period on every later pass.
                std::thread::sleep(MAINTENANCE_PERIOD * 2);
            })),
        };
        let mut backoff = Duration::from_millis(500);
        let _ = session(&ctx, &mut commands, &mut backoff).await;
        relay.join().unwrap();
    }
}
