//! A causal authentication barrier for the existing certified socket fixture.
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Notify;

#[derive(Default)]
pub(in crate::agent::source_mode::tests) struct AuthGate {
    held: AtomicBool,
    reached: Notify,
    released: Notify,
}
impl AuthGate {
    pub(in crate::agent::source_mode::tests) fn hold(&self) {
        self.held.store(true, Ordering::SeqCst);
    }
    pub(in crate::agent::source_mode::tests) async fn reached(&self) {
        self.reached.notified().await;
    }
    pub(in crate::agent::source_mode::tests) fn release(&self) {
        self.held.store(false, Ordering::SeqCst);
        self.released.notify_one();
    }
    pub(super) async fn before_ready(&self) {
        if self.held.load(Ordering::SeqCst) {
            self.reached.notify_one();
            while self.held.load(Ordering::SeqCst) {
                self.released.notified().await;
            }
        }
    }
}
