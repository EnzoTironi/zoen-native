//! Fan-out across sync nodes (ADR 0019, S5).
//!
//! Every frame a session should see goes through [`Fanout::send`]: the local [`Hub`] delivers
//! to sessions on this node, and the [`Bus`] carries the same frame to the other nodes. With
//! one node the bus is [`LocalBus`] and does nothing. With several it is [`NatsBus`]: every
//! identity with a session on a node is a NATS subscription on that node, so a publish reaches
//! exactly the nodes where the recipient is connected (interest-based routing, O(edges
//! interested)). Delivery is at most once; a device that misses a frame catches up with
//! `Sync` by cursor, the same as after a reconnect.

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering::Relaxed},
        Arc, Mutex,
    },
    time::Duration,
};

use async_nats::{header::HeaderMap, Client};
use bytes::Bytes;
use futures_util::StreamExt;
use roda_proto::ServerFrame;
use sha2::{Digest, Sha256};
use tokio::{sync::mpsc, task::JoinHandle};

use crate::hub::{Hub, Mailbox};

const ORIGIN: &str = "Zoen-Origin";
const KIND: &str = "Zoen-Kind";
const PING: &str = "ping";
const REVOKE: &str = "revoke-device";
const OUTBOUND_QUEUE: usize = 65_536;
const PRESENCE_TIMEOUT: Duration = Duration::from_millis(750);

#[async_trait::async_trait]
pub trait Bus: Send + Sync {
    fn kind(&self) -> &'static str;
    /// One more session of `identity` on this node.
    fn attach(&self, identity: &str);
    /// One session of `identity` on this node went away.
    fn detach(&self, identity: &str);
    /// Carries `frame` to the sessions of `to` on every other node.
    fn publish(&self, to: &[String], frame: &ServerFrame);
    async fn revoke_device(&self, identity: &str, device: &str) -> anyhow::Result<()>;
    /// Which of `ids` have a session on some other node.
    async fn online_elsewhere(&self, ids: &[String]) -> Vec<String>;
    fn stats(&self) -> BusStats {
        BusStats::default()
    }
}

#[derive(Default, Clone, Copy)]
pub struct BusStats {
    pub published: u64,
    pub received: u64,
    pub dropped: u64,
}

pub struct Fanout {
    pub hub: Arc<Hub>,
    pub node: String,
    bus: Box<dyn Bus>,
}

impl Fanout {
    pub fn local(node: String) -> Self {
        Self {
            hub: Arc::new(Hub::default()),
            node,
            bus: Box::new(LocalBus),
        }
    }

    pub async fn nats(node: String, url: &str, cell: &str) -> anyhow::Result<Self> {
        let hub = Arc::new(Hub::default());
        let bus = NatsBus::connect(url, cell, &node, hub.clone()).await?;
        Ok(Self {
            hub,
            node,
            bus: Box::new(bus),
        })
    }

    pub fn bus_kind(&self) -> &'static str {
        self.bus.kind()
    }

    pub fn add(&self, identity: &str, mailbox: Mailbox) -> u64 {
        let id = self.hub.add(identity, mailbox);
        self.bus.attach(identity);
        id
    }

    /// Returns true when this was the identity's last session on this node.
    pub fn remove(&self, identity: &str, id: u64) -> bool {
        let last = self.hub.remove(identity, id);
        self.bus.detach(identity);
        last
    }

    /// Delivers to every session of `to` on every node, except local session `skip`.
    /// Returns how many local sessions took it.
    pub fn send(&self, to: &[String], frame: &ServerFrame, skip: Option<u64>) -> usize {
        let n = self.hub.send(to, frame, skip);
        self.bus.publish(to, frame);
        n
    }

    /// The durable revocation commits before this invalidation. Receivers also check
    /// that record at authorization boundaries, so a missed bus notice cannot grant access.
    pub async fn revoke_device(&self, identity: &str, device: &str) {
        self.hub.revoke_device(identity, device);
        match tokio::time::timeout(
            Duration::from_secs(2),
            self.bus.revoke_device(identity, device),
        )
        .await
        {
            Ok(Ok(())) => {}
            Ok(Err(error)) => tracing::warn!(%error, "live device revocation broadcast failed"),
            Err(_) => tracing::warn!("live device revocation broadcast timed out"),
        }
    }

    pub async fn is_online(&self, identity: &str) -> bool {
        !self
            .online(std::slice::from_ref(&identity.to_string()))
            .await
            .is_empty()
    }

    /// The subset of `ids` with a session on any node.
    pub async fn online(&self, ids: &[String]) -> Vec<String> {
        let (mut here, away): (Vec<String>, Vec<String>) =
            ids.iter().cloned().partition(|i| self.hub.is_online(i));
        here.extend(self.bus.online_elsewhere(&away).await);
        here
    }

    pub fn render_metrics(&self) -> String {
        let s = self.bus.stats();
        let mut out = String::new();
        for (name, help, v) in [
            (
                "zoen_relay_bus_published_total",
                "Frames published to other nodes",
                s.published,
            ),
            (
                "zoen_relay_bus_received_total",
                "Frames received from other nodes",
                s.received,
            ),
            (
                "zoen_relay_bus_dropped_total",
                "Frames dropped because the outbound queue was full",
                s.dropped,
            ),
        ] {
            out.push_str(&format!(
                "# HELP {name} {help}\n# TYPE {name} counter\n{name} {v}\n"
            ));
        }
        out
    }
}

