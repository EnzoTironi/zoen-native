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
use protocol::ServerFrame;
use sha2::{Digest, Sha256};
use tokio::{sync::mpsc, task::JoinHandle};

use crate::hub::{Hub, Mailbox};

mod nats;
mod presence;
pub use nats::NatsBus;
use presence::collect_presence;

const ORIGIN: &str = "Zoen-Origin";
const KIND: &str = "Zoen-Kind";
const PING: &str = "ping";
const REVOKE: &str = "revoke-device";
const OUTBOUND_QUEUE: usize = 65_536;
const PRESENCE_TIMEOUT: Duration = Duration::from_millis(750);
const PRESENCE_CONCURRENCY: usize = 32;
// Presence is best effort and must not consume login's entire 5s go_online budget.
const PRESENCE_LOOKUP_TIMEOUT: Duration = Duration::from_secs(2);

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
    /// Authors of the last messages per Space delivered through this node (receipt routing).
    pub recent: Arc<crate::recent::RecentAuthors>,
    pub node: String,
    bus: Box<dyn Bus>,
}

impl Fanout {
    pub fn local(node: String) -> Self {
        Self {
            hub: Arc::new(Hub::default()),
            recent: Arc::default(),
            node,
            bus: Box::new(LocalBus),
        }
    }

    pub async fn nats(node: String, url: &str, cell: &str) -> anyhow::Result<Self> {
        let hub = Arc::new(Hub::default());
        let recent: Arc<crate::recent::RecentAuthors> = Arc::default();
        let bus = NatsBus::connect(url, cell, &node, hub.clone(), recent.clone()).await?;
        Ok(Self {
            hub,
            recent,
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
        remember(&self.recent, frame);
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

    /// A synchronous local check for admission paths that must not wait on NATS.
    pub fn is_online_here(&self, identity: &str) -> bool {
        self.hub.is_online(identity)
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

fn remember(recent: &crate::recent::RecentAuthors, frame: &ServerFrame) {
    if let ServerFrame::Event { ev } = frame {
        recent.record(ev.env.space(), ev.seq, ev.env.author());
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

/// A fresh node id per process: two relays behind one public name are still two nodes.
pub fn new_node_id() -> String {
    let mut b = [0u8; 16];
    getrandom::getrandom(&mut b).expect("os randomness");
    format!("n{}", hex::encode(b))
}

#[cfg(test)]
mod tests;
