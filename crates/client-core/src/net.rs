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
use protocol::{
    auth_message, blob_put_message, ClientFrame, EphemeralKind, ErrorCode, KeyPackageClaimClock,
    Op, Reply, ServerFrame, CAPABILITIES, KEY_PACKAGE_CLAIM_TTL_MS, MIN_PROTOCOL_VERSION,
    PROTOCOL_VERSION,
};
use tokio::sync::{mpsc, oneshot, watch};
use tokio_tungstenite::tungstenite::Message;
use types::Identity;

use crate::engine::Engine;
use crate::sync::{Ingest, RejectionOutcome};
use crate::CoreListener;

mod cursor_repair;
use cursor_repair::{CursorRepair, CursorRepairPolicy};

/// Live fan-out can race a room's membership history during catch-up. Hold only a
/// bounded set of unvalidated pairs on this socket, never in the durable cache.
#[derive(Default)]
pub(crate) struct DeferredReceipts(HashMap<(String, String), (u64, u64)>);

impl DeferredReceipts {
    pub(crate) fn device_message(
        &mut self,
        engine: &mut Engine,
        from: &str,
        to: &str,
        sealed: &[u8],
        dirty: &mut HashSet<String>,
    ) -> Result<(), crate::CoreError> {
        engine.device_message(from, to, sealed);
        // Opened history can release membership held behind a waiting approval.
        self.flush(engine, dirty)
    }

    pub(crate) fn receive(
        &mut self,
        engine: &mut Engine,
        space: &str,
        from: &str,
        delivered: u64,
        read: u64,
    ) -> Result<bool, crate::CoreError> {
        if engine.receipt_scope(space, from)? {
            return engine.receipt(space, from, delivered, read);
        }
        // Explicit native/Web correction: a known signed removal is a cut,
        // not unresolved membership that can authorize this arrival on rejoin.
        if engine.receipt_confirmed_removed(space, from)? {
            self.0.remove(&(space.into(), from.into()));
            return Ok(false);
        }
        if delivered == 0 && read == 0 {
            return Ok(false);
        }
        // Space IDs are bounded to 128 bytes by the push protocol; identity IDs
        // are 64-byte hex. Preserve that upper bound for deferred frame strings.
        let key = (space.to_string(), from.to_string());
        if space.is_empty()
            || space.len() > 128
            || from.is_empty()
            || from.len() > 128
            || (self.0.len() >= 1024 && !self.0.contains_key(&key))
        {
            // The caller reconnects and catches up: overflow never silently
            // discards a potentially legitimate latest watermark.
            return Err(crate::CoreError::Invalid {
                reason: "receipt catch-up buffer exceeded".into(),
            });
        }
        let mark = self.0.entry(key).or_default();
        *mark = (mark.0.max(delivered).max(read), mark.1.max(read));
        Ok(false)
    }

    pub(crate) fn flush(
        &mut self,
        engine: &mut Engine,
        dirty: &mut HashSet<String>,
    ) -> Result<(), crate::CoreError> {
        let mut ready = Vec::new();
        let mut discarded = Vec::new();
        for (space, from) in self.0.keys() {
            if engine.receipt_scope(space, from)? {
                ready.push((space.clone(), from.clone()));
            } else if engine.receipt_confirmed_removed(space, from)? {
                discarded.push((space.clone(), from.clone()));
            }
        }
        for key in discarded {
            self.0.remove(&key);
        }
        for (space, from) in ready {
            let (delivered, read) = self.0[&(space.clone(), from.clone())];
            if engine.receipt(&space, &from, delivered, read)? {
                dirty.insert(space.clone());
            }
            self.0.remove(&(space, from));
        }
        // SyncDone also acknowledges partial gap/join requests, so it cannot
        // prove unrelated membership complete. Keep unresolved pairs bounded
        // until canonical history admits them or this socket is discarded.
        Ok(())
    }
}

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
    authenticated_relay: Option<AuthenticatedRelay>,
}

#[derive(Clone, Debug)]
struct AuthenticatedRelay {
    base: String,
    name: String,
    identity: String,
    device: String,
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
    engine: EngineLease,
}

type Shared = Arc<Mutex<Engine>>;

#[derive(Clone)]
struct EngineLease(Arc<Mutex<Option<Shared>>>);

impl EngineLease {
    fn new(engine: Shared) -> Self {
        Self(Arc::new(Mutex::new(Some(engine))))
    }

    fn with<T>(&self, work: impl FnOnce(&mut Engine) -> T) -> Option<T> {
        let holder = self.0.lock().unwrap_or_else(|p| p.into_inner());
        holder.as_ref().map(|engine| work(&mut lock(engine)))
    }

