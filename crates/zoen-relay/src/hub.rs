//! Who is online, and how to reach them. One relay node keeps this in memory; with
//! several nodes, `fanout::Fanout` adds the NATS bus on top (ADR 0019).

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, RwLock,
    },
};

use tokio::sync::{mpsc, Notify};
use zoen_proto::ServerFrame;

pub type Outbox = mpsc::Sender<ServerFrame>;

/// A session's mailbox plus a kick: a device that can't keep up is disconnected, so it
/// reconnects and catches up with Sync instead of silently missing an event.
#[derive(Clone)]
pub struct Mailbox {
    pub tx: Outbox,
    pub kick: Arc<Notify>,
    pub device: String,
    pub revoked: Arc<AtomicBool>,
}

#[derive(Default)]
pub struct Hub {
    next: AtomicU64,
    sessions: RwLock<HashMap<String, HashMap<u64, Mailbox>>>,
}

impl Hub {
    pub fn add(&self, identity: &str, tx: Mailbox) -> u64 {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        self.sessions
            .write()
            .expect("hub")
            .entry(identity.to_string())
            .or_default()
            .insert(id, tx);
        id
    }

    /// Returns true when this was the identity's last session.
    pub fn remove(&self, identity: &str, id: u64) -> bool {
        let mut map = self.sessions.write().expect("hub");
        if let Some(s) = map.get_mut(identity) {
            s.remove(&id);
            if s.is_empty() {
                map.remove(identity);
                return true;
            }
        }
        false
    }

    pub fn is_online(&self, identity: &str) -> bool {
        self.sessions
            .read()
            .expect("hub")
            .get(identity)
            .is_some_and(|sessions| {
                sessions
                    .values()
                    .any(|mailbox| !mailbox.revoked.load(Ordering::Acquire))
            })
    }

    pub fn online_sessions(&self) -> usize {
        self.sessions
            .read()
            .expect("hub")
            .values()
            .map(|s| {
                s.values()
                    .filter(|mailbox| !mailbox.revoked.load(Ordering::Acquire))
                    .count()
            })
            .sum()
    }

    /// Sends to every session of every identity in `to`, except session `skip`.
    /// A session whose buffer is full gets kicked (it reconnects and syncs).
    pub fn send(&self, to: &[String], frame: &ServerFrame, skip: Option<u64>) -> usize {
        let map = self.sessions.read().expect("hub");
        let mut n = 0;
        for who in to {
            if let Some(s) = map.get(who) {
                for (id, tx) in s {
                    if Some(*id) == skip {
                        continue;
                    }
                    if tx.revoked.load(Ordering::Acquire) {
                        continue;
                    }
                    // Slow consumers must not force a deep frame clone just to discard it.
                    match tx.tx.try_reserve() {
                        Ok(permit) => {
                            permit.send(frame.clone());
                            n += 1;
                        }
                        Err(mpsc::error::TrySendError::Full(_)) => tx.kick.notify_one(),
                        Err(mpsc::error::TrySendError::Closed(_)) => {}
                    }
                }
            }
        }
        n
    }

    pub fn revoke_device(&self, identity: &str, device: &str) {
        if let Some(sessions) = self.sessions.read().expect("hub").get(identity) {
            for mailbox in sessions.values().filter(|mailbox| mailbox.device == device) {
                mailbox.revoked.store(true, Ordering::Release);
                mailbox.kick.notify_waiters();
                mailbox.kick.notify_one();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn full_mailbox_kicks_without_blocking_other_sessions() {
        let hub = Hub::default();
        let (slow, _slow_rx) = mpsc::channel(1);
        let (fast, mut fast_rx) = mpsc::channel(1);
        let kick = Arc::new(Notify::new());
        let frame = ServerFrame::Presence {
            identity: "alice".into(),
            online: true,
        };
        slow.try_send(frame.clone()).unwrap();
        for (tx, notify) in [(slow, kick.clone()), (fast, Arc::new(Notify::new()))] {
            hub.add(
                "alice",
                Mailbox {
                    tx,
                    kick: notify,
                    device: "device".into(),
                    revoked: Arc::new(AtomicBool::new(false)),
                },
            );
        }
        assert_eq!(hub.send(&["alice".into()], &frame, None), 1);
        assert!(fast_rx.try_recv().is_ok());
        tokio::time::timeout(std::time::Duration::from_millis(100), kick.notified())
            .await
            .unwrap();
    }
}
