//! Explicit loss injection used only by the downstream-loss CLI journey.
//! The relay's publisher-refusal test reuses the observer without this API.
use super::final_event_proxy::{FinalEventProxy, Selector};

impl FinalEventProxy {
    /// Unique synthetic plaintext selector avoids a race with the automatic poke.
    /// The trace records the selected signed client_id/seq/hash once consumed.
    pub fn arm(&self, space: &str, author: &str, text: &str) -> usize {
        let expected = text.to_owned();
        self.arm_target(space, author, move |ev| {
            ev.plain_event().is_some_and(|event| {
                matches!(event.body, types::EventBody::MessagePosted { text, .. } if text == expected)
            })
        })
    }

    #[allow(dead_code, reason = "used by the forwarding restart journey")]
    pub fn arm_client_id(&self, space: &str, author: &str, id: &str) -> usize {
        let id = id.to_owned();
        self.arm_target(space, author, move |ev| ev.env.client_id() == id)
    }

    fn arm_target(
        &self,
        space: &str,
        author: &str,
        target: impl Fn(&protocol::Sequenced) -> bool + Send + Sync + 'static,
    ) -> usize {
        let mut state = self.state.lock().unwrap();
        assert!(state.selector.is_none(), "one exact fault per test");
        let checkpoint = state.records.len();
        state.selector = Some(Selector {
            space: space.into(),
            author: author.into(),
            target: Box::new(target),
            consumed: false,
        });
        checkpoint
    }
}
