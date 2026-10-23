//! One WebSocket = one authenticated device.
//!
//! The device row authorizes request admission. An ordinary request admitted before
//! Unlink's Postgres commit may finish afterward, including an FDB append whose caller
//! was cancelled. Fresh requests and every outgoing frame recheck the durable row.
//! Atomic ordering of FDB commits against Postgres revocation would require one
//! transaction domain; cancelling a response future cannot establish that ordering.

use crate::pseudonym::pseudo;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, OnceLock,
    },
    time::Duration,
};

use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use protocol::{
    negotiate, AgentRequest, ClientFrame, ErrorCode, Op, Reply, ServerFrame, MIN_PROTOCOL_VERSION,
    PROTOCOL_VERSION,
};
use tokio::sync::{mpsc, Notify};
use tracing::{field::Empty, Instrument};
use types::{EventBody, IdentityKind};

use crate::{db, hub::Mailbox, limits, log::Sequencing, metrics::Metrics, Shared};

mod authentication;
mod ephemeral;
mod online;
mod publishing;
mod requests;
mod syncing;

use authentication::{handshake, LoggedIn};

/// Spaces up to this size hear every member's receipts; larger ones route each receipt
/// to the author it concerns (`Session::receipt_audience`).
pub const RECEIPT_BROADCAST_MAX: usize = 16;

pub const MAX_FRAME: usize = relay_admission::portable::MAX_FRAME_BYTES;
pub const MAX_ENVELOPE: usize = relay_admission::portable::MAX_ENVELOPE_BYTES;
const SYNC_PAGE: usize = 500;

struct Session {
    st: Shared,
    tx: mpsc::Sender<ServerFrame>,
    identity: String,
    device: String,
    cert: String,
    hub_id: Option<u64>,
    // Polled alongside frames, never awaited by login or a request handler. Dropping
    // the session cancels the lookup/snapshot without leaving a detached task.
    presence: Option<std::sync::Mutex<futures_util::future::BoxFuture<'static, ()>>>,
    kick: Arc<Notify>,
    revoked: Arc<AtomicBool>,
    require_device: Arc<AtomicBool>,
    /// The client's address, for per-IP limits.
    ip: String,
    /// When the device logged in, for time to first sync.
    started: std::time::Instant,
    first_sync_done: bool,
    claim_receipts: bool,
}

struct DeliveryAuthorization {
    identity: String,
    device: String,
    require_device: Arc<AtomicBool>,
}

fn nonce() -> String {
    let mut b = [0u8; 32];
    getrandom::getrandom(&mut b).expect("entropy");
    hex::encode(b)
}

async fn recv_frame(
    stream: &mut futures_util::stream::SplitStream<WebSocket>,
) -> Option<ClientFrame> {
    while let Some(Ok(msg)) = stream.next().await {
        match msg {
            Message::Binary(b) => return ClientFrame::decode(&b).ok(),
            Message::Close(_) => return None,
            _ => continue,
        }
    }
    None
}

