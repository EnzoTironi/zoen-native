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
use roda_log::{device_cert_message, verify_sig};
use roda_proto::{
    auth_message, negotiate, normalize_handle, ClientFrame, ErrorCode, Op, Reply, ServerFrame,
    MIN_PROTOCOL_VERSION, PROTOCOL_VERSION,
};
use roda_types::{EventBody, IdentityKind};
use tokio::sync::{mpsc, Notify};
use tracing::{field::Empty, Instrument};

use crate::{db, hub::Mailbox, limits, log::Sequencing, metrics::Metrics, Shared};

/// An X25519/Ed25519 key package is about 300 bytes; this leaves room for extensions.
const MAX_KEY_PACKAGE: usize = 4096;

pub const MAX_FRAME: usize = 1024 * 1024;
const MAX_ENVELOPE: usize = 90 * 1024;
const SYNC_PAGE: usize = 500;

struct Session {
    st: Shared,
    tx: mpsc::Sender<ServerFrame>,
    identity: String,
    device: String,
    cert: String,
    hub_id: Option<u64>,
    kick: Arc<Notify>,
    revoked: Arc<AtomicBool>,
    require_device: Arc<AtomicBool>,
    /// The client's address, for per-IP limits.
    ip: String,
    /// When the device logged in, for time to first sync.
    started: std::time::Instant,
    first_sync_done: bool,
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
        kick,
        revoked,
        require_device,
        ip,
        started: std::time::Instant::now(),
        first_sync_done: false,
    };
    // `ready` first: the client's handshake reads it before anything else.
    let _ = tx
        .send(ServerFrame::Ready {
            identity: identity.clone(),
            registered,
        })
        .await;
    drop(initial_authorization);
    if registered {
        if tokio::time::timeout(Duration::from_secs(5), s.go_online())
            .await
            .is_err()
        {
            s.kick.notify_waiters();
            s.kick.notify_one();
        }
    }
    tracing::info!(identity = %pseudo(&identity), registered, "session ready");

    // ── frames ──
    let kick = s.kick.clone();
    let mut refresh = tokio::time::interval(Duration::from_secs(5));
    let mut idle_deadline = tokio::time::Instant::now() + Duration::from_secs(90);
    loop {
        tokio::select! {
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

/// A device that proved its keys.
struct LoggedIn {
    identity: String,
    device: String,
    cert: String,
    registered: bool,
}

/// Hello → Challenge → Auth. `None` when the client is refused; the reason has been sent.
#[tracing::instrument(
    name = "handshake",
    skip_all,
    fields(outcome = Empty, identity = Empty, registered = Empty)
)]
async fn handshake(
    st: &Shared,
    tx: &mpsc::Sender<ServerFrame>,
    stream: &mut futures_util::stream::SplitStream<WebSocket>,
    ip: &str,
) -> Option<LoggedIn> {
    let refuse = |outcome: &'static str, frame: ServerFrame| async move {
        tracing::Span::current().record("outcome", outcome);
        let _ = tx.send(frame).await;
        None
    };
    if let Err(wait) = st.limits.connect_ip.check(ip) {
        return refuse(
            "rate_limited",
            ServerFrame::error(ErrorCode::RateLimited, limits::slow_down(wait)),
        )
        .await;
    }
    let hello = tokio::time::timeout(Duration::from_secs(10), recv_frame(stream))
        .await
        .ok()
        .flatten();
    let Some(ClientFrame::Hello {
        protocol,
        capabilities,
        identity,
        device,
        cert,
    }) = hello
    else {
        return refuse(
            "no_hello",
            ServerFrame::error(ErrorCode::Other, "expected hello"),
        )
        .await;
    };
    if protocol < MIN_PROTOCOL_VERSION {
        Metrics::inc(&st.metrics.upgrade_required);
        return refuse(
            "upgrade_required",
            ServerFrame::error(
                ErrorCode::UpgradeRequired,
                format!("protocol {protocol} is too old, this relay needs {MIN_PROTOCOL_VERSION} or newer"),
            ),
        )
        .await;
    }
    if !verify_sig(&identity, &device_cert_message(&device), &cert) {
        return refuse(
            "bad_certificate",
            ServerFrame::error(ErrorCode::Unauthorized, "device certificate invalid"),
        )
        .await;
    }
    let n = nonce();
    let _ = tx
        .send(ServerFrame::Challenge {
            nonce: n.clone(),
            relay: st.relay_name.clone(),
            protocol: protocol.min(PROTOCOL_VERSION),
            capabilities: negotiate(&capabilities),
        })
        .await;
    let auth = tokio::time::timeout(Duration::from_secs(10), recv_frame(stream))
        .await
        .ok()
        .flatten();
    let Some(ClientFrame::Auth { sig }) = auth else {
        return refuse(
            "no_auth",
            ServerFrame::error(ErrorCode::Other, "expected auth"),
        )
        .await;
    };
    if !verify_sig(&device, &auth_message(&n, &st.relay_name), &sig) {
        return refuse(
            "bad_signature",
            ServerFrame::error(ErrorCode::Unauthorized, "login signature invalid"),
        )
        .await;
    }
    match db::device_revoked(&st.pool, &device).await {
        Ok(false) => {}
        Ok(true) => {
            return refuse(
                "unlinked",
                ServerFrame::error(ErrorCode::Unauthorized, "this device was unlinked"),
            )
            .await;
        }
        Err(_) => {
            return refuse(
                "unavailable",
                ServerFrame::error(ErrorCode::Unavailable, "database unavailable"),
            )
            .await;
        }
    }
    let Ok(registered) = db::is_registered(&st.pool, &identity).await else {
        return refuse(
            "unavailable",
            ServerFrame::error(ErrorCode::Unavailable, "database unavailable"),
        )
        .await;
    };
    if registered {
        match db::device_linked(&st.pool, &identity, &device).await {
            Ok(true) => {}
            Ok(false) => {
                return refuse(
                    "not_enrolled",
                    ServerFrame::error(ErrorCode::Unauthorized, "link this device first"),
                )
                .await;
            }
            Err(_) => {
                return refuse(
                    "unavailable",
                    ServerFrame::error(ErrorCode::Unavailable, "database unavailable"),
                )
                .await;
            }
        }
    }
    let span = tracing::Span::current();
    span.record("outcome", "ok");
    span.record("identity", tracing::field::display(pseudo(&identity)));
    span.record("registered", registered);
    Some(LoggedIn {
        identity,
        device,
        cert,
        registered,
    })
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

    /// Tells each claimed device that's running low, wherever it is connected.
    async fn key_packages_low(&self, claimed: &[roda_proto::KeyPackageRecord]) {
        let devices: Vec<(String, String)> = claimed
            .iter()
            .map(|k| (k.identity.clone(), k.device.clone()))
            .collect();
        let stock = match db::key_package_stock(&self.st.pool, &devices).await {
            Ok(stock) => stock,
            Err(error) => {
                tracing::warn!(%error, "claimed key-package stock query failed");
                return;
            }
        };
        for (identity, device, left) in stock {
            if left < roda_proto::KEY_PACKAGES_LOW as i64 {
                let delivered_here = self.st.fanout.send(
                    std::slice::from_ref(&identity),
                    &ServerFrame::KeyPackagesLow {
                        device: device.clone(),
                        remaining: left as u32,
                    },
                    None,
                );
                tracing::debug!(identity = %pseudo(&identity), device = %pseudo(&device), remaining = left, delivered_here, "low key-package notice");
            }
        }
    }

    async fn go_online(&mut self) {
        if self.hub_id.is_some() {
            return;
        }
        self.st.analytics.session(&self.identity);
        let was_online = self.st.fanout.is_online(&self.identity).await;
        self.hub_id = Some(self.st.fanout.add(
            &self.identity,
            Mailbox {
                tx: self.tx.clone(),
                kick: self.kick.clone(),
                device: self.device.clone(),
                revoked: self.revoked.clone(),
            },
        ));
        // Attach before reading stock: a claim racing the snapshot then reaches this
        // mailbox, or is reflected in the query. Reading first can lose both signals.
        // Packages claimed while this device was away: it refills as it comes back.
        self.send_key_package_stock().await;
        if let Ok(Ok(co)) = tokio::time::timeout(
            Duration::from_secs(2),
            self.st.log.co_members(&self.identity),
        )
        .await
        {
            if !was_online {
                self.st.fanout.send(
                    &co,
                    &ServerFrame::Presence {
                        identity: self.identity.clone(),
                        online: true,
                    },
                    None,
                );
            }
            // And tell this device who is already here (every device, not just the first).
            for who in self.st.fanout.online(&co).await {
                self.send(ServerFrame::Presence {
                    identity: who,
                    online: true,
                })
                .await;
            }
        }
    }

    async fn send_key_package_stock(&self) {
        let me = [(self.identity.clone(), self.device.clone())];
        if let Ok(stock) = db::key_package_stock(&self.st.pool, &me).await {
            for (_, device, left) in stock {
                if left < roda_proto::KEY_PACKAGES_LOW as i64 {
                    self.send(ServerFrame::KeyPackagesLow {
                        device,
                        remaining: left as u32,
                    })
                    .await;
                }
            }
        }
    }

    /// Device bucket first, then the account's: one device can't spend its siblings' share.
    fn publish_allowed(&self) -> Result<(), std::time::Duration> {
        let l = &self.st.limits;
        l.publish_device.check(&self.device)?;
        l.publish_account.check(&self.identity)
    }

    async fn still_authorized(&self) -> bool {
        if self.revoked.load(Ordering::Acquire) {
            return false;
        }
        match db::authorize_device(
            &self.st.session_auth,
            &self.identity,
            &self.device,
            self.require_device.load(Ordering::Acquire),
        )
        .await
        {
            Ok(Some(tx)) => {
                drop(tx);
                true
            }
            _ => {
                self.revoked.store(true, Ordering::Release);
                self.kick.notify_one();
                false
            }
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
            ClientFrame::Ping => self.send(ServerFrame::Pong).await,
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
            ClientFrame::Ephemeral { space, kind } => self.ephemeral(space, kind).await,
            ClientFrame::Hello { .. } | ClientFrame::Auth { .. } => {
                self.send(ServerFrame::error(ErrorCode::Other, "already logged in"))
                    .await
            }
        }
        !self.revoked.load(Ordering::Acquire)
    }

    #[tracing::instrument(
        name = "ephemeral",
        skip_all,
        fields(identity = %pseudo(&self.identity), space = %pseudo(&space), delivered_here = Empty)
    )]
    async fn ephemeral(&self, space: String, kind: roda_proto::EphemeralKind) {
        if self.st.limits.ephemeral_device.check(&self.device).is_err() {
            return;
        }
        let Ok(Some(_)) = self.st.log.role(&space, &self.identity).await else {
            return;
        };
        let Ok(members) = self.st.log.members(&space).await else {
            return;
        };
        let to: Vec<String> = members
            .into_iter()
            .map(|(m, _)| m)
            .filter(|m| *m != self.identity)
            .collect();
        let n = self.st.fanout.send(
            &to,
            &ServerFrame::Ephemeral {
                space,
                from: self.identity.clone(),
                kind,
            },
            None,
        );
        tracing::Span::current().record("delivered_here", n as i64);
        Metrics::inc(&self.st.metrics.ephemeral_forwarded);
    }

    async fn request(
        &mut self,
        op: Op,
        registered: bool,
        authorization: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<Reply, String> {
        let pool = &self.st.pool;
        match op {
            Op::Register { mut profile } => {
                if profile.id != self.identity {
                    return Err("profile is for another identity".into());
                }
                let handle = normalize_handle(&profile.handle).ok_or(
                    "handle must be 3–24 letters, digits, '.' or '_', starting with a letter",
                )?;
                profile.handle = handle.clone();
                match profile.kind {
                    IdentityKind::Person => {
                        profile.name.clear();
                        profile.bio.clear();
                    }
                    IdentityKind::Agent => {
                        profile.name = profile.name.trim().chars().take(64).collect();
                        profile.bio = profile.bio.chars().take(280).collect();
                        if profile.name.is_empty() {
                            return Err("name is required".into());
                        }
                        if profile.owner.is_none() {
                            return Err("an agent needs an owner".into());
                        }
                    }
                }
                if !registered {
                    self.st
                        .limits
                        .register_ip
                        .check(&self.ip)
                        .map_err(limits::slow_down)?;
                }
                db::register(
                    authorization.ok_or("registration authorization unavailable")?,
                    &profile,
                    &handle,
                    &self.device,
                    &self.cert,
                    !registered,
                )
                .await?;
                Ok(Reply::Registered(profile))
            }
            Op::FetchLink { id } => {
                if !valid_hex64(&id) {
                    return Err("bad link id".into());
                }
                Ok(Reply::Link(
                    db::take_link_box(pool, &id)
                        .await
                        .map_err(|e| e.to_string())?,
                ))
            }
            _ if !registered => Err("register first".into()),
            Op::DeliverLink { id, sealed, device } => {
                self.st
                    .limits
                    .lookup_account
                    .check(&self.identity)
                    .map_err(limits::slow_down)?;
                if !valid_hex64(&id) || sealed.is_empty() || sealed.len() > db::MAX_LINK_BOX {
                    return Err("bad link box".into());
                }
                let authorization = authorization.ok_or("link authorization unavailable")?;
                if let Some(device) = device {
                    db::enroll_device(authorization, &self.identity, &device.device, &device.cert)
                        .await
                        .map_err(|e| e.to_string())?;
                }
                if !db::put_link_box(authorization, &id, &sealed)
                    .await
                    .map_err(|e| e.to_string())?
                {
                    return Err("a link box is already there".into());
                }
                Ok(Reply::Done)
            }
            Op::Devices => Ok(Reply::Devices(
                db::devices_of(pool, &self.identity)
                    .await
                    .map_err(|e| e.to_string())?,
            )),
            Op::Unlink { device } => {
                let authorization = authorization.ok_or("unlink authorization unavailable")?;
                if !db::revoke_device(authorization, &self.identity, &device)
                    .await
                    .map_err(|e| e.to_string())?
                {
                    return Err("not a linked device of yours".into());
                }
                tracing::info!(identity = %pseudo(&self.identity), "device unlinked");
                Ok(Reply::Done)
            }
            Op::SendDevice { to, sealed } => {
                self.st
                    .limits
                    .lookup_account
                    .check(&self.identity)
                    .map_err(limits::slow_down)?;
                if sealed.is_empty() || sealed.len() > db::MAX_LINK_BOX * 16 {
                    return Err("bad device message".into());
                }
                if !db::device_linked(pool, &self.identity, &to)
                    .await
                    .map_err(|e| e.to_string())?
                {
                    return Err("not a linked device of yours".into());
                }
                self.st.fanout.send(
                    std::slice::from_ref(&self.identity),
                    &ServerFrame::DeviceMessage {
                        from: self.device.clone(),
                        to,
                        sealed,
                    },
                    None,
                );
                Ok(Reply::Done)
            }
            Op::Lookup { handle, prefix } => {
                self.st
                    .limits
                    .lookup_account
                    .check(&self.identity)
                    .map_err(limits::slow_down)?;
                let h = handle.trim().trim_start_matches('@').to_lowercase();
                if h.is_empty() {
                    return Ok(Reply::Profiles(Vec::new()));
                }
                Ok(Reply::Profiles(
                    db::lookup(pool, &h, prefix)
                        .await
                        .map_err(|e| e.to_string())?,
                ))
            }
            Op::Profiles { ids } => {
                let ids: Vec<String> = ids.into_iter().take(500).collect();
                Ok(Reply::Profiles(
                    db::profiles(pool, &ids).await.map_err(|e| e.to_string())?,
                ))
            }
            Op::CreateInvite {
                space,
                role,
                max_uses,
                ttl_secs,
            } => {
                let invite = self
                    .st
                    .log
                    .create_invite(&self.identity, &space, role, max_uses, ttl_secs)
                    .await?;
                self.st.analytics.count("invites_created", 1);
                Ok(Reply::Invite(invite))
            }
            Op::PublishAgreementKey { public, signed } => {
                if !roda_log::profile::verify_agreement(&self.identity, &public, &signed) {
                    return Err("agreement key isn't signed by this identity".into());
                }
                db::put_agreement_key(pool, &self.identity, &public, &signed)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(Reply::Done)
            }
            Op::AgreementKeys { ids } => {
                let ids: Vec<String> = ids.into_iter().take(500).collect();
                Ok(Reply::AgreementKeys(
                    db::agreement_keys(pool, &ids)
                        .await
                        .map_err(|e| e.to_string())?,
                ))
            }
            Op::PutProfile { profile } => {
                if profile.identity != self.identity {
                    return Err("profile is for another identity".into());
                }
                if profile.ciphertext.len() > roda_log::profile::MAX_CIPHERTEXT {
                    return Err("profile too large".into());
                }
                if !roda_log::profile::verify_upload(
                    &profile.identity,
                    profile.version,
                    &profile.ciphertext,
                    &profile.signed,
                ) {
                    return Err("profile isn't signed by this identity".into());
                }
                if !db::put_profile(pool, &profile)
                    .await
                    .map_err(|e| e.to_string())?
                {
                    return Err("stale profile version".into());
                }
                if let Ok(mut to) = self.st.log.co_members(&self.identity).await {
                    to.push(self.identity.clone());
                    self.st.fanout.send(
                        &to,
                        &ServerFrame::ProfileChanged {
                            identity: self.identity.clone(),
                            version: profile.version,
                        },
                        self.hub_id,
                    );
                }
                Ok(Reply::Done)
            }
            Op::PublishKeyPackages {
                packages,
                last_resort,
            } => {
                if packages.len() > 100 {
                    return Err("at most 100 key packages at a time".into());
                }
                let mut publications = Vec::with_capacity(packages.len());
                for kp in &packages {
                    if kp.len() > MAX_KEY_PACKAGE {
                        return Err("key package too large".into());
                    }
                    let (leaf, expires) = roda_mls::key_package_publication(kp)
                        .map_err(|_| "not a valid key package".to_string())?;
                    if leaf.identity != self.identity || leaf.device != self.device {
                        return Err("key package is for another device".into());
                    }
                    publications.push((kp.clone(), expires));
                }
                if let Some(kp) = &last_resort {
                    if kp.len() > MAX_KEY_PACKAGE {
                        return Err("key package too large".into());
                    }
                    let (leaf, _) = roda_mls::key_package_publication(kp)
                        .map_err(|_| "not a valid key package".to_string())?;
                    if leaf.identity != self.identity || leaf.device != self.device {
                        return Err("key package is for another device".into());
                    }
                }
                if !db::put_key_packages(
                    pool,
                    &self.identity,
                    &self.device,
                    &publications,
                    last_resort.as_deref(),
                )
                .await
                .map_err(|e| e.to_string())?
                {
                    return Err("too many key packages stored".into());
                }
                Ok(Reply::Done)
            }
            Op::ClaimKeyPackages { ids } => {
                self.st
                    .limits
                    .lookup_account
                    .check(&self.identity)
                    .map_err(limits::slow_down)?;
                let ids: Vec<String> = ids.into_iter().take(50).collect();
                let claimed = db::claim_key_packages(pool, &ids)
                    .await
                    .map_err(|e| e.to_string())?;
                self.key_packages_low(&claimed).await;
                Ok(Reply::KeyPackages(claimed))
            }
            Op::GetProfiles { ids } => {
                let ids: Vec<String> = ids.into_iter().take(500).collect();
                Ok(Reply::SealedProfiles(
                    db::sealed_profiles(pool, &ids)
                        .await
                        .map_err(|e| e.to_string())?,
                ))
            }
            Op::PreviewInvite { code } => {
                self.st
                    .limits
                    .invite_account
                    .check(&self.identity)
                    .map_err(limits::slow_down)?;
                let i = self.st.log.preview_invite(&code).await?;
                let inviter = db::profiles(pool, &[i.inviter])
                    .await
                    .map_err(|e| e.to_string())?
                    .into_iter()
                    .next();
                Ok(Reply::Preview(roda_proto::InvitePreview {
                    space: i.space,
                    role: i.role,
                    title: i.title,
                    members: i.members,
                    inviter,
                }))
            }
        }
    }

    /// What the metrics learn from a sequenced envelope: its kind and who it went to,
    /// never its content (sealed envelopes only say whether they're application data).
    fn count_sequenced(
        &self,
        env: &roda_proto::Envelope,
        ev: &roda_proto::Sequenced,
        audience: &[String],
        joined: Option<&str>,
        started: std::time::Instant,
    ) {
        let a = &self.st.analytics;
        a.count("publish_ok", 1);
        a.latency("send_ms", started.elapsed());
        let is_message = match ev.env.body() {
            Some(EventBody::MessagePosted { .. }) => true,
            Some(EventBody::SpaceCreated { kind, .. }) => {
                a.count(
                    &format!("spaces_created_{}", format!("{kind:?}").to_lowercase()),
                    1,
                );
                false
            }
            Some(EventBody::RequestResolved { approved, .. }) => {
                a.count(
                    if approved {
                        "approvals_approved"
                    } else {
                        "approvals_denied"
                    },
                    1,
                );
                false
            }
            _ => env.sealed_kind() == Some(roda_proto::SealedKind::Application),
        };
        if is_message {
            a.message_sent(crate::analytics::SentMessage {
                author: &self.identity,
                space: env.space(),
                audience,
            });
        }
        if let (Some(_), Some(who)) = (&env.invite, joined) {
            a.invite_accepted(who);
        }
    }

    #[tracing::instrument(
        name = "publish",
        skip_all,
        fields(
            identity = %pseudo(&self.identity),
            space = %pseudo(env.space()),
            partition = i64::from(crate::ownership::partition_of(env.space())),
            outcome = Empty,
            reason = Empty,
            seq = Empty,
            audience = Empty,
            delivered_here = Empty,
        )
    )]
    async fn publish(&mut self, env: roda_proto::Envelope) {
        let started = std::time::Instant::now();
        let span = tracing::Span::current();
        let analytics = &self.st.analytics;
        let reject = |outcome: &'static str, reason: &str, permanent: bool| {
            span.record("outcome", outcome);
            analytics.count("publish_rejected", 1);
            ServerFrame::Rejected {
                space: env.space().to_string(),
                client_id: env.client_id().to_string(),
                reason: reason.to_string(),
                permanent,
            }
        };
        if let Err(wait) = self.publish_allowed() {
            Metrics::inc(&self.st.metrics.events_rejected);
            return self
                .send(reject("rate_limited", &limits::slow_down(wait), false))
                .await;
        }
        if env.stored_len() > MAX_ENVELOPE {
            Metrics::inc(&self.st.metrics.events_rejected);
            return self.send(reject("too_large", "too large", true)).await;
        }
        if env.author() != self.identity || env.device().is_some_and(|d| d != self.device) {
            Metrics::inc(&self.st.metrics.events_rejected);
            return self
                .send(reject(
                    "wrong_author",
                    "you can only publish as yourself, from this device",
                    true,
                ))
                .await;
        }
        if let Err(e) = env.verify() {
            Metrics::inc(&self.st.metrics.events_rejected);
            return self
                .send(reject("bad_signature", &format!("signature: {e}"), true))
                .await;
        }
        let target_known = match env.body() {
            Some(EventBody::MemberAdded { identity, .. }) => {
                db::is_registered(&self.st.pool, &identity)
                    .await
                    .unwrap_or(false)
            }
            _ => true,
        };
        if !self.st.owner.may_append(env.space()) {
            Metrics::inc(&self.st.metrics.events_rejected);
            return self
                .send(reject(
                    "not_owner",
                    "this relay does not own that Space's partition",
                    false,
                ))
                .await;
        }
        match self.st.log.append(&env, target_known).await {
            Ok(Sequencing::Duplicate { ev }) => {
                Metrics::inc(&self.st.metrics.events_duplicate);
                span.record("outcome", "duplicate");
                span.record("seq", ev.seq as i64);
                self.send(ServerFrame::Accepted {
                    space: env.space().to_string(),
                    client_id: env.client_id().to_string(),
                    seq: ev.seq,
                })
                .await;
                self.send(ServerFrame::Event { ev }).await;
            }
            Ok(Sequencing::New {
                ev,
                audience,
                joined,
            }) => {
                Metrics::inc(&self.st.metrics.events_sequenced);
                span.record("outcome", "sequenced");
                self.count_sequenced(&env, &ev, &audience, joined.as_deref(), started);
                span.record("seq", ev.seq as i64);
                self.send(ServerFrame::Accepted {
                    space: env.space().to_string(),
                    client_id: env.client_id().to_string(),
                    seq: ev.seq,
                })
                .await;
                let space = ev.env.space().to_string();
                let is_create = matches!(ev.env.body(), Some(EventBody::SpaceCreated { .. }));
                let n = self
                    .st
                    .fanout
                    .send(&audience, &ServerFrame::Event { ev }, None);
                span.record("audience", audience.len() as i64);
                span.record("delivered_here", n as i64);
                if let Some(j) = joined {
                    if j != self.identity {
                        self.st.fanout.send(
                            std::slice::from_ref(&j),
                            &ServerFrame::Joined {
                                space: space.clone(),
                            },
                            None,
                        );
                    }
                }
                if is_create {
                    tracing::info!(space = %pseudo(&space), "space created");
                }
            }
            Err(r) => {
                Metrics::inc(&self.st.metrics.events_rejected);
                span.record("reason", r.reason.as_str());
                tracing::info!(space = %pseudo(env.space()), reason = %r.reason, "rejected");
                self.send(reject("rejected", &r.reason, r.permanent)).await;
            }
        }
    }

    #[tracing::instrument(
        name = "sync",
        skip_all,
        fields(identity = %pseudo(&self.identity), all, spaces = Empty, events = Empty)
    )]
    async fn sync(&mut self, cursors: Vec<roda_proto::Cursor>, all: bool) {
        let Ok(Ok(mine)) = tokio::time::timeout(
            Duration::from_secs(5),
            self.st.log.spaces_of(&self.identity),
        )
        .await
        else {
            self.send_sync(ServerFrame::error(
                ErrorCode::Unavailable,
                "database unavailable",
            ))
            .await;
            return;
        };
        // (space, from, through): a member reads to the head; someone removed reads up to
        // and including their removal, so their device learns of it, and nothing after.
        let mut plan: Vec<(String, u64, Option<u64>)> = Vec::new();
        for c in cursors {
            if mine.contains(&c.space) {
                plan.push((c.space, c.next_seq, None));
            } else if let Ok(Ok(Some(gone))) = tokio::time::timeout(
                Duration::from_secs(5),
                self.st.log.removed_at(&c.space, &self.identity),
            )
            .await
            {
                if c.next_seq <= gone {
                    plan.push((c.space, c.next_seq, Some(gone)));
                }
            }
        }
        if all {
            for s in &mine {
                if !plan.iter().any(|(p, ..)| p == s) {
                    plan.push((s.clone(), 0, None));
                }
            }
        }
        let span = tracing::Span::current();
        span.record("spaces", plan.len() as i64);
        let mut sent = 0usize;
        for (space, mut next, through) in plan {
            loop {
                let Ok(Ok(page)) = tokio::time::timeout(
                    Duration::from_secs(5),
                    self.st.log.read(&space, next, SYNC_PAGE),
                )
                .await
                else {
                    self.send_sync(ServerFrame::error(
                        ErrorCode::Unavailable,
                        "log store unavailable",
                    ))
                    .await;
                    return;
                };
                let n = page.len();
                for ev in page {
                    if through.is_some_and(|last| ev.seq > last) {
                        break;
                    }
                    next = ev.seq + 1;
                    Metrics::inc(&self.st.metrics.sync_events_sent);
                    sent += 1;
                    if !self.send_sync(ServerFrame::Event { ev }).await {
                        return;
                    }
                }
                if n < SYNC_PAGE || through.is_some_and(|last| next > last) {
                    break;
                }
            }
        }
        span.record("events", sent as i64);
        if !self.send_sync(ServerFrame::SyncDone).await {
            return;
        }
        self.st.analytics.synced(&self.identity);
        if !self.first_sync_done {
            self.first_sync_done = true;
            self.st
                .analytics
                .latency("first_sync_ms", self.started.elapsed());
        }
    }
}

fn valid_hex64(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}
