//! Counts real group reads; injected latency never substitutes their result.
use std::{cell::Cell, time::Duration};

thread_local! {
    static DOWNLOAD: Cell<usize> = const { Cell::new(0) };
    static RECOVERY: Cell<usize> = const { Cell::new(0) };
    static LATENCY: Cell<Duration> = const { Cell::new(Duration::ZERO) };
}

pub(crate) fn reset(latency: Duration) {
    DOWNLOAD.set(0);
    RECOVERY.set(0);
    LATENCY.set(latency);
}

pub(super) fn before_download_read() {
    DOWNLOAD.set(DOWNLOAD.get() + 1);
    std::thread::sleep(LATENCY.get());
}

pub(super) fn before_recovery_read() {
    RECOVERY.set(RECOVERY.get() + 1);
    std::thread::sleep(LATENCY.get());
}

pub(crate) fn reads() -> (usize, usize) {
    (DOWNLOAD.get(), RECOVERY.get())
}