/// One node: the hub is the whole world.
pub struct LocalBus;

#[async_trait::async_trait]
impl Bus for LocalBus {
    fn kind(&self) -> &'static str {
        "local"
    }
    fn attach(&self, _: &str) {}
    fn detach(&self, _: &str) {}
    fn publish(&self, _: &[String], _: &ServerFrame) {}
    async fn revoke_device(&self, _: &str, _: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn online_elsewhere(&self, _: &[String]) -> Vec<String> {
        Vec::new()
    }
}

/// Subjects are `zoen.<cell>.to.<sha256("zoen-subject-v1" ‖ identity)[..16] hex>`: NATS sees a
/// pseudonym, never an identity, and a cell never hears another cell. Presence is the same
/// subscription answering a ping: "no responders" means nobody else has that identity online.
pub struct NatsBus {
    client: Client,
    node: String,
    prefix: String,
    hub: Arc<Hub>,
    subs: Mutex<HashMap<String, (usize, JoinHandle<()>)>>,
    out: mpsc::Sender<Outbound>,
    stats: Arc<Counters>,
}

/// A frame for other nodes: its recipients, its bytes, and the publisher's trace context.
struct Outbound {
    to: Vec<String>,
    bytes: Bytes,
    trace: Vec<(String, String)>,
}

#[derive(Default)]
struct Counters {
    published: AtomicU64,
    received: AtomicU64,
    dropped: AtomicU64,
}

impl NatsBus {
    pub async fn connect(url: &str, cell: &str, node: &str, hub: Arc<Hub>) -> anyhow::Result<Self> {
        let client = async_nats::ConnectOptions::new()
            .name(format!("zoen-relay {node}"))
            .request_timeout(Some(PRESENCE_TIMEOUT))
            .retry_on_initial_connect()
            .connect(url)
            .await?;
        let prefix = format!("zoen.{}.to", subject_token(cell));
        let stats = Arc::new(Counters::default());
        let (out, rx) = mpsc::channel(OUTBOUND_QUEUE);
        tokio::spawn(publisher(
            client.clone(),
            prefix.clone(),
            node.to_string(),
            rx,
            stats.clone(),
        ));
        Ok(Self {
            client,
            node: node.to_string(),
            prefix,
            hub,
            subs: Mutex::new(HashMap::new()),
            out,
            stats,
        })
    }

    fn listen(&self, identity: &str) -> JoinHandle<()> {
        let client = self.client.clone();
        let subject = subject_for(&self.prefix, identity);
        let identity = identity.to_string();
        let node = self.node.clone();
        let hub = self.hub.clone();
        let stats = self.stats.clone();
        tokio::spawn(async move {
            let mut sub = match client.subscribe(subject).await {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(error = %e, "bus subscribe failed");
                    return;
                }
            };
            while let Some(m) = sub.next().await {
                let header = |k: &str| {
                    m.headers
                        .as_ref()
                        .and_then(|h| h.get(k))
                        .map(|v| v.as_str().to_string())
                };
                if header(KIND).as_deref() == Some(PING) {
                    if let Some(reply) = m.reply.clone() {
                        let _ = client.publish(reply, Bytes::new()).await;
                    }
                    continue;
                }
                if header(ORIGIN).as_deref() == Some(node.as_str()) {
                    continue;
                }
                if header(KIND).as_deref() == Some(REVOKE) {
                    if let Ok(device) = std::str::from_utf8(&m.payload) {
                        if device.len() == 64 && device.bytes().all(|b| b.is_ascii_hexdigit()) {
                            hub.revoke_device(&identity, device);
                        }
                    }
                    continue;
                }
                match ServerFrame::decode(&m.payload) {
                    Ok(frame) => {
                        stats.received.fetch_add(1, Relaxed);
                        let span = tracing::info_span!(
                            "bus.deliver",
                            otel.kind = "consumer",
                            delivered_here = tracing::field::Empty,
                        );
                        if let Some(h) = &m.headers {
                            crate::telemetry::adopt_remote_parent(&span, &NatsHeaders(h));
                        }
                        let n = hub.send(std::slice::from_ref(&identity), &frame, None);
                        span.record("delivered_here", n as i64);
                    }
                    Err(e) => tracing::warn!(error = %e, "undecodable frame on the bus"),
                }
            }
        })
    }
}