pub async fn run(socket: WebSocket, st: Shared, ip: String) {
    Metrics::inc(&st.metrics.connections);
    let (mut sink, mut stream) = socket.split();
    let (tx, mut rx) = mpsc::channel::<ServerFrame>(1024);
    let credentials = Arc::new(OnceLock::<DeliveryAuthorization>::new());
    let writer_credentials = credentials.clone();
    let revoked = Arc::new(AtomicBool::new(false));
    let writer_revoked = revoked.clone();
    let kick = Arc::new(Notify::new());
    let writer_kick = kick.clone();
    let delivery_auth = st.delivery_auth.clone();
    let writer = tokio::spawn(async move {
        loop {
            let frame = tokio::select! {
                frame = rx.recv() => match frame { Some(frame) => frame, None => break },
                _ = writer_kick.notified() => break,
            };
            if writer_revoked.load(Ordering::Acquire) {
                break;
            }
            // Even when a node misses the bus invalidation, the durable row fences
            // every delivery against revocation. Hold it until the bytes are sent.
            let authorization = if let Some(auth) = writer_credentials.get() {
                match db::authorize_device(
                    &delivery_auth,
                    &auth.identity,
                    &auth.device,
                    auth.require_device.load(Ordering::Acquire),
                )
                .await
                {
                    Ok(Some(tx)) => Some(tx),
                    _ => {
                        writer_revoked.store(true, Ordering::Release);
                        writer_kick.notify_one();
                        break;
                    }
                }
            } else {
                None
            };
            let sent = tokio::time::timeout(
                Duration::from_secs(5),
                sink.send(Message::Binary(frame.encode().into())),
            )
            .await;
            drop(authorization);
            if !matches!(sent, Ok(Ok(()))) {
                writer_kick.notify_one();
                break;
            }
        }
        let _ = tokio::time::timeout(Duration::from_secs(1), sink.close()).await;
        writer_kick.notify_one();
    });

    let Some(LoggedIn {
        identity,
        device,
        cert,
        registered,
        claim_receipts,
    }) = handshake(&st, &tx, &mut stream, &ip).await
    else {
        return finish(writer, tx).await;
    };

    if registered {
        let refused = match db::touch_device(&st.pool, &identity, &device, &cert).await {
            Ok(true) => None,
            Ok(false) => Some(ServerFrame::error(
                ErrorCode::Unauthorized,
                "link this device first",
            )),
            Err(_) => Some(ServerFrame::error(
                ErrorCode::Unavailable,
                "database unavailable",
            )),
        };
        if let Some(error) = refused {
            let _ = tx.send(error).await;
            return finish(writer, tx).await;
        }
    }
    let initial_authorization =
        match db::authorize_device(&st.session_auth, &identity, &device, registered).await {
            Ok(Some(tx)) => tx,
            Ok(None) => {
                let _ = tx
                    .send(ServerFrame::error(
                        ErrorCode::Unauthorized,
                        "this device was unlinked",
                    ))
                    .await;
                return finish(writer, tx).await;
            }
            Err(_) => {
                let _ = tx
                    .send(ServerFrame::error(
                        ErrorCode::Unavailable,
                        "database unavailable",
                    ))
                    .await;
                return finish(writer, tx).await;
            }
        };
    let require_device = Arc::new(AtomicBool::new(registered));
    let _ = credentials.set(DeliveryAuthorization {
        identity: identity.clone(),
        device: device.clone(),
        require_device: require_device.clone(),
    });

    let mut s = Session {
        st: st.clone(),
        tx: tx.clone(),
        identity: identity.clone(),
        device,
        cert,
        hub_id: None,
        presence: None,
        kick,
        revoked,
        require_device,
        ip,
        started: std::time::Instant::now(),
        first_sync_done: false,
        claim_receipts,
    };
    // `ready` first: the client's handshake reads it before anything else.
    let _ = tx
        .send(ServerFrame::Ready {
            identity: identity.clone(),
            registered,
        })
        .await;
    drop(initial_authorization);
    if registered
        && tokio::time::timeout(Duration::from_secs(5), s.go_online())
            .await
            .is_err()
    {
        s.kick.notify_waiters();
        s.kick.notify_one();
    }
    tracing::info!(identity = %pseudo(&identity), registered, "session ready");

    // ── frames ──
    let kick = s.kick.clone();
    let mut refresh = tokio::time::interval(Duration::from_secs(5));
    let mut idle_deadline = tokio::time::Instant::now() + Duration::from_secs(90);
    loop {
        tokio::select! {
            _ = async {
                match s.presence.as_mut() {
                    Some(presence) => presence.get_mut().expect("presence future poisoned").await,
                    None => std::future::pending().await,
                }
            } => { s.presence = None; }
            frame = tokio::time::timeout_at(idle_deadline, recv_frame(&mut stream)) => {
                match frame {
                    Ok(Some(f)) => {
                        idle_deadline = tokio::time::Instant::now() + Duration::from_secs(90);
                        let handled = if matches!(&f, ClientFrame::Sync { .. }) {
                            // Catch-up may stream for longer than one request budget;
                            // each log read and output wait has its own deadline.
                            tokio::select! {
                                _ = kick.notified() => false,
                                keep = s.handle(f) => keep,
                            }
                        } else {
                            tokio::select! {
                                _ = kick.notified() => false,
                                result = tokio::time::timeout(Duration::from_secs(5), s.handle(f)) => result.is_ok_and(|keep| keep),
                            }
                        };
                        if !handled { break; }
                    },
                    Ok(None) => break,
                    Err(_) => { tracing::info!(identity = %pseudo(&identity), "idle timeout"); break }
                }
            }
            _ = kick.notified() => {
                tracing::warn!(identity = %pseudo(&identity), "session too slow, disconnecting so it resyncs");
                break;
            }
            _ = refresh.tick() => {
                if !tokio::time::timeout(Duration::from_secs(5), s.still_authorized()).await.is_ok_and(|active| active) { break; }
            }
        }
    }

    let last = s.hub_id.is_some_and(|id| st.fanout.remove(&identity, id));
    drop(s);
    finish(writer, tx).await;
    if last && !st.fanout.is_online(&identity).await {
        if let Ok(Ok(co)) =
            tokio::time::timeout(Duration::from_secs(2), st.log.co_members(&identity)).await
        {
            st.fanout.send(
                &co,
                &ServerFrame::Presence {
                    identity: identity.clone(),
                    online: false,
                },
                None,
            );
        }
    }
}

async fn finish(mut writer: tokio::task::JoinHandle<()>, tx: mpsc::Sender<ServerFrame>) {
    drop(tx);
    if tokio::time::timeout(Duration::from_secs(2), &mut writer)
        .await
        .is_err()
    {
        writer.abort();
        let _ = writer.await;
    }
}

