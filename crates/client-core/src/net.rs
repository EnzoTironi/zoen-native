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
    auth_message, ClientFrame, EphemeralKind, ErrorCode, KeyPackageClaimClock, Op, Reply,
    ServerFrame, CAPABILITIES, KEY_PACKAGE_CLAIM_TTL_MS, MIN_PROTOCOL_VERSION, PROTOCOL_VERSION,
};
use tokio::sync::{mpsc, oneshot, watch};
use tokio_tungstenite::tungstenite::Message;
use types::Identity;

use crate::engine::Engine;
use crate::sync::{Ingest, RejectionOutcome};
use crate::CoreListener;

mod dispatch;
mod maintenance;
mod media;
mod pickup;
mod profile_traffic;
mod receipts;
mod responses;
mod session;
mod transport;

#[cfg(test)]
use media::bounded_blob;
use media::media_worker;
pub(crate) use pickup::fetch_link_box;
use profile_traffic::{free, remember_waiting, settle};
use session::session;
#[cfg(test)]
use session::session_with_repair_policy;
use transport::{flush, recv, refused, register, run, send};

mod cursor_repair;
use cursor_repair::{CursorRepair, CursorRepairPolicy};

/// Live fan-out can race a room's membership history during catch-up. Hold only a
/// bounded set of unvalidated pairs on this socket, never in the durable cache.
#[derive(Default)]
pub(crate) struct DeferredReceipts(HashMap<(String, String), (u64, u64)>);

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
        self.engine.with(|engine| {
            if engine.net.file_demands.clear().is_err() {
                tracing_like("media: historical demand state unavailable at shutdown");
            }
        });
        let _ = self.cmd.send(Cmd::Stop);
        if let Some(rt) = self.rt.take() {
            // Safe from inside another async runtime too.
            rt.shutdown_background();
        }
        self.engine.revoke();
    }
}

#[derive(Clone)]
pub struct RequestHandle {
    cmd: mpsc::UnboundedSender<Cmd>,
}

impl RequestHandle {
    pub(crate) fn same_channel(&self, other: &Self) -> bool {
        self.cmd.same_channel(&other.cmd)
    }

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

type Sink = futures_util::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    Message,
>;
type Stream = futures_util::stream::SplitStream<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
>;

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

/// Encrypted-profile requests: free (`None`), or in flight / backing off until a deadline.
#[derive(Default)]
struct ProfileTraffic {
    agreement: Option<tokio::time::Instant>,
    upload: Option<tokio::time::Instant>,
    keys: Option<tokio::time::Instant>,
    profiles: Option<tokio::time::Instant>,
    key_packages: Option<tokio::time::Instant>,
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

/// Background deadlines survive one admitted foreground attempt and its failure.
#[derive(Default)]
struct MediaDownloadRetries(HashMap<String, (tokio::time::Instant, u32)>);

impl MediaDownloadRetries {
    fn ready(&self, blob: &str, foreground: bool, now: tokio::time::Instant) -> bool {
        foreground || self.0.get(blob).is_none_or(|(at, _)| *at <= now)
    }

    fn completed(&mut self, blob: &str) {
        self.0.remove(blob);
    }

    fn failed(&mut self, blob: &str, now: tokio::time::Instant) {
        let tries = self.0.get(blob).map(|(_, n)| n + 1).unwrap_or(1);
        let wait =
            Duration::from_secs(2u64.saturating_pow(tries.min(8))).min(Duration::from_secs(300));
        self.0.insert(blob.into(), (now + wait, tries));
    }
}

pub(crate) fn tracing_like(msg: &str) {
    if std::env::var_os("ZOEN_NET_DEBUG").is_some() {
        eprintln!("[zoen-net] {msg}");
    }
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

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "net/file_demand_tests.rs"]
mod file_demand_tests;