#[async_trait::async_trait]
impl Bus for NatsBus {
    fn kind(&self) -> &'static str {
        "nats"
    }

    fn attach(&self, identity: &str) {
        let mut subs = self.subs.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = subs.get_mut(identity) {
            entry.0 += 1;
            return;
        }
        let task = self.listen(identity);
        subs.insert(identity.to_string(), (1, task));
    }

    fn detach(&self, identity: &str) {
        let mut subs = self.subs.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = subs.get_mut(identity) {
            entry.0 -= 1;
            if entry.0 == 0 {
                if let Some((_, task)) = subs.remove(identity) {
                    task.abort();
                }
            }
        }
    }

    fn publish(&self, to: &[String], frame: &ServerFrame) {
        if to.is_empty() {
            return;
        }
        let out = Outbound {
            to: to.to_vec(),
            bytes: Bytes::from(frame.encode()),
            trace: crate::telemetry::current_context_headers(),
        };
        if self.out.try_send(out).is_err() {
            self.stats.dropped.fetch_add(1, Relaxed);
        }
    }

    async fn revoke_device(&self, identity: &str, device: &str) -> anyhow::Result<()> {
        let mut headers = HeaderMap::new();
        headers.insert(ORIGIN, self.node.as_str());
        headers.insert(KIND, REVOKE);
        self.client
            .publish_with_headers(
                subject_for(&self.prefix, identity),
                headers,
                Bytes::copy_from_slice(device.as_bytes()),
            )
            .await?;
        self.client.flush().await?;
        Ok(())
    }

    async fn online_elsewhere(&self, ids: &[String]) -> Vec<String> {
        let asks = ids.iter().map(|id| {
            let client = self.client.clone();
            let subject = subject_for(&self.prefix, id);
            let id = id.clone();
            async move {
                let mut h = HeaderMap::new();
                h.insert(KIND, PING);
                match client.request_with_headers(subject, h, Bytes::new()).await {
                    Ok(_) => Some(id),
                    Err(_) => None,
                }
            }
        });
        futures_util::future::join_all(asks)
            .await
            .into_iter()
            .flatten()
            .collect()
    }

    fn stats(&self) -> BusStats {
        BusStats {
            published: self.stats.published.load(Relaxed),
            received: self.stats.received.load(Relaxed),
            dropped: self.stats.dropped.load(Relaxed),
        }
    }
}

/// Reads W3C trace context out of a bus message's headers.
struct NatsHeaders<'a>(&'a HeaderMap);

impl opentelemetry::propagation::Extractor for NatsHeaders<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(|v| v.as_str())
    }

    fn keys(&self) -> Vec<&str> {
        self.0.iter().map(|(k, _)| k.as_ref()).collect()
    }
}

async fn publisher(
    client: Client,
    prefix: String,
    node: String,
    mut rx: mpsc::Receiver<Outbound>,
    stats: Arc<Counters>,
) {
    while let Some(Outbound { to, bytes, trace }) = rx.recv().await {
        for id in to {
            let mut h = HeaderMap::new();
            h.insert(ORIGIN, node.as_str());
            for (k, v) in &trace {
                h.insert(k.as_str(), v.as_str());
            }
            match client
                .publish_with_headers(subject_for(&prefix, &id), h, bytes.clone())
                .await
            {
                Ok(()) => {
                    stats.published.fetch_add(1, Relaxed);
                }
                Err(_) => {
                    stats.dropped.fetch_add(1, Relaxed);
                }
            }
        }
    }
}

fn subject_for(prefix: &str, identity: &str) -> String {
    let mut h = Sha256::new();
    h.update(b"zoen-subject-v1\0");
    h.update(identity.as_bytes());
    format!("{prefix}.{}", hex::encode(&h.finalize()[..16]))
}

fn subject_token(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// A fresh node id per process: two relays behind one public name are still two nodes.
pub fn new_node_id() -> String {
    let mut b = [0u8; 16];
    getrandom::getrandom(&mut b).expect("os randomness");
    format!("n{}", hex::encode(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subjects_are_pseudonymous_and_cell_scoped() {
        let a = subject_for("zoen.c0.to", "id_ana");
        assert!(a.starts_with("zoen.c0.to."));
        assert!(!a.contains("ana"));
        assert_eq!(a, subject_for("zoen.c0.to", "id_ana"));
        assert_ne!(a, subject_for("zoen.c1.to", "id_ana"));
        assert_eq!(subject_token("zoen t.1"), "zoen_t_1");
    }
}
