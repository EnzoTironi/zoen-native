//! Explicit loss injection used only by the downstream-loss CLI journey.
//! The relay's publisher-refusal test reuses the observer without this API.
use super::final_event_proxy::{EventTarget, FinalEventProxy, Selector};

impl FinalEventProxy {
    /// Unique synthetic plaintext selector avoids a race with the automatic poke.
    /// The trace records the selected signed client_id/seq/hash once consumed.
    pub fn arm(&self, space: &str, author: &str, text: &str) -> usize {
        self.arm_target(space, author, EventTarget::PlainText(text.into()))
    }

    #[allow(dead_code, reason = "used by the forwarding restart journey")]
    pub fn arm_client_id(&self, space: &str, author: &str, id: &str) -> usize {
        self.arm_target(space, author, EventTarget::ClientId(id.into()))
    }

    fn arm_target(&self, space: &str, author: &str, target: EventTarget) -> usize {
        let mut state = self.state.lock().unwrap();
        assert!(state.selector.is_none(), "one exact fault per test");
        let checkpoint = state.records.len();
        state.selector = Some(Selector {
            space: space.into(),
            author: author.into(),
            target,
            consumed: false,
        });
        checkpoint
    }
}
