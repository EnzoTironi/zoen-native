//! Relay minima belong to pending originals across this network task's sockets.

use super::{Duration, HashMap, HashSet};
use tokio::time::Instant;

#[derive(Default)]
pub(super) struct PublicationRetries(HashMap<String, Instant>);

impl PublicationRetries {
    pub(super) fn defer(&mut self, client_id: String, reason: &str) {
        let wait = protocol::retry_hint(reason).unwrap_or(Duration::from_millis(500));
        let until = Instant::now() + wait;
        self.0
            .entry(client_id)
            .and_modify(|previous| *previous = (*previous).max(until))
            .or_insert(until);
    }

    pub(super) fn settled(&mut self, client_id: &str) {
        self.0.remove(client_id);
    }

    pub(super) fn deadline(&self) -> Option<Instant> {
        self.0.values().copied().min()
    }

    pub(super) fn release_due(&mut self) {
        let now = Instant::now();
        self.0.retain(|_, until| *until > now);
    }

    pub(super) fn waiting_spaces(&mut self, pending: &HashMap<String, String>) -> HashSet<String> {
        self.release_due();
        self.0.retain(|id, _| pending.contains_key(id));
        // The wire hint does not identify a limiter's scope. Hold the refused
        // original's existing FIFO lane, without inventing a global embargo.
        self.0
            .keys()
            .filter_map(|id| pending.get(id).cloned())
            .collect()
    }
}
