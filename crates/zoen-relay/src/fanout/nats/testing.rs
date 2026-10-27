//! Closed unit-test controls: stop the real publisher after dequeue, not enqueue.
//! No queue limit, frame, storage result, clock or subscriber ACK is fabricated.
use super::*;
use std::sync::atomic::{AtomicBool, Ordering::Acquire, Ordering::Release};
use tokio::{sync::Notify, task::AbortHandle};

#[derive(Default)]
pub struct PublisherControl {
    blocked: AtomicBool,
    held: AtomicBool,
    changed: Notify,
    resume: Notify,
    task: Mutex<Option<AbortHandle>>,
    selector: Mutex<Option<(String, String)>>,
    refused: Mutex<Option<protocol::Sequenced>>,
}

impl PublisherControl {
    pub(super) fn own(&self, task: AbortHandle) {
        *self.task.lock().expect("test publisher task lock") = Some(task);
    }

    pub(super) async fn before_publish(&self) {
        if !self.blocked.load(Acquire) {
            return;
        }
        self.held.store(true, Release);
        self.changed.notify_waiters();
        loop {
            let resumed = self.resume.notified();
            if !self.blocked.load(Acquire) {
                break;
            }
            resumed.await;
        }
        self.held.store(false, Release);
    }

    pub(super) fn refused(&self, frame: &ServerFrame) {
        let ServerFrame::Event { ev } = frame else {
            return;
        };
        let selector = self.selector.lock().expect("test selector lock");
        if selector
            .as_ref()
            .is_some_and(|(space, author)| ev.env.space() == space && ev.env.author() == author)
        {
            let mut refused = self.refused.lock().expect("test refusal lock");
            assert!(refused.is_none(), "only one selected final event may fail");
            *refused = Some(ev.clone());
            self.changed.notify_waiters();
        }
    }

    fn retire(&self) {
        if let Some(task) = self.task.lock().expect("test publisher task lock").take() {
            task.abort();
        }
        self.blocked.store(false, Release);
        self.resume.notify_waiters();
    }
}

pub struct PublisherProbe {
    control: Arc<PublisherControl>,
    out: mpsc::Sender<Outbound>,
    budget: Arc<Semaphore>,
    stats: Arc<Counters>,
}

impl PublisherProbe {
    pub(super) fn new(bus: &NatsBus) -> Self {
        Self {
            control: bus.publisher_control.clone(),
            out: bus.out.clone(),
            budget: bus.outbound_bytes.clone(),
            stats: bus.stats.clone(),
        }
    }

    pub fn pause(&self, space: &str, author: &str) {
        *self.control.selector.lock().expect("test selector lock") =
            Some((space.into(), author.into()));
        self.control.blocked.store(true, Release);
    }

    pub async fn held(&self) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let changed = self.control.changed.notified();
                if self.control.held.load(Acquire) {
                    return;
                }
                changed.await;
            }
        })
        .await
        .expect("publisher must hold the actually dequeued pressure item");
    }

    pub fn capacity(&self) -> usize {
        self.out.capacity()
    }

    pub fn retained_bytes(&self) -> usize {
        outbound::MAX_BYTES - self.budget.available_permits()
    }

    pub fn published(&self) -> u64 {
        self.stats.published.load(Relaxed)
    }

    pub fn dropped(&self) -> u64 {
        self.stats.dropped.load(Relaxed)
    }

    pub fn refused_event(&self) -> Option<protocol::Sequenced> {
        self.control
            .refused
            .lock()
            .expect("test refusal lock")
            .clone()
    }
}

impl Drop for PublisherProbe {
    fn drop(&mut self) {
        // Owns only this instance's paused publisher. Dropping its receiver frees
        // real queued frames/credits instead of sending pressure to the broker.
        self.control.retire();
    }
}