impl Session {
    async fn send(&self, f: ServerFrame) {
        let _ = self.tx.send(f).await;
    }

    async fn send_sync(&self, frame: ServerFrame) -> bool {
        if tokio::time::timeout(Duration::from_secs(5), self.tx.send(frame))
            .await
            .is_ok_and(|sent| sent.is_ok())
        {
            true
        } else {
            self.kick.notify_waiters();
            self.kick.notify_one();
            false
        }
    }

    async fn handle(&mut self, f: ClientFrame) -> bool {
        if self.revoked.load(Ordering::Acquire) {
            return false;
        }
        let registered = self.hub_id.is_some();
        // Directory changes keep the active-device fence through their SQL commit.
        // They reuse this connection even when each bounded pool has only one slot.
        let directory_change = matches!(
            &f,
            ClientFrame::Req {
                op: Op::Unlink { .. } | Op::Register { .. } | Op::DeliverLink { .. },
                ..
            }
        );
        let authorization_pool = if directory_change {
            &self.st.pool
        } else {
            &self.st.session_auth
        };
        let authorization = match tokio::time::timeout(
            Duration::from_secs(5),
            db::authorize_device(authorization_pool, &self.identity, &self.device, registered),
        )
        .await
        {
            Ok(Ok(Some(tx))) => tx,
            _ => {
                self.revoked.store(true, Ordering::Release);
                self.kick.notify_one();
                return false;
            }
        };
        let mut authorization = if directory_change {
            Some(authorization)
        } else {
            // This is a request-admission fence. Work admitted before revocation can
            // finish, including a queued FDB append after this handler is cancelled.
            // Fresh requests and every later delivery independently check the row.
            if !tokio::time::timeout(Duration::from_secs(5), authorization.commit())
                .await
                .is_ok_and(|committed| committed.is_ok())
            {
                return false;
            }
            None
        };
        match f {
            ClientFrame::Ping => {
                let server_time_ms = if self.claim_receipts {
                    tokio::time::timeout(
                        Duration::from_secs(5),
                        crate::key_package_claims::clock_ms(&self.st.pool),
                    )
                    .await
                    .ok()
                    .and_then(Result::ok)
                } else {
                    None
                };
                self.send(ServerFrame::Pong { server_time_ms }).await;
            }
            ClientFrame::Req { id, op } => {
                let refill = matches!(&op, Op::PublishKeyPackages { .. });
                let unlinked = match &op {
                    Op::Unlink { device } => Some(device.clone()),
                    _ => None,
                };
                let registering = matches!(&op, Op::Register { .. });
                let span = tracing::info_span!(
                    "request",
                    op = op.name(),
                    identity = %pseudo(&self.identity),
                    outcome = Empty,
                );
                let result = match self.st.limits.request_device.check(&self.device) {
                    Ok(()) => {
                        let r = self
                            .request(op, registered, authorization.as_mut())
                            .instrument(span.clone())
                            .await;
                        span.record("outcome", if r.is_ok() { "ok" } else { "refused" });
                        r
                    }
                    Err(wait) => {
                        span.record("outcome", "rate_limited");
                        Err(limits::slow_down(wait))
                    }
                };
                let refill = refill && result.is_ok();
                if let Some(authorization) = authorization {
                    let committed = if result.is_ok() {
                        authorization.commit().await
                    } else {
                        authorization.rollback().await
                    };
                    if committed.is_err() {
                        return false;
                    }
                }
                if result.is_ok() {
                    if registering {
                        self.require_device.store(true, Ordering::Release);
                        self.go_online().await;
                    }
                    if let Some(device) = unlinked {
                        self.st.fanout.revoke_device(&self.identity, &device).await;
                    }
                }
                self.send(ServerFrame::Res { id, result }).await;
                // Reconcile claims after the publication acknowledgment, when the client
                // has cleared its in-flight batch and can act on the current stock.
                if refill {
                    self.send_key_package_stock().await;
                }
                return !self.revoked.load(Ordering::Acquire);
            }
            _ if !registered => {
                self.send(ServerFrame::error(ErrorCode::Other, "register first"))
                    .await
            }
            ClientFrame::Publish { env } => self.publish(env).await,
            ClientFrame::Sync { cursors, all } => self.sync(cursors, all).await,
            ClientFrame::Ephemeral { space, kind } => {
                return self.ephemeral(space, kind).await && !self.revoked.load(Ordering::Acquire);
            }
            ClientFrame::Hello { .. } | ClientFrame::Auth { .. } => {
                self.send(ServerFrame::error(ErrorCode::Other, "already logged in"))
                    .await
            }
        }
        !self.revoked.load(Ordering::Acquire)
    }
}

/// A named Space is served from sync, not from the live canonical fan-out.
fn fork_histories_hold(space: &str) -> bool {
    #[cfg(debug_assertions)]
    {
        crate::log::sequencer::fork_histories::armed(space)
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = space;
        false
    }
}

fn valid_hex64(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}
