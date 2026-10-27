//! Bounded receipts for a pure author check over exact complete retained originals.

use std::collections::VecDeque;
use types::Event;

const MAX_ENTRIES: usize = 256;
const MAX_BYTES: usize = 8 * 1024 * 1024;

#[cfg(test)]
#[path = "verification_tests.rs"]
mod tests;

struct Receipt {
    client_id: String,
    json: String,
}

impl Receipt {
    fn bytes(&self) -> usize {
        self.client_id.capacity() + self.json.capacity()
    }
}

#[derive(Default)]
pub(crate) struct VerificationCache {
    receipts: VecDeque<Receipt>,
    bytes: usize,
    #[cfg(test)]
    checks: usize,
}

impl VerificationCache {
    pub(super) fn read_original(
        &mut self,
        head: &store::OutboxHead,
        json: String,
    ) -> Option<Event> {
        let event = original(head, &json)?;
        self.verify(&event, json).then_some(event)
    }

    pub(super) fn read_uncached(head: &store::OutboxHead, json: &str) -> Option<Event> {
        original(head, json).filter(|event| event_log::verify_author(event).is_ok())
    }

    fn verify(&mut self, event: &Event, json: String) -> bool {
        if let Some(index) = self
            .receipts
            .iter()
            .position(|receipt| receipt.client_id == event.client_id)
        {
            // read_original freshly parsed this JSON and checked the head.
            // Equal bytes mean exactly the already verified complete
            // Event, including all decoded views, content, signature and certificate.
            if self.receipts[index].json == json {
                return true;
            }
            if let Some(previous) = self.receipts.remove(index) {
                self.bytes -= previous.bytes();
            }
        }
        #[cfg(test)]
        {
            self.checks += 1;
        }
        if event_log::verify_author(event).is_err() {
            return false;
        }
        if json.capacity().saturating_add(event.client_id.len()) > MAX_BYTES {
            return true;
        }
        let receipt = Receipt {
            client_id: event.client_id.clone(),
            json,
        };
        let bytes = receipt.bytes();
        // Large originals remain valid only through the ordinary author check.
        // The budget counts actual retained String allocations, not payload estimates.
        if bytes > MAX_BYTES {
            return true;
        }
        while self.receipts.len() >= MAX_ENTRIES || self.bytes + bytes > MAX_BYTES {
            let Some(evicted) = self.receipts.pop_front() else {
                break;
            };
            self.bytes -= evicted.bytes();
        }
        self.bytes += bytes;
        self.receipts.push_back(receipt);
        true
    }

    pub(crate) fn clear(&mut self) {
        self.receipts.clear();
        self.bytes = 0;
    }
}

fn original(head: &store::OutboxHead, json: &str) -> Option<Event> {
    let event: Event = serde_json::from_str(json).ok()?;
    (event.client_id == head.client_id && event.space == head.space).then_some(event)
}
