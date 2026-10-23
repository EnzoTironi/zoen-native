//! Keep the service-journey state machines small enough to leave room for
//! nested polling on the ordinary Linux test-thread stack. No future is polled
//! or even constructed to measure its compiler-chosen layout.
use super::*;
use std::{future::Future, mem::size_of};

fn frame<F: Future>(_: impl FnOnce() -> F) -> usize {
    size_of::<F>()
}

#[test]
fn retained_runtime_async_frames_are_bounded() {
    let frames = [
        ("journeys", frame(journeys::run)),
        ("image finance", frame(image_tasks::run)),
        ("managed custody", frame(managed_custody::run)),
        ("retained", frame(retained::run)),
        ("retained output", frame(retained::output::run)),
    ];
    // One eighth of the ordinary 2MiB Linux test stack, leaving room for
    // callers, nested poll frames, decoders and the executor. This is a frame
    // regression limit, not a claim that the entire call stack is bounded.
    const MAX_FRAME_BYTES: usize = 256 * 1024;
    assert!(
        frames.iter().all(|(_, bytes)| *bytes <= MAX_FRAME_BYTES),
        "oversized async frames (bytes): {frames:?}; split sequential journeys \
         or box the large child future at its ownership boundary; do not raise \
         the process-wide stack or remove journey assertions"
    );
}
