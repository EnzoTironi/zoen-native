//! Explicit loss injection used only by the downstream-loss CLI journey.
//! The relay's publisher-refusal test reuses the observer without this API.
use super::final_event_proxy::{FinalEventProxy, Selector};

impl FinalEventProxy {
    /// Unique synthetic plaintext selector avoids a race with the automatic poke.
    /// The trace records the selected signed client_id/seq/hash once consumed.
    pub fn arm(&self, space: &str, author: &str, text: &str) -> usize {
        let mut state = self.state.lock().unwrap();
        assert!(state.selector.is_none(), "one exact fault per test");
        let checkpoint = state.records.len();
        state.selector = Some(Selector {
            space: space.into(),
            author: author.into(),
            text: text.into(),
            consumed: false,
        });
        checkpoint
    }
}
