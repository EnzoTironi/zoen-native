//! Prometheus text metrics with plain atomics (no content, ever: counts only).

use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

#[derive(Default)]
pub struct Metrics {
    pub connections: AtomicU64,
    pub events_sequenced: AtomicU64,
    pub events_duplicate: AtomicU64,
    pub events_rejected: AtomicU64,
    pub ephemeral_forwarded: AtomicU64,
    pub sync_events_sent: AtomicU64,
    pub blobs_stored: AtomicU64,
    pub blob_bytes_stored: AtomicU64,
    pub upgrade_required: AtomicU64,
}

impl Metrics {
    pub fn inc(c: &AtomicU64) {
        c.fetch_add(1, Relaxed);
    }

    pub fn blob_stored(&self, bytes: usize) {
        self.blobs_stored.fetch_add(1, Relaxed);
        self.blob_bytes_stored.fetch_add(bytes as u64, Relaxed);
    }

    pub fn render(&self, online: usize) -> String {
        let mut s = String::new();
        let mut put = |name: &str, help: &str, kind: &str, v: u64| {
            s.push_str(&format!(
                "# HELP {name} {help}\n# TYPE {name} {kind}\n{name} {v}\n"
            ));
        };
        put(
            "zoen_relay_connections_total",
            "WebSocket sessions opened",
            "counter",
            self.connections.load(Relaxed),
        );
        put(
            "zoen_relay_sessions_online",
            "Authenticated sessions online now",
            "gauge",
            online as u64,
        );
        put(
            "zoen_relay_events_sequenced_total",
            "Envelopes sequenced",
            "counter",
            self.events_sequenced.load(Relaxed),
        );
        put(
            "zoen_relay_events_duplicate_total",
            "Retries answered idempotently",
            "counter",
            self.events_duplicate.load(Relaxed),
        );
        put(
            "zoen_relay_events_rejected_total",
            "Envelopes rejected",
            "counter",
            self.events_rejected.load(Relaxed),
        );
        put(
            "zoen_relay_ephemeral_forwarded_total",
            "Typing/presence signals forwarded",
            "counter",
            self.ephemeral_forwarded.load(Relaxed),
        );
        put(
            "zoen_relay_sync_events_sent_total",
            "Events sent during catch-up",
            "counter",
            self.sync_events_sent.load(Relaxed),
        );
        put(
            "zoen_relay_blobs_stored_total",
            "Encrypted blobs stored",
            "counter",
            self.blobs_stored.load(Relaxed),
        );
        put(
            "zoen_relay_blob_bytes_stored_total",
            "Bytes of encrypted blobs stored",
            "counter",
            self.blob_bytes_stored.load(Relaxed),
        );
        put(
            "zoen_relay_upgrade_required_total",
            "Handshakes refused because the client is too old",
            "counter",
            self.upgrade_required.load(Relaxed),
        );
        s
    }
}
