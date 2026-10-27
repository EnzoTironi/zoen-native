//! Count and delay the actual checkpoint group read; never substitute its result.
use std::{cell::Cell, time::Duration};

thread_local! {
    static DELAY: Cell<Duration> = const { Cell::new(Duration::ZERO) };
    static READS: Cell<usize> = const { Cell::new(0) };
}

pub(crate) struct Probe;
impl Probe {
    pub(crate) fn start(delay: Duration) -> Self {
        DELAY.with(|value| value.set(delay));
        READS.with(|value| value.set(0));
        Self
    }
    pub(crate) fn reads(&self) -> usize {
        READS.with(Cell::get)
    }
}
impl Drop for Probe {
    fn drop(&mut self) {
        DELAY.with(|value| value.set(Duration::ZERO));
        READS.with(|value| value.set(0));
    }
}

pub(super) fn before_group_read() {
    READS.with(|value| value.set(value.get() + 1));
    let delay = DELAY.with(Cell::get);
    if !delay.is_zero() {
        std::thread::sleep(delay);
    }
}