    fn revoke(&self) {
        let engine = self.0.lock().unwrap_or_else(|p| p.into_inner()).take();
        // Finish releasing ownership here; task cancellation is asynchronous.
        drop(engine);
    }
}

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
            authenticated_relay: None,
        });
        {
            let poke_tx = cmd_tx.clone();
            let mut engine = lock(&engine);
            // stop_sync cancels its runtime; its dispatcher may not reach normal cleanup.
            engine.mls_claims_disconnected();
            engine.net.poke = Some(Box::new(move || {
                let _ = poke_tx.send(Cmd::Flush);
            }));
        }
        let engine = EngineLease::new(engine);
        rt.spawn(crate::i18n::scope(
            lang,
            run(
                Ctx {
                    engine: engine.clone(),
                    listener,
                    status: status_tx,
                    lang,
                    #[cfg(test)]
                    maintenance_work: None,
                },
                cmd_rx,
            ),
        ));
        Ok(Net {
            rt: Some(rt),
            cmd: cmd_tx,
            status: status_rx,
            engine,
        })
    }

    pub fn status(&self) -> NetStatus {
        self.status.borrow().clone()
    }

    /// HTTP signatures share the current socket's authenticated relay domain.
    pub fn authenticated_relay_name(
        &self,
        url: &str,
        identity: &str,
        device: &str,
    ) -> Option<String> {
        let status = self.status.borrow();
        let relay = status.authenticated_relay.as_ref()?;
        (status.state == ConnState::Online
            && relay.base == http_base(url)
            && relay.identity == identity
            && relay.device == device)
            .then(|| relay.name.clone())
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
        self.engine.with(|engine| engine.net.file_demands.clear());
        let _ = self.cmd.send(Cmd::Stop);
        if let Some(rt) = self.rt.take() {
            // Safe from inside another async runtime too.
            rt.shutdown_background();
        }
        self.engine.revoke();
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
    engine: EngineLease,
    listener: Option<Arc<dyn CoreListener>>,
    status: watch::Sender<NetStatus>,
    lang: crate::i18n::Lang,
    #[cfg(test)]
    maintenance_work: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl Ctx {
    fn set(&self, f: impl FnOnce(&mut NetStatus)) {
        self.status.send_modify(|status| {
            f(status);
            if status.state != ConnState::Online {
                status.authenticated_relay = None;
            }
        });
        self.notify_connection();
    }

    fn notify_connection(&self) {
        if let Some(l) = &self.listener {
            let st = self.status.borrow().clone();
            let Some(pending) = self.engine(|engine| engine.outbox_len()) else {
                return;
            };
            l.on_connection(crate::ConnectionDto::from_status(&st, pending));
        }
    }

    fn engine<T>(&self, work: impl FnOnce(&mut Engine) -> T) -> Option<T> {
        crate::i18n::with_lang(self.lang, || self.engine.with(work))
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
                return ServerFrame::decode(&b).map_err(|e| e.to_string());
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
    Claimed {
        space: String,
        operation: String,
    },
    /// Our devices, as the relay lists them.
    Devices,
    /// Nothing to do with the answer (a message to another device of ours).
    Ignored,
}

/// Only the current retry may consume a profile response or settle its deadline.
/// A timed-out request can still reply after the replacement was sent.
fn remember_waiting(waiting: &mut HashMap<u64, Waiting>, id: u64, next: Waiting) {
    match &next {
        Waiting::SealedProfiles(_) => {
            waiting.retain(|_, entry| !matches!(entry, Waiting::SealedProfiles(_)))
        }
        Waiting::ProfileUploaded(_) => {
            waiting.retain(|_, entry| !matches!(entry, Waiting::ProfileUploaded(_)))
        }
        _ => {}
    }
    waiting.insert(id, next);
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
struct ClaimClockSample {
    clock: KeyPackageClaimClock,
    sampled: tokio::time::Instant,
}

impl ClaimClockSample {
    fn new(clock: KeyPackageClaimClock) -> Self {
        Self {
            clock,
            sampled: tokio::time::Instant::now(),
        }
    }

    fn fresh(&self) -> Option<KeyPackageClaimClock> {
        (self.sampled.elapsed() < Duration::from_millis(KEY_PACKAGE_CLAIM_TTL_MS as u64))
            .then_some(self.clock)
    }
}

fn report_rejection(ctx: &Ctx, outcome: &RejectionOutcome) {
    let notice = crate::i18n::with_lang(ctx.lang, || match outcome {
        RejectionOutcome::UserFailed => Some(crate::i18n::t(
            "Não foi possível enviar. Tente novamente.",
            "Could not send. Try again.",
        )),
        RejectionOutcome::StorageFailure(_) => Some(crate::i18n::t(
            "Não foi possível concluir a sincronização. Tente novamente.",
            "Could not finish syncing. Try again.",
        )),
        _ => None,
    });
    if let (Some(listener), Some(notice)) = (&ctx.listener, notice) {
        listener.on_error(notice);
    }
}

const MAINTENANCE_PERIOD: Duration = Duration::from_millis(50);

async fn session(
    ctx: &Ctx,
    cmd_rx: &mut mpsc::UnboundedReceiver<Cmd>,
    backoff: &mut Duration,
) -> Exit {
    session_with_repair_policy(ctx, cmd_rx, backoff, CursorRepairPolicy::DEFAULT).await
}

async fn session_with_repair_policy(
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
        capabilities: CAPABILITIES.iter().map(|c| c.to_string()).collect(),
        identity: identity.clone(),
        device: key.id(),
        cert,
    };
    if let Err(e) = send(&mut sink, &hello).await {
        return Exit::Retry(e);
    }
    let (nonce, relay, profiles, advertised_clock) = match recv(&mut stream).await {
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
    let clock_negotiated = advertised_clock.is_some();
    let mut claim_clock = advertised_clock;
    if !relay_knows_me || !registered {
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
    let cursors = engine!(|engine| engine.cursors());
    if let Err(e) = send(&mut sink, &ClientFrame::Sync { cursors, all: true }).await {
        return Exit::Retry(e);
    }
    let mut cursor_repair = CursorRepair::new(repair_policy);
    // Preserve the actual sent epoch even if maintenance reseals the outbox while
    // this publication is still waiting for the relay's answer.
    let mut sent: HashMap<String, Option<u64>> = HashMap::new();
    if let Err(e) = flush(ctx, &mut sink, &mut sent).await {
        return Exit::Retry(e);
    }

    let mut waiting: HashMap<u64, Waiting> = HashMap::new();
    let mut next_id: u64 = 1;
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

    let (repair_deadline, repair_deadline_rx) = watch::channel(None);
    let (exit, repair_expired) = cursor_repair::drive(async {
    loop {
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
                        let r = engine!(|engine| engine.ingest(ev));
                        if r == Ingest::Confirmed { sent.remove(&client_id); }
                        match r {
                            Ingest::Confirmed => {
                                // Release Welcomes behind commits and descendants behind
                                // an offline genesis as soon as their predecessor lands.
                                dirty.insert(space);
                                media_kick.notify_one();
                                if let Err(e) = flush(ctx, &mut sink, &mut sent).await { break Exit::Retry(e) }
                            }
                            Ingest::Applied => { dirty.insert(space); media_kick.notify_one(); }
                            Ingest::Duplicate => {}
                            Ingest::Gap { next } => {
                                let c = vec![protocol::Cursor { space, next_seq: next }];
                                if let Err(e) = send(&mut sink, &ClientFrame::Sync { cursors: c, all: false }).await { break Exit::Retry(e) }
                                if !cursor_repair.submitted() { break Exit::Retry("too many outstanding sync requests".into()) }
                            }
                            Ingest::Invalid(reason) => {
                                // A relay that serves a bad chain or signature is not trusted for this
                                // Space until a fresh sync; the event isn't stored.
                                tracing_like(&format!("refused event in {space}: {reason}"));
                            }
                        }
                        if let Err(e) = engine!(|engine| deferred_receipts.flush(engine, &mut dirty)) {
                            break Exit::Retry(e.to_string());
                        }
                    }
                    ServerFrame::Accepted { .. } => {}
                    ServerFrame::Rejected { space, client_id, reason, permanent } => {
                        let sent_epoch = sent.remove(&client_id).flatten();
                        let outcome = engine!(|engine| engine.reject(&client_id, &reason, permanent, sent_epoch));
                        tracing_like(&format!("publication {client_id} refused: {reason}; {outcome:?}"));
                        if outcome.user_failed() { dirty.insert(space); }
                        if !permanent {
                            // Transient (rate limit, db hiccup): retry shortly.
                            let poke = engine!(|engine| engine.net.poke.is_some());
                            if poke { tokio::time::sleep(Duration::from_millis(500)).await; }
                        }
                        if matches!(outcome, RejectionOutcome::Retry) {
                            // The relay refuses the old encoding permanently, but the
                            // queued plaintext may already have a current sealed copy.
                            if let Err(e) = flush(ctx, &mut sink, &mut sent).await { break Exit::Retry(e) }
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
                                Ok(true) => { dirty.insert(space.clone()); }
                                Ok(false) => {}
                                Err(e) => break Exit::Retry(e.to_string()),
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
                        if let Some(l) = &ctx.listener { l.on_presence(identity, online); }
                    }
                    ServerFrame::Joined { space } => {
                        let next = engine!(|engine| engine.next_seq(&space));
                        let c = vec![protocol::Cursor { space, next_seq: next }];
                        if let Err(e) = send(&mut sink, &ClientFrame::Sync { cursors: c, all: false }).await { break Exit::Retry(e) }
                        if !cursor_repair.submitted() { break Exit::Retry("too many outstanding sync requests".into()) }
                    }
                    ServerFrame::SyncDone => {
                        cursor_repair.completed(tokio::time::Instant::now());
                        let _ = repair_deadline.send_replace(cursor_repair.abort_deadline());
                        if let Err(e) = engine!(|engine| deferred_receipts.flush(engine, &mut dirty)) {
                            break Exit::Retry(e.to_string());
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
                                if let Err(e) = send(&mut sink, &ClientFrame::Ephemeral { space: space.clone(), kind: kind.clone() }).await { failed = Some(e); break }
                                if let EphemeralKind::Delivered { seq } = kind {
                                    engine!(|engine| engine.delivered_announced(&space, seq));
                                }
                            }
                            if let Some(e) = failed { break Exit::Retry(e) }
                        }
                        if let Err(e) = engine!(|engine| engine.mls_ask_rejoin()) { tracing_like(&format!("rejoin: {e}")); }
                        // Anything still pending after a full catch-up goes out again (it
                        // may have needed the Space to exist first).
                        if let Err(e) = flush(ctx, &mut sink, &mut sent).await { break Exit::Retry(e) }
                        ctx.notify_connection();
                    }
                    ServerFrame::Res { id, result } => {
                        match waiting.remove(&id) {
                            Some(Waiting::External(tx)) => { let _ = tx.send(result); }
                            Some(Waiting::Devices) => {
                                if let Ok(Reply::Devices(list)) = result { engine!(|engine| engine.devices_arrived(list)); }
                                if let Err(e) = flush(ctx, &mut sink, &mut sent).await { break Exit::Retry(e) }
                            }
                            Some(Waiting::Ignored) => {
                                if let Err(e) = result { tracing_like(&format!("device message: {e}")); }
                            }
                            Some(Waiting::Profiles(asked)) => {
                                profiles_inflight = None;
                                engine!(|eng| {
                                    if let Ok(Reply::Profiles(list)) = result {
                                        if !list.is_empty() && eng.put_profiles(list).is_ok() {
                                            dirty.insert(String::new());
                                        }
                                    }
                                    eng.profiles_answered(&asked);
                                });
                            }
                            Some(Waiting::AgreementPublished(public)) => {
                                let ok = matches!(result, Ok(Reply::Done));
                                if ok { engine!(|engine| engine.agreement_published(&public)); } else { tracing_like(&format!("agreement key refused: {result:?}")); }
                                settle(&mut traffic.agreement, ok);
                            }
                            Some(Waiting::ProfileUploaded(version)) => {
                                let ok = matches!(result, Ok(Reply::Done));
                                let answer = result.map(|_| ());
                                engine!(|engine| engine.profile_upload_answered(version, answer));
                                settle(&mut traffic.upload, ok);
                            }
                            Some(Waiting::AgreementKeys(asked)) => {
                                let ok = if let Ok(Reply::AgreementKeys(list)) = result {
                                    engine!(|engine| engine.agreement_keys_arrived(list, &asked));
                                    true
                                } else { false };
                                settle(&mut traffic.keys, ok);
                            }
                            Some(Waiting::SealedProfiles(asked)) => {
                                opening = None;
                                let ok = if let Ok(Reply::SealedProfiles(list)) = result {
                                    let (changed, complete) = engine!(|engine| {
                                        let changed=engine.sealed_profiles_arrived(list,&asked);
                                        let complete=engine.profiles_answer_complete(&asked);
                                        (changed,complete)
                                    });
                                    if !changed.is_empty() {
                                        dirty.insert(String::new());
                                        media_kick.notify_one();
                                        if let Some(l) = &ctx.listener { for id in changed { l.on_profile_changed(id); } }
                                    }
                                    complete
                                } else { false };
                                settle(&mut traffic.profiles, ok);
                            }
                            Some(Waiting::KeyPackagesPublished) => {
                                let ok = matches!(result, Ok(Reply::Done));
                                engine!(|engine| engine.mls_key_packages_published(result.map(|_| ())));
                                settle(&mut traffic.key_packages, ok);
                            }
                            Some(Waiting::Claimed { space, operation }) => {
                                let packages = match result {
                                    Ok(Reply::KeyPackages(list)) => Ok(list),
                                    Ok(other) => Err(format!("unexpected {other:?}")),
                                    Err(e) => Err(e),
                                };
                                engine!(|engine| engine.mls_claimed(&space, &operation, packages));
                                if let Err(e) = flush(ctx, &mut sink, &mut sent).await { break Exit::Retry(e) }
                            }
                            None => {}
                        }
                    }
                    ServerFrame::ProfileChanged { identity, .. } => engine!(|engine| engine.profile_changed(&identity)),
                    ServerFrame::KeyPackagesLow { device, remaining } => engine!(|engine| engine.mls_key_packages_low(&device, remaining)),
                    ServerFrame::DeviceMessage { from, to, sealed } => {
                        if let Err(e) = engine!(|engine| deferred_receipts.device_message(engine, &from, &to, &sealed, &mut dirty)) {
                            break Exit::Retry(e.to_string());
                        }
                        if engine!(|engine| engine.take_initial_history_ready()) {
                            if let Some(listener) = &ctx.listener {
                                listener.on_ephemeral(String::new(), from, "history_ready".to_owned(), String::new());
                            }
                        }
                    },
                    ServerFrame::Pong { server_time_ms } => {
                        if clock_negotiated {
                            claim_clock = server_time_ms.and_then(KeyPackageClaimClock::from_server_ms).map(ClaimClockSample::new);
                        }
                    }
                    // A failed initial/repair Sync has no SyncDone and leaves the socket open.
                    // Reconnect to retry it; the normal run loop bounds retry frequency.
                    ServerFrame::Error { code: ErrorCode::Unavailable, message }
                        if !ctx.status.borrow().synced || cursor_repair.in_flight() => break Exit::Retry(message),
                    ServerFrame::Error { message, .. } => tracing_like(&format!("relay: {message}")),
                    ServerFrame::Challenge { .. } | ServerFrame::Ready { .. } => {}
                }
            }
            cmd = cmd_rx.recv() => match cmd {
                None | Some(Cmd::Stop) => break Exit::Stop,
                Some(Cmd::Flush) => {
                    // A profile edit needs a fresh login to re-register.
                    let needs_register = engine!(|engine| engine.account().map(|a| !a.registered).unwrap_or(false));
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
                    let unknown = engine!(|engine| engine.take_unknown());
                    if !unknown.is_empty() {
                        let id = next_id; next_id += 1;
                        if let Err(e) = send(&mut sink, &ClientFrame::Req { id, op: Op::Profiles { ids: unknown.clone() } }).await { break Exit::Retry(e) }
                        waiting.insert(id, Waiting::Profiles(unknown));
                        profiles_inflight = Some(tokio::time::Instant::now());
                    }
                }
                let shared = engine!(|engine| engine.profile_share_pass());
                match shared {
                    Ok(true) => if let Err(e) = flush(ctx, &mut sink, &mut sent).await { break Exit::Retry(e) },
                    Ok(false) => {}
                    Err(e) => tracing_like(&format!("profile shares: {e}")),
                }
                let mut reqs: Vec<(Op, Waiting)> = Vec::new();
                if free(&mut traffic.agreement) {
                    let publish = engine!(|engine| engine.agreement_to_publish());
                    if let Some((public, signed)) = publish {
                        traffic.agreement = Some(tokio::time::Instant::now() + PROFILE_TIMEOUT);
                        reqs.push((Op::PublishAgreementKey { public: public.clone(), signed }, Waiting::AgreementPublished(public)));
                    }
                }
                if free(&mut traffic.upload) {
                    let upload = engine!(|engine| engine.profile_to_upload());
                    if let Some(profile) = upload {
                        traffic.upload = Some(tokio::time::Instant::now() + PROFILE_TIMEOUT);
                        let version = profile.version;
                        reqs.push((Op::PutProfile { profile }, Waiting::ProfileUploaded(version)));
                    }
                }
                if free(&mut traffic.keys) {
                    let ids = engine!(|engine| engine.take_need_agreement());
                    if !ids.is_empty() {
                        traffic.keys = Some(tokio::time::Instant::now() + PROFILE_TIMEOUT);
                        reqs.push((Op::AgreementKeys { ids: ids.clone() }, Waiting::AgreementKeys(ids)));
                    }
                }
                if free(&mut traffic.profiles) {
                    let ids = engine!(|engine| engine.take_need_profiles());
                    if !ids.is_empty() {
                        traffic.profiles = Some(tokio::time::Instant::now() + PROFILE_TIMEOUT);
                        opening = Some(tokio::time::Instant::now());
                        reqs.push((Op::GetProfiles { ids: ids.clone() }, Waiting::SealedProfiles(ids)));
                    }
                }
                if free(&mut traffic.key_packages) {
                    let publish = engine!(|engine| engine.mls_key_packages_to_publish());
                    if let Some((packages, last_resort)) = publish {
                        traffic.key_packages = Some(tokio::time::Instant::now() + PROFILE_TIMEOUT);
                        reqs.push((Op::PublishKeyPackages { packages, last_resort }, Waiting::KeyPackagesPublished));
                    }
                }
                for (to, sealed) in engine!(|engine| engine.take_device_outbox()) {
                    tracing_like(&format!("device message to {} ({} bytes)", &to[..12.min(to.len())], sealed.len()));
                    reqs.push((Op::SendDevice { to, sealed }, Waiting::Ignored));
                }
                if engine!(|engine| engine.take_ask_devices()) {
                    reqs.push((Op::Devices, Waiting::Devices));
                }
                match engine!(|engine| engine.mls_recovery_pass()) {
                    Ok(true) => media_kick.notify_one(),
                    Ok(false) => {}
                    Err(e) => tracing_like(&format!("group recovery: {e}")),
                }
                let claim = engine!(|engine| engine.mls_to_claim(claim_clock.as_ref().and_then(ClaimClockSample::fresh)));
                match claim {
                    Ok(Some((space, ids, operation))) => reqs.push((
                        Op::ClaimKeyPackages { ids, operation_id: Some(operation.clone()) },
                        Waiting::Claimed { space, operation },
                    )),
                    Ok(None) => {},
                    Err(error) if !claim_refusal_reported => {
                        let message = error.to_string();
                        tracing_like(&message);
                        ctx.set(|s| s.error = Some(message.clone()));
                        if let Some(listener) = &ctx.listener { listener.on_error(message); }
                        claim_refusal_reported = true;
                    }
                    Err(_) => {},
                }
                let checkpoints = engine!(|engine| engine.mls_checkpoints());
                match checkpoints {
                    Ok(true) => if let Err(e) = flush(ctx, &mut sink, &mut sent).await { break Exit::Retry(e) },
                    Ok(false) => {}
                    Err(e) => tracing_like(&format!("checkpoints: {e}")),
                }
                let sealed = engine!(|engine| engine.mls_seal_outbox());
                match sealed {
                    Ok(true) => if let Err(e) = flush(ctx, &mut sink, &mut sent).await { break Exit::Retry(e) },
                    Ok(false) => {}
                    Err(e) => tracing_like(&format!("sealing: {e}")),
                }
                let mut failed = None;
                for (op, w) in reqs {
                    let id = next_id; next_id += 1;
                    if let Err(e) = send(&mut sink, &ClientFrame::Req { id, op }).await { failed = Some(e); break }
                    remember_waiting(&mut waiting,id,w);
                }
                if let Some(e) = failed { break Exit::Retry(e) }
                // Hold UI updates briefly while we learn who new authors are, so the
                // app doesn't flash "Unknown" (or an @handle) for someone about to get a name.
                let hold = |t: &Option<tokio::time::Instant>| t.is_some_and(|t| t.elapsed() < Duration::from_millis(1500));
                let naming = hold(&profiles_inflight) || hold(&opening);
                if !dirty.is_empty() && !naming {
                    let spaces: Vec<String> = dirty.drain().filter(|s| !s.is_empty()).collect();
                    // ✓✓ for the senders: one aggregated watermark per Space whose head moved.
                    let marks: Vec<(String, u64)> = engine!(|engine| spaces.iter().filter_map(|s| engine.delivered_to_announce(s).map(|q| (s.clone(), q))).collect());
                    for (space, seq) in marks {
                        if let Err(e) = send(&mut sink, &ClientFrame::Ephemeral { space: space.clone(), kind: EphemeralKind::Delivered { seq } }).await { failed = Some(e); break }
                        engine!(|engine| engine.delivered_announced(&space, seq));
                    }
                    if let Some(e) = failed { break Exit::Retry(e) }
                    if let Some(l) = &ctx.listener { l.on_change(spaces); }
                    ctx.notify_connection();
                }
                // A slow pass must leave time to drive the socket, rather than build a
                // burst of overdue scans over the same pending work.
                tick.as_mut().reset(tokio::time::Instant::now() + MAINTENANCE_PERIOD);
            }
            _ = async {
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
    engine!(|engine| engine.mls_claims_disconnected());
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
    cursor_repair::close_transport(
        sink.close(),
        cursor_repair.abort_deadline(),
        repair_abandoned || matches!(&exit, Exit::Stop),
    )
    .await;
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
    let envs = ctx
        .engine(|engine| engine.outbox_envelopes_except(sent))
        .ok_or_else(|| "offline".to_string())?;
    for env in envs {
        if let std::collections::hash_map::Entry::Vacant(entry) =
            sent.entry(env.client_id().to_string())
        {
            let epoch = env.sealed_data().and_then(|(kind, data)| match kind {
                event_log::content::SealedKind::Application => ::mls::application_epoch(data),
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

#[derive(Default)]
struct RecoveryRetries {
    by_space: HashMap<String, (crate::mls_recovery::ContextRef, tokio::time::Instant)>,
}

impl RecoveryRetries {
    fn next(
        &mut self,
        wanted: Vec<crate::mls_recovery::ContextRef>,
        now: tokio::time::Instant,
    ) -> Option<crate::mls_recovery::ContextRef> {
        let current: HashMap<_, _> = wanted.iter().map(|r| (r.space.as_str(), r)).collect();
        self.by_space.retain(|space, (reference, _)| {
            current.get(space.as_str()).is_some_and(|r| *r == reference)
        });
        wanted
            .into_iter()
            .find(|r| !self.by_space.get(&r.space).is_some_and(|(_, at)| *at > now))
    }

    fn defer(&mut self, reference: crate::mls_recovery::ContextRef, now: tokio::time::Instant) {
        self.by_space.insert(
            reference.space.clone(),
            (reference, now + Duration::from_secs(5)),
        );
    }

    fn complete(&mut self, reference: &crate::mls_recovery::ContextRef) {
        if self
            .by_space
            .get(&reference.space)
            .is_some_and(|(r, _)| r == reference)
        {
            self.by_space.remove(&reference.space);
        }
    }
}

/// Uploads queued encrypted copies and downloads the ones the chats need, retrying with
/// backoff (a peer's event can arrive before their upload finishes).
async fn media_worker(
    engine: EngineLease,
    listener: Option<Arc<dyn CoreListener>>,
    base: String,
    relay: String,
    key: event_log::Signer,
    kick: Arc<tokio::sync::Notify>,
) {
    macro_rules! engine {
        ($work:expr) => {
            match engine.with($work) {
                Some(value) => value,
                None => return,
            }
        };
    }

    let Ok(http) = crate::https::client()
        .timeout(Duration::from_secs(60))
        .build()
    else {
        tracing_like("media: no HTTP client");
        return;
    };
    let mut retry_at: HashMap<String, (tokio::time::Instant, u32)> = HashMap::new();
    let mut recovery_retries = RecoveryRetries::default();
    loop {
        // ── up ──
        // sync_idle waits until this queue is empty, so a full batch is drained
        // before the download pass and the idle sleep.
        let uploads = engine!(|engine| engine.pending_uploads(8));
        let batch = uploads.len();
        let mut upload_failed = false;
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
                Ok(r) if r.status().is_success() => engine!(|engine| engine.upload_done(&sha)),
                Ok(r) if matches!(r.status().as_u16(), 400 | 413) => {
                    // The relay will never take it (corrupt or too big): don't loop forever.
                    tracing_like(&format!(
                        "media: upload {} refused ({})",
                        &sha[..12],
                        r.status()
                    ));
                    engine!(|engine| engine.upload_refused(&sha));
                }
                Ok(r) => {
                    tracing_like(&format!(
                        "media: upload {} failed ({})",
                        &sha[..12],
                        r.status()
                    ));
                    upload_failed = true;
                    break;
                }
                Err(e) => {
                    tracing_like(&format!("media: upload failed: {e}"));
                    upload_failed = true;
                    break;
                }
            }
        }
        if batch == 8 && !upload_failed {
            continue;
        }

        // Recovery uses a separate bounded download. Only ciphertext is cached, and
        // its content address must match the signed commit before MLS sees any bytes.
        let wanted = engine!(|engine| engine.wanted_recovery_contexts());
        if let Some(reference) = recovery_retries.next(wanted, tokio::time::Instant::now()) {
            let bytes = bounded_blob(
                &http,
                &format!("{base}/v1/blobs/{}", reference.blob),
                event_log::recovery::MAX_RECOVERY_BLOB_BYTES,
            )
            .await;
            if match bytes {
                Some(bytes) => {
                    engine!(|engine| engine.recovery_context_arrived(&reference, &bytes))
                }
                None => false,
            } {
                recovery_retries.complete(&reference);
            } else {
                recovery_retries.defer(reference, tokio::time::Instant::now());
            }
        }

        // ── down ──
        let wanted = engine!(|engine| engine.wanted_media());
        let now = tokio::time::Instant::now();
        let mut changed: Vec<String> = Vec::new();
        for w in wanted {
            // A canceled historical open or changed membership must not start
            // another fetch from a stale snapshot of the demand list.
            if !engine!(|engine| engine.wanted_media().contains(&w)) {
                continue;
            }
            if retry_at.get(&w.blob).is_some_and(|(t, _)| *t > now) {
                continue;
            }
            let url = format!("{base}/v1/blobs/{}", w.blob);
            let got = if let Some(limit) = w.maximum_sealed_bytes {
                bounded_blob(&http, &url, usize::try_from(limit).unwrap_or(0)).await
            } else {
                match http.get(url).send().await {
                    Ok(r) if r.status().is_success() => r.bytes().await.ok().map(|b| b.to_vec()),
                    _ => None,
                }
            };
            let ok =
                match got {
                    Some(bytes) => engine!(|engine| engine.wanted_media().contains(&w)
                        && engine.media_arrived(&w, &bytes)),
                    None => false,
                };
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

async fn bounded_blob(http: &reqwest::Client, url: &str, limit: usize) -> Option<Vec<u8>> {
    let mut response = http.get(url).send().await.ok()?;
    if !response.status().is_success()
        || response.content_length().is_some_and(|n| n > limit as u64)
    {
        return None;
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.ok()? {
        if chunk.len() > limit.saturating_sub(bytes.len()) {
            return None;
        }
        bytes.extend_from_slice(&chunk);
    }
    (!bytes.is_empty()).then_some(bytes)
}

pub(crate) fn tracing_like(msg: &str) {
    if std::env::var_os("ZOEN_NET_DEBUG").is_some() {
        eprintln!("[zoen-net] {msg}");
    }
}

/// Waits for a link box as a device with no account yet (ADR 0045): it signs in as itself
/// (its device key as identity, self-certified), which lets it fetch and nothing else.
pub(crate) async fn fetch_link_box(
    relay_url: &str,
    key: &impl crate::link_pickup::PickupSigning,
    id: &str,
    deadline: tokio::time::Instant,
) -> Result<Vec<u8>, String> {
    let expired = || "O prazo para conectar este aparelho acabou. Gere outro código.".to_string();
    if tokio::time::Instant::now() >= deadline {
        return Err(expired());
    }
    let pickup = async {
        loop {
            key.check()?;
            match fetch_link_attempt(relay_url, key, id).await {
                Ok(bytes) => return Ok(bytes),
                Err(PickupError::Fatal(message)) => return Err(message),
                Err(PickupError::Transport) => {
                    tokio::time::sleep(Duration::from_millis(700)).await;
                }
            }
        }
    };
    // One deadline owns connect, authentication, every receive/send and backoff.
    // Dropping this future also drops the socket and cancels all pickup work.
    let result = tokio::time::timeout_at(deadline, pickup)
        .await
        .map_err(|_| expired())?;
    if tokio::time::Instant::now() >= deadline {
        return Err(expired());
    }
    result
}

enum PickupError {
    Transport,
    Fatal(String),
}

impl From<String> for PickupError {
    fn from(_: String) -> Self {
        Self::Transport
    }
}

async fn fetch_link_attempt(
    relay_url: &str,
    key: &impl crate::link_pickup::PickupSigning,
    id: &str,
) -> Result<Vec<u8>, PickupError> {
    key.check().map_err(PickupError::Fatal)?;
    let url = ws_url(relay_url);
    let (ws, _) = tokio::time::timeout(
        Duration::from_secs(10),
        tokio_tungstenite::connect_async_with_config(url.as_str(), None, true),
    )
    .await
    .map_err(|_| PickupError::Transport)?
    .map_err(|_| PickupError::Transport)?;
    let (mut sink, mut stream) = ws.split();
    let me = key.id().map_err(PickupError::Fatal)?;
    send(
        &mut sink,
        &ClientFrame::Hello {
            protocol: PROTOCOL_VERSION,
            capabilities: CAPABILITIES.iter().map(|c| c.to_string()).collect(),
            identity: me.clone(),
            device: me.clone(),
            cert: key
                .sign(&event_log::device_cert_message(&me))
                .map_err(PickupError::Fatal)?,
        },
    )
    .await?;
    let (nonce, relay) = match recv(&mut stream).await? {
        ServerFrame::Challenge {
            nonce,
            relay,
            protocol,
            ..
        } if protocol >= MIN_PROTOCOL_VERSION => (nonce, relay),
        ServerFrame::Challenge { protocol, .. } => {
            return Err(PickupError::Fatal(format!(
                "O servidor precisa ser atualizado antes de conectar (versão {protocol})."
            )));
        }
        ServerFrame::Error { message, .. } => return Err(PickupError::Fatal(message)),
        _ => {
            return Err(PickupError::Fatal(
                "Resposta inesperada ao conectar.".into(),
            ));
        }
    };
    send(
        &mut sink,
        &ClientFrame::Auth {
            sig: key
                .sign(&auth_message(&nonce, &relay))
                .map_err(PickupError::Fatal)?,
        },
    )
    .await?;
    loop {
        match recv(&mut stream).await? {
            ServerFrame::Ready { .. } => break,
            ServerFrame::Error { message, .. } => return Err(PickupError::Fatal(message)),
            _ => continue,
        }
    }
    let mut n = 0u64;
    loop {
        n += 1;
        key.check().map_err(PickupError::Fatal)?;
        let op = Op::FetchLink { id: id.to_string() };
        send(&mut sink, &ClientFrame::Req { id: n, op }).await?;
        let result = loop {
            match recv(&mut stream).await? {
                ServerFrame::Res { id, result } if id == n => break result,
                ServerFrame::Error { message, .. } => return Err(PickupError::Fatal(message)),
                _ => continue,
            }
        };
        match result {
            Ok(Reply::Link(Some(b))) => return Ok(b),
            Ok(Reply::Link(None)) => {}
            Ok(_) => {
                return Err(PickupError::Fatal(
                    "Resposta inesperada ao conectar.".into(),
                ));
            }
            Err(e) => return Err(PickupError::Fatal(e)),
        }
        tokio::time::sleep(Duration::from_millis(700)).await;
    }
}

#[cfg(test)]
mod tests {
    mod cursor_repair;
    mod incoming_pipeline;

    #[tokio::test]
    async fn pickup_expired_deadline_opens_no_connection() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let result = fetch_link_box(
            &format!("http://{}", listener.local_addr().unwrap()),
            &Signer::generate(),
            "fixture",
            tokio::time::Instant::now(),
        )
        .await;
        assert!(result.unwrap_err().contains("prazo"));
        assert!(
            tokio::time::timeout(Duration::from_millis(80), listener.accept())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn pickup_deadline_bounds_a_stalled_connection() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let deadline = tokio::time::Instant::now() + Duration::from_millis(80);
        let result = fetch_link_box(
            &format!("http://{address}"),
            &Signer::generate(),
            "fixture",
            deadline,
        )
        .await;
        assert!(result.unwrap_err().contains("prazo"));
        assert!(tokio::time::Instant::now() < deadline + Duration::from_millis(500));
    }

    #[tokio::test]
    async fn pickup_reconnects_after_transport_failure_and_is_cancelable() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            fetch_link_box(
                &format!("http://{address}"),
                &Signer::generate(),
                "fixture",
                tokio::time::Instant::now() + Duration::from_secs(5),
            )
            .await
        });
        let (first, _) = listener.accept().await.unwrap();
        drop(first);
        let (second, _) = tokio::time::timeout(Duration::from_secs(2), listener.accept())
            .await
            .unwrap()
            .unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        drop(second);
        assert!(
            tokio::time::timeout(Duration::from_millis(900), listener.accept())
                .await
                .is_err()
        );
    }

    #[test]
    fn an_unrefreshed_receipt_clock_expires_without_advancing_its_timestamp() {
        let clock = protocol::KeyPackageClaimClock::from_server_ms(1_800_000_000_000).unwrap();
        let mut sample = super::ClaimClockSample::new(clock);
        sample.sampled -=
            std::time::Duration::from_millis(protocol::KEY_PACKAGE_CLAIM_TTL_MS as u64 / 2);
        assert_eq!(sample.fresh(), Some(clock));
        sample.sampled -=
            std::time::Duration::from_millis(protocol::KEY_PACKAGE_CLAIM_TTL_MS as u64 / 2 + 1_000);
        assert_eq!(sample.fresh(), None);
    }

    use super::*;
    use ::mls::{sealed::state_key, Device, Opened, SUITE_ID};
    use event_log::{
        chain_hash, content::InnerEvent, content::Sealed, content::SealedKind, Author, Signer,
    };
    use protocol::{Envelope, Sequenced};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc as channel,
    };
    use tokio_tungstenite::WebSocketStream;
    use types::{EventBody, Privacy, Role, Seen, SpaceKind, GENESIS_PREV};

    type RelaySocket = WebSocketStream<tokio::net::TcpStream>;

    #[test]
    fn recovery_download_retries_follow_current_references_and_discard_obsolete_failures() {
        let mut retries = RecoveryRetries::default();
        let now = tokio::time::Instant::now();
        let mut reference = crate::mls_recovery::ContextRef {
            space: "changing-context".into(),
            epoch: 0,
            blob: "0".repeat(64),
            device: "device".into(),
        };
        for epoch in 0..10_000 {
            reference.epoch = epoch;
            reference.blob = format!("{epoch:064x}");
            assert_eq!(
                retries.next(vec![reference.clone()], now),
                Some(reference.clone())
            );
            retries.defer(reference.clone(), now);
            assert_eq!(retries.next(vec![reference.clone()], now), None);
            assert_eq!(retries.by_space.len(), 1);
        }
        assert_eq!(
            retries.next(vec![reference.clone()], now + Duration::from_secs(5)),
            Some(reference.clone())
        );
        let other = crate::mls_recovery::ContextRef {
            space: "another-space".into(),
            ..reference.clone()
        };
        retries.defer(other.clone(), now);
        assert_eq!(retries.next(vec![other.clone()], now), None);
        assert_eq!(retries.by_space.len(), 1, "obsolete Space was removed");
        retries.complete(&reference);
        assert_eq!(
            retries.by_space.len(),
            1,
            "a stale completion cannot clear another Space's backoff"
        );
        retries.complete(&other);
        assert!(retries.by_space.is_empty());
        retries.defer(other, now);
        assert!(retries.next(Vec::new(), now).is_none());
        assert!(
            retries.by_space.is_empty(),
            "cached, removed or restored groups need no retry state"
        );
    }

    #[test]
    fn engine_lease_revocation_waits_for_current_work_and_finishes_storage_release() {
        let path = std::env::temp_dir().join(types::new_id("lease-revoke"));
        let path = path.to_str().unwrap();
        let lease = EngineLease::new(Arc::new(Mutex::new(Engine::open(path).unwrap())));
        let (entered, active) = channel::channel();
        let (release, released) = channel::channel();
        let worker_lease = lease.clone();
        let worker = std::thread::spawn(move || {
            worker_lease
                .with(|engine| {
                    engine.store.set_meta("lease-proof", "committed").unwrap();
                    entered.send(()).unwrap();
                    released.recv_timeout(Duration::from_secs(5)).unwrap();
                })
                .unwrap();
        });
        active.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(matches!(
            lease.0.try_lock(),
            Err(std::sync::TryLockError::WouldBlock)
        ));
        let (revoking, began) = channel::channel();
        let (completed, done) = channel::channel();
        let revoke_lease = lease.clone();
        let revoker = std::thread::spawn(move || {
            revoking.send(()).unwrap();
            revoke_lease.revoke();
            completed.send(()).unwrap();
        });
        began.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(matches!(done.try_recv(), Err(channel::TryRecvError::Empty)));
        release.send(()).unwrap();
        done.recv_timeout(Duration::from_secs(5)).unwrap();
        let reopened = Engine::open(path).unwrap();
        assert_eq!(
            reopened.store.meta("lease-proof").unwrap().as_deref(),
            Some("committed")
        );
        assert!(lease.with(|_| ()).is_none());
        worker.join().unwrap();
        revoker.join().unwrap();
        drop(reopened);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn poisoned_engine_lease_still_revokes_and_releases_storage() {
        let path = std::env::temp_dir().join(types::new_id("lease-poison"));
        let path = path.to_str().unwrap();
        let lease = EngineLease::new(Arc::new(Mutex::new(Engine::open(path).unwrap())));
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            lease.with(|engine| {
                engine.store.set_meta("poison-proof", "committed").unwrap();
                panic!("controlled Engine closure panic");
            });
        }));
        assert!(panic.is_err());
        assert_eq!(
            lease
                .with(|engine| engine.store.meta("poison-proof").unwrap())
                .flatten()
                .as_deref(),
            Some("committed")
        );
        lease.revoke();
        let reopened = Engine::open(path).unwrap();
        assert_eq!(
            reopened.store.meta("poison-proof").unwrap().as_deref(),
            Some("committed")
        );
        drop(reopened);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn blocked_media_upload_releases_storage_and_preserves_ciphertext() {
        use std::io::Read;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let path = std::env::temp_dir().join(types::new_id("media-stop"));
        let path = path.to_str().unwrap();
        let mut engine = Engine::open(path).unwrap();
        engine
            .create_account("Media stop", "media_stop", &url)
            .unwrap();
        engine.set_registered(true).unwrap();
        let media = engine
            .put_media(b"actual attachment", "image/png", 1, 1)
            .unwrap();
        let mut media = types::MediaRef {
            sha256: media.sha256,
            mime: media.mime,
            width: media.width,
            height: media.height,
            bytes: media.bytes,
            key: None,
            blob: None,
        };
        engine.seal_for_relay(&mut media).unwrap();
        let queued = engine.pending_uploads(8);
        assert_eq!(queued.len(), 1);
        let (started, uploading) = channel::channel();
        let (release, released) = channel::channel();
        let relay = std::thread::spawn(move || {
            let (socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut socket = tokio_tungstenite::tungstenite::accept(socket).unwrap();
            let Message::Binary(hello) = socket.read().unwrap() else {
                panic!("hello")
            };
            let ClientFrame::Hello {
                identity, device, ..
            } = ClientFrame::decode(&hello).unwrap()
            else {
                panic!("hello")
            };
            let nonce = "lease-media-nonce";
            let relay_name = "lease-media-local-relay";
            socket
                .send(Message::Binary(
                    ServerFrame::Challenge {
                        nonce: nonce.into(),
                        relay: relay_name.into(),
                        protocol: PROTOCOL_VERSION,
                        capabilities: Vec::new(),
                        server_time_ms: None,
                    }
                    .encode()
                    .into(),
                ))
                .unwrap();
            let Message::Binary(auth) = socket.read().unwrap() else {
                panic!("auth")
            };
            let ClientFrame::Auth { sig } = ClientFrame::decode(&auth).unwrap() else {
                panic!("auth")
            };
            assert!(event_log::verify_sig(
                &device,
                &auth_message(nonce, relay_name),
                &sig
            ));
            socket
                .send(Message::Binary(
                    ServerFrame::Ready {
                        identity,
                        registered: true,
                    }
                    .encode()
                    .into(),
                ))
                .unwrap();
            let (mut http, _) = listener.accept().unwrap();
            http.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut headers = Vec::new();
            let mut byte = [0u8; 1];
            while !headers.ends_with(b"\r\n\r\n") {
                http.read_exact(&mut byte).unwrap();
                headers.push(byte[0]);
                assert!(headers.len() < 16 * 1024);
            }
            let headers = String::from_utf8(headers).unwrap();
            assert!(headers.starts_with("PUT /v1/blobs/"));
            assert!(headers.contains(&format!("x-zoen-device: {device}")));
            started.send(()).unwrap();
            released.recv_timeout(Duration::from_secs(5)).unwrap();
        });
        let net = Net::start(Arc::new(Mutex::new(engine)), None, crate::i18n::Lang::En).unwrap();
        uploading.recv_timeout(Duration::from_secs(5)).unwrap();
        let began = std::time::Instant::now();
        drop(net);
        let reopened = Engine::open(path).unwrap();
        eprintln!(
            "blocked_media stop_reopen_us={}",
            began.elapsed().as_micros()
        );
        assert_eq!(reopened.pending_uploads(8), queued);
        assert!(reopened.account().is_some());
        release.send(()).unwrap();
        relay.join().unwrap();
        drop(reopened);
        std::fs::remove_file(path).unwrap();
    }

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

    /// An authenticated socket that is waiting for the initial catch-up result.
    async fn catch_up_socket(listener: &tokio::net::TcpListener) -> RelaySocket {
        let (socket, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(socket).await.unwrap();
        let ClientFrame::Hello { identity, .. } = request(&mut socket).await else {
            panic!("hello")
        };
        reply(
            &mut socket,
            ServerFrame::Challenge {
                nonce: "catch-up-retry".into(),
                relay: "catch-up-retry".into(),
                protocol: PROTOCOL_VERSION,
                capabilities: Vec::new(),
                server_time_ms: None,
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
        socket
    }

    #[tokio::test(flavor = "current_thread")]
    async fn live_event_overtaking_final_replay_page_reconciles_once_without_later_message() {
        fn assert_history(engine: &Engine, space: &str) {
            let messages: Vec<_> = engine
                .timeline(space)
                .unwrap()
                .into_iter()
                .filter_map(|entry| match entry.kind {
                    crate::EntryKind::Message { text, .. } => Some((entry.id, entry.seq, text)),
                    _ => None,
                })
                .collect();
            assert_eq!(
                messages,
                vec![
                    (
                        "replay-event".into(),
                        2,
                        "Already in the replay snapshot".into()
                    ),
                    (
                        "boundary-event".into(),
                        3,
                        "Committed at the replay/live boundary".into()
                    ),
                ]
            );
            let events = engine.store.events(space).unwrap();
            assert_eq!(
                events
                    .iter()
                    .map(|event| event.client_id.as_str())
                    .collect::<Vec<_>>(),
                ["created", "add-peer", "replay-event", "boundary-event"]
            );
            assert_eq!(engine.next_seq(space), 4);
        }

        struct Directory(std::path::PathBuf);
        impl Drop for Directory {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _ = rustls::crypto::ring::default_provider().install_default();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let directory = Directory(
            std::env::temp_dir().join(format!("zoen-replay-handoff-{}", Signer::generate().id())),
        );
        std::fs::create_dir(&directory.0).unwrap();
        let reader_path = directory.0.join("reader.sqlite");
        let relay_path = directory.0.join("canonical.sqlite");
        let mut engine = Engine::open(reader_path.to_str().unwrap()).unwrap();
        let (_, device_secret, _) = engine.create_account("Ana", "ana", &url).unwrap();
        engine.set_registered(true).unwrap();
        let _ = engine.mls_key_packages_to_publish().unwrap();
        engine.mls_key_packages_published(Ok(()));
        let author = engine.net.author.clone().unwrap();
        let peer = Author::root(Signer::generate());
        let space = "sp_replay_handoff";
        let mut head = None;
        let canonical = store::Store::open(relay_path.to_str().unwrap()).unwrap();
        for (id, body) in [
            (
                "created",
                EventBody::SpaceCreated {
                    title: "Replay handoff".into(),
                    kind: SpaceKind::Group,
                    privacy: Privacy::Closed,
                },
            ),
            (
                "add-peer",
                EventBody::MemberAdded {
                    identity: peer.identity.clone(),
                    role: Role::Member,
                },
            ),
        ] {
            let ev = sequence(
                Envelope::plain(&author.sign_event(space, id, 1, head.clone(), body)),
                &mut head,
            );
            canonical.append_event(&ev.plain_event().unwrap()).unwrap();
            assert_eq!(engine.ingest(ev), Ingest::Applied);
        }
        let replay = sequence(
            Envelope::plain(&peer.sign_event(
                space,
                "replay-event",
                2,
                head.clone(),
                EventBody::MessagePosted {
                    message: "replay-message".into(),
                    text: "Already in the replay snapshot".into(),
                    attaches: None,
                    reply: None,
                    media: None,
                    forwarded: false,
                },
            )),
            &mut head,
        );
        canonical
            .append_event(&replay.plain_event().unwrap())
            .unwrap();
        // This immutable last replay page excludes the event that will commit at the cut.
        let replay_snapshot = canonical.events(space).unwrap();
        assert_eq!(replay_snapshot.len(), 3);
        let boundary = sequence(
            Envelope::plain(&peer.sign_event(
                space,
                "boundary-event",
                3,
                head.clone(),
                EventBody::MessagePosted {
                    message: "boundary-message".into(),
                    text: "Committed at the replay/live boundary".into(),
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
        let (finished, close_socket) = oneshot::channel();
        let relay = async {
            let mut socket = catch_up_socket(&listener).await;
            // Persist the concurrent event before its live delivery, after the replay cut.
            canonical
                .append_event(&boundary.plain_event().unwrap())
                .unwrap();
            drop(canonical);
            let canonical = store::Store::open(relay_path.to_str().unwrap()).unwrap();
            assert_eq!(canonical.events(space).unwrap().len(), 4);
            // Live fanout can overtake an older replay page; no later content wakes repair.
            reply(
                &mut socket,
                ServerFrame::Event {
                    ev: boundary.clone(),
                },
            )
            .await;
            let gap_request = tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    match request(&mut socket).await {
                        ClientFrame::Sync { cursors, all } => break (cursors, all),
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
                        other => panic!("unexpected before gap repair: {other:?}"),
                    }
                }
            })
            .await
            .expect("overtaking live event did not request its missing replay cursor");
            assert!(!gap_request.1);
            assert_eq!(gap_request.0.len(), 1);
            assert_eq!(gap_request.0[0].space, space);
            assert_eq!(gap_request.0[0].next_seq, 2);
            assert!(
                !ctx.status.borrow().synced,
                "repair starts before initial SyncDone"
            );
            reply(&mut socket, ServerFrame::Event { ev: replay }).await;
            reply(&mut socket, ServerFrame::SyncDone).await;
            // The real worker requested cursor2; replay from retained canonical storage.
            for event in canonical
                .events(space)
                .unwrap()
                .into_iter()
                .filter(|event| event.seq >= 2)
            {
                let ev = Sequenced {
                    seq: event.seq,
                    prev: event.prev.clone(),
                    hash: event.hash.clone(),
                    env: Envelope::plain(&event),
                };
                reply(&mut socket, ServerFrame::Event { ev }).await;
            }
            reply(&mut socket, ServerFrame::SyncDone).await;
            // An ordinary request/response is an ordered wire barrier after all event
            // frames; it introduces no message that could conceal a missing wake.
            let (marker, marked) = oneshot::channel();
            cmd.send(Cmd::Req {
                op: Op::FetchLink {
                    id: "handoff-barrier".into(),
                },
                reply: marker,
            })
            .unwrap();
            loop {
                match request(&mut socket).await {
                    ClientFrame::Req {
                        id,
                        op: Op::FetchLink { id: link },
                    } if link == "handoff-barrier" => {
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
                    ClientFrame::Ephemeral { .. } => {}
                    ClientFrame::Ping => {
                        reply(
                            &mut socket,
                            ServerFrame::Pong {
                                server_time_ms: None,
                            },
                        )
                        .await
                    }
                    other => panic!("unexpected after gap repair: {other:?}"),
                }
            }
            assert!(matches!(marked.await.unwrap(), Ok(Reply::Link(None))));
            assert!(ctx.status.borrow().synced);
            assert_history(&lock(&engine), space);
            cmd.send(Cmd::Stop).unwrap();
            close_socket.await.unwrap();
        };
        let mut backoff = Duration::from_millis(500);
        let client = async {
            let exit = session(&ctx, &mut commands, &mut backoff).await;
            finished.send(()).unwrap();
            exit
        };
        let (exit, ()) = tokio::time::timeout(Duration::from_secs(15), async {
            tokio::join!(client, relay)
        })
        .await
        .expect("replay/live handoff did not finish");
        assert!(matches!(exit, Exit::Stop));
        // Aborting the spawned media task does not synchronously drop its lease.
        ctx.engine.revoke();
        assert_eq!(Arc::strong_count(&engine), 1);
        drop(ctx);
        drop(engine);
        let mut reopened = Engine::open(reader_path.to_str().unwrap()).unwrap();
        assert!(reopened.unlock(Some(device_secret.to_vec())).unwrap());
        assert_history(&reopened, space);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn initial_catch_up_unavailable_retries_without_resetting_backoff() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let mut engine = Engine::open(":memory:").unwrap();
        engine.create_account("Ana", "ana", &url).unwrap();
        engine.set_registered(true).unwrap();
        let _ = engine.mls_key_packages_to_publish().unwrap();
        engine.mls_key_packages_published(Ok(()));
        let (cmd, mut commands) = mpsc::unbounded_channel();
        let (status, mut observed) = watch::channel(NetStatus {
            state: ConnState::Connecting,
            synced: false,
            error: None,
            registered: true,
            authenticated_relay: None,
        });
        let ctx = Ctx {
            engine: EngineLease::new(Arc::new(Mutex::new(engine))),
            listener: None,
            status,
            lang: crate::i18n::Lang::En,
            maintenance_work: None,
        };
        let (finished, shutdown) = oneshot::channel();
        let relay = async {
            let mut failed = catch_up_socket(&listener).await;
            reply(
                &mut failed,
                ServerFrame::error(ErrorCode::Unavailable, "database unavailable"),
            )
            .await;
            // Keep the failed socket open, exactly as the real relay does. A client
            // that only logs the error will never reach this second connection.
            let mut recovered = catch_up_socket(&listener).await;
            reply(&mut recovered, ServerFrame::SyncDone).await;
            loop {
                match request(&mut recovered).await {
                    ClientFrame::Req {
                        id,
                        op: Op::Profiles { ids },
                    } if ids.len() == 1 && ids[0] == "catch-up-probe" => {
                        // After catch-up this unrelated error keeps its existing
                        // behavior: it must not abort an in-flight request.
                        reply(
                            &mut recovered,
                            ServerFrame::error(ErrorCode::Unavailable, "database unavailable"),
                        )
                        .await;
                        reply(
                            &mut recovered,
                            ServerFrame::Res {
                                id,
                                result: Ok(Reply::Done),
                            },
                        )
                        .await;
                        break;
                    }
                    ClientFrame::Req { id, .. } => {
                        reply(
                            &mut recovered,
                            ServerFrame::Res {
                                id,
                                result: Ok(Reply::Done),
                            },
                        )
                        .await;
                    }
                    ClientFrame::Ping => {
                        reply(
                            &mut recovered,
                            ServerFrame::Pong {
                                server_time_ms: None,
                            },
                        )
                        .await;
                    }
                    _ => {}
                }
            }
            shutdown.await.unwrap();
            drop((failed, recovered));
        };
        let client = async {
            let mut backoff = Duration::from_secs(4);
            assert!(matches!(
                session(&ctx, &mut commands, &mut backoff).await,
                Exit::Retry(message) if message == "database unavailable"
            ));
            assert!(!ctx.status.borrow().synced);
            assert_eq!(
                backoff,
                Duration::from_secs(4),
                "Ready must not erase retries"
            );
            let exercise = async {
                while !observed.borrow().synced {
                    observed.changed().await.unwrap();
                }
                let (reply, answer) = oneshot::channel();
                cmd.send(Cmd::Req {
                    op: Op::Profiles {
                        ids: vec!["catch-up-probe".into()],
                    },
                    reply,
                })
                .unwrap();
                assert!(matches!(answer.await, Ok(Ok(Reply::Done))));
                cmd.send(Cmd::Stop).unwrap();
            };
            let (exit, ()) = tokio::join!(session(&ctx, &mut commands, &mut backoff), exercise,);
            assert!(matches!(exit, Exit::Stop));
            assert_eq!(backoff, Duration::from_millis(500));
            finished.send(()).unwrap();
        };
        tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(client, relay)
        })
        .await
        .expect("an unavailable initial catch-up must retry while its socket remains open");
    }

    #[test]
    fn catch_up_retry_keeps_handshake_refusals_terminal_and_rate_hints_bounded() {
        for code in [
            ErrorCode::Unauthorized,
            ErrorCode::UpgradeRequired,
            ErrorCode::Other,
        ] {
            let mut backoff = Duration::from_secs(4);
            assert!(matches!(
                refused(code, "refused".into(), &mut backoff),
                Exit::Blocked(message) if message == "refused"
            ));
            assert_eq!(backoff, Duration::from_secs(4));
        }
        let mut backoff = Duration::from_secs(4);
        assert!(matches!(
            refused(ErrorCode::Unavailable, "unavailable".into(), &mut backoff),
            Exit::Retry(_)
        ));
        assert_eq!(backoff, Duration::from_secs(4));
        assert!(matches!(
            refused(
                ErrorCode::RateLimited,
                protocol::slow_down(Duration::from_secs(9)),
                &mut backoff
            ),
            Exit::Retry(_)
        ));
        assert_eq!(backoff, Duration::from_secs(9));
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

    #[tokio::test(flavor = "current_thread")]
    async fn handled_commit_collision_reconciles_without_listener_error() {
        queued_messages_round_trip(CatchUp::HandshakeCollision).await;
    }
    #[tokio::test(flavor = "current_thread")]
    async fn terminal_application_refusal_keeps_failed_delivery_and_listener_notice() {
        queued_messages_round_trip(CatchUp::PermissionRefusal).await;
    }
    #[tokio::test(flavor = "current_thread")]
    async fn handshake_storage_failure_notifies_and_retains_retry_before_reconciliation() {
        queued_messages_round_trip(CatchUp::StorageRefusal).await;
    }
    #[derive(Default)]
    struct RejectionListener {
        errors: Mutex<Vec<String>>,
        changed: tokio::sync::Notify,
    }
    impl CoreListener for RejectionListener {
        fn on_change(&self, _: Vec<String>) {}
        fn on_ephemeral(&self, _: String, _: String, _: String, _: String) {}
        fn on_presence(&self, _: String, _: bool) {}
        fn on_connection(&self, _: crate::ConnectionDto) {}
        fn on_error(&self, message: String) {
            self.errors.lock().unwrap().push(message);
            self.changed.notify_one();
        }
        fn on_profile_changed(&self, _: String) {}
    }
    enum CatchUp {
        Initial,
        Incremental,
        HandshakeCollision,
        PermissionRefusal,
        StorageRefusal,
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
        ::mls::migrate(&mut peer_store).unwrap();
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
                    media: None,
                    forwarded: false,
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
                    media: None,
                    forwarded: false,
                },
            )
            .unwrap();
        let fresh_key = format!("mls.sealed:{}", fresh.client_id);
        assert!(engine.store.meta(&fresh_key).unwrap().is_none());
        let advance = peer.commit(space, &[], &Default::default()).unwrap();
        assert!(matches!(
            peer.open(space, &advance.commit, &roster, peer.leaf())
                .unwrap(),
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
        let losing_commit = if matches!(
            catch_up,
            CatchUp::HandshakeCollision | CatchUp::StorageRefusal
        ) {
            engine.commit_now(space, &[], &Default::default()).unwrap();
            Some(
                engine
                    .store
                    .outbox_handshakes()
                    .unwrap()
                    .into_iter()
                    .find(|(_, _, kind)| kind == SealedKind::Commit.name())
                    .unwrap()
                    .0,
            )
        } else {
            None
        };
        if matches!(catch_up, CatchUp::StorageRefusal) {
            engine.store.conn().execute_batch("CREATE TRIGGER fail_refusal BEFORE DELETE ON outbox BEGIN SELECT RAISE(ABORT, 'injected handshake storage failure'); END;").unwrap();
        }
        let notices = Arc::new(RejectionListener::default());
        let engine = Arc::new(Mutex::new(engine));
        let (cmd, mut commands) = mpsc::unbounded_channel();
        let (status, _) = watch::channel(NetStatus {
            state: ConnState::Connecting,
            synced: false,
            error: None,
            registered: true,
            authenticated_relay: None,
        });
        let (maintenance, mut maintenance_started) = mpsc::unbounded_channel();
        let ctx = Ctx {
            engine: EngineLease::new(engine.clone()),
            listener: Some(notices.clone()),
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
                    server_time_ms: None,
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
            let mut held_commit = None;
            if matches!(
                catch_up,
                CatchUp::HandshakeCollision | CatchUp::StorageRefusal
            ) {
                reply(&mut socket, ServerFrame::SyncDone).await;
                loop {
                    match request(&mut socket).await {
                        ClientFrame::Publish { env }
                            if Some(env.client_id()) == losing_commit.as_deref() =>
                        {
                            reply(
                                &mut socket,
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
                        other => panic!("unexpected before commit collision {other:?}"),
                    }
                }
                if matches!(catch_up, CatchUp::StorageRefusal) {
                    notices.changed.notified().await;
                    {
                        let engine = lock(&engine);
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
                        &mut socket,
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
                        &mut socket,
                        ServerFrame::Event {
                            ev: missed_commit.clone(),
                        },
                    )
                    .await;
                }
            }
            if matches!(catch_up, CatchUp::PermissionRefusal) {
                reply(
                    &mut socket,
                    ServerFrame::Event {
                        ev: missed_commit.clone(),
                    },
                )
                .await;
                reply(&mut socket, ServerFrame::SyncDone).await;
            }
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
                            assert_eq!(::mls::application_epoch(bytes), Some(1));
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
                        ClientFrame::Ping => {
                            reply(
                                &mut socket,
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
                            reason: protocol::STALE_SEAL.into(),
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
                    op: Op::FetchLink {
                        id: marker_box.clone(),
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
                            op: Op::FetchLink { id: link },
                        } if link == marker_box => {
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
                        ClientFrame::Ping => {
                            reply(
                                &mut socket,
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
                        if matches!(catch_up, CatchUp::PermissionRefusal)
                            && env.client_id() == cached.client_id
                        {
                            assert!(readable.insert(env.client_id().to_string()));
                            reply(
                                &mut socket,
                                ServerFrame::Rejected {
                                    space: space.into(),
                                    client_id: env.client_id().into(),
                                    reason: "permission denied".into(),
                                    permanent: true,
                                },
                            )
                            .await;
                            continue;
                        }
                        let (_, bytes) = env.sealed_data().unwrap();
                        let epoch = ::mls::application_epoch(bytes);
                        if epoch == Some(1)
                            && matches!(
                                catch_up,
                                CatchUp::HandshakeCollision | CatchUp::StorageRefusal
                            )
                        {
                            // `original` above restricts this to the same two queued IDs.
                            // A sent copy can precede catch-up even after its server reply.
                            reply(
                                &mut socket,
                                ServerFrame::Rejected {
                                    space: space.into(),
                                    client_id: env.client_id().into(),
                                    reason: protocol::STALE_SEAL.into(),
                                    permanent: true,
                                },
                            )
                            .await;
                            if let Some(ev) = held_commit.take() {
                                reply(&mut socket, ServerFrame::Event { ev }).await;
                            }
                            continue;
                        }
                        assert_eq!(epoch, Some(2));
                        let Opened::Application { plaintext, .. } = peer
                            .open(
                                space,
                                bytes,
                                &roster,
                                &::mls::Leaf {
                                    identity: env.author().into(),
                                    device: env.device().unwrap().into(),
                                    cert: env.cert.clone().unwrap(),
                                },
                            )
                            .unwrap()
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
                    ClientFrame::Ping => {
                        reply(
                            &mut socket,
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
            assert!(held_commit.is_none(), "the old-epoch gap was not exercised");
            // A response ordered after both event echoes confirms that the client
            // applied them and cleared their pending records before stopping.
            let (marker, marked) = oneshot::channel();
            let marker_box = "22".repeat(32);
            cmd.send(Cmd::Req {
                op: Op::FetchLink {
                    id: marker_box.clone(),
                },
                reply: marker,
            })
            .unwrap();
            loop {
                match request(&mut socket).await {
                    ClientFrame::Req {
                        id,
                        op: Op::FetchLink { id: link },
                    } if link == marker_box => {
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
                    ClientFrame::Publish { .. } => {}
                    other => panic!("unexpected {other:?}"),
                }
            }
            assert!(matches!(marked.await.unwrap(), Ok(Reply::Link(None))));
            {
                let engine = lock(&engine);
                assert!(!engine.net.pending.contains_key(&cached.client_id));
                assert!(!engine.net.pending.contains_key(&fresh.client_id));
                engine.logs[space].verify().unwrap();
                let errors = notices.errors.lock().unwrap();
                if matches!(catch_up, CatchUp::PermissionRefusal) {
                    assert_eq!(errors.as_slice(), ["Could not send. Try again."]);
                    assert!(engine.store.outbox().unwrap().iter().any(|p| p.failed
                        && p.event.client_id == cached.client_id
                        && p.last_error.as_deref() == Some("permission denied")));
                } else if matches!(catch_up, CatchUp::StorageRefusal) {
                    assert_eq!(errors.as_slice(), ["Could not finish syncing. Try again."]);
                    assert_eq!(engine.outbox_len(), 0);
                } else {
                    assert!(
                        errors.is_empty(),
                        "handled reconciliation is not a user error: {errors:?}"
                    );
                    assert_eq!(engine.outbox_len(), 0);
                }
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
            authenticated_relay: None,
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
                    server_time_ms: None,
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
                            ClientFrame::Ping => reply(&mut socket, ServerFrame::Pong { server_time_ms: None }).await,
                            _ => {}
                        }
                    }
                })
        });
        let work_passes = passes.clone();
        let work_engine = engine.clone();
        let ctx = Ctx {
            engine: EngineLease::new(engine),
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

    #[tokio::test(flavor = "current_thread")]
    async fn member_seen_beyond_the_next_sequence_is_fetched_once_then_forks() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let mut engine = Engine::open(":memory:").unwrap();
        engine.create_account("Ana", "ana", &url).unwrap();
        engine.set_registered(true).unwrap();
        let author = engine.net.author.clone().unwrap();
        let space = "seen-loop";
        let mut head = None;
        let created = sequence(
            Envelope::plain(&author.sign_event(
                space,
                "created",
                1,
                None,
                EventBody::SpaceCreated {
                    title: space.into(),
                    kind: SpaceKind::Group,
                    privacy: Privacy::Closed,
                },
            )),
            &mut head,
        );
        assert_eq!(engine.ingest(created), Ingest::Applied);
        let peer = event_log::Author::root(event_log::Signer::generate());
        let added = sequence(
            Envelope::plain(&author.sign_event(
                space,
                "add-peer",
                2,
                head.clone(),
                EventBody::MemberAdded {
                    identity: peer.identity.clone(),
                    role: Role::Member,
                },
            )),
            &mut head,
        );
        assert_eq!(engine.ingest(added), Ingest::Applied);
        let next = engine.next_seq(space);
        let bad = sequence(
            Envelope::plain(&peer.sign_event(
                space,
                "seen-loop",
                3,
                Some(Seen {
                    seq: next + 8,
                    hash: "12".repeat(32),
                }),
                EventBody::MessagePosted {
                    message: "other-fork".into(),
                    text: "other-fork".into(),
                    attaches: None,
                    reply: None,
                    media: None,
                    forwarded: false,
                },
            )),
            &mut head,
        );
        assert_eq!(bad.seq, next);
        let engine = Arc::new(Mutex::new(engine));
        let (cmd, mut commands) = mpsc::unbounded_channel();
        let (status, _) = watch::channel(NetStatus {
            state: ConnState::Connecting,
            synced: false,
            error: None,
            registered: true,
            authenticated_relay: None,
        });
        let relay_engine = engine.clone();
        let relay_space = space.to_string();
        let relay = async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(socket).await.unwrap();
            let ClientFrame::Hello { identity, .. } = request(&mut socket).await else {
                panic!("hello");
            };
            reply(
                &mut socket,
                ServerFrame::Challenge {
                    nonce: "seen-loop".into(),
                    relay: "seen-loop".into(),
                    protocol: PROTOCOL_VERSION,
                    capabilities: Vec::new(),
                    server_time_ms: None,
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
            reply(&mut socket, ServerFrame::Event { ev: bad }).await;
            let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
            let mut refetches = 0u32;
            loop {
                let left = deadline.saturating_duration_since(tokio::time::Instant::now());
                if left.is_zero() {
                    break;
                }
                let frame = match tokio::time::timeout(left, request(&mut socket)).await {
                    Ok(frame) => frame,
                    Err(_) => break,
                };
                match frame {
                    ClientFrame::Sync { .. } => {
                        refetches += 1;
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
                        .await;
                    }
                    ClientFrame::Ping => {
                        reply(
                            &mut socket,
                            ServerFrame::Pong {
                                server_time_ms: None,
                            },
                        )
                        .await;
                    }
                    ClientFrame::Publish { .. } => {}
                    other => panic!("unexpected {other:?}"),
                }
            }
            let guard = lock(&relay_engine);
            let integrity = guard.state.spaces[&relay_space].integrity_error.clone();
            let stored = guard.logs[&relay_space]
                .events()
                .iter()
                .any(|event| event.client_id == "seen-loop");
            drop(guard);
            cmd.send(Cmd::Stop).unwrap();
            (refetches, integrity, stored)
        };
        let ctx = Ctx {
            engine: EngineLease::new(engine),
            listener: None,
            status,
            lang: crate::i18n::Lang::En,
            maintenance_work: None,
        };
        let mut backoff = Duration::from_millis(500);
        let (_exit, (refetches, integrity, stored)) =
            tokio::time::timeout(Duration::from_secs(15), async {
                tokio::join!(session(&ctx, &mut commands, &mut backoff), relay)
            })
            .await
            .expect("seen-loop sync stalled");
        assert_eq!(refetches, 0, "sync asked for the same next event again");
        assert_eq!(
            integrity.as_deref(),
            Some("Members see different groups in this chat (the server may have forked it).")
        );
        assert!(!stored, "the refused event was appended");
    }
    #[test]
    fn superseded_profile_query_cannot_consume_current_batch_or_its_backoff() {
        let mut waiting = HashMap::new();
        remember_waiting(
            &mut waiting,
            1,
            Waiting::SealedProfiles(vec!["self".into()]),
        );
        remember_waiting(
            &mut waiting,
            2,
            Waiting::AgreementKeys(vec!["contact".into()]),
        );
        remember_waiting(
            &mut waiting,
            3,
            Waiting::SealedProfiles(vec!["self".into()]),
        );
        assert!(
            waiting.remove(&1).is_none(),
            "late Q1 must never reach sealed_profiles_arrived"
        );
        assert!(
            matches!(waiting.get(&3),Some(Waiting::SealedProfiles(ids)) if ids==&vec!["self".to_owned()])
        );
        assert!(
            waiting.contains_key(&2),
            "independent agreement request remains tracked"
        );
        remember_waiting(&mut waiting, 4, Waiting::ProfileUploaded(2));
        remember_waiting(&mut waiting, 5, Waiting::ProfileUploaded(3));
        assert!(
            waiting.remove(&4).is_none(),
            "old upload ACK must not settle current request deadline"
        );
        assert!(matches!(waiting.get(&5), Some(Waiting::ProfileUploaded(3))));
    }
}

#[cfg(test)]
#[path = "net/file_demand_tests.rs"]
mod file_demand_tests;
