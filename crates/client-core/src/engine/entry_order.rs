//! Relay-ordered entries precede the still-pending suffix, just as on cold replay.
//! Move only the newly confirmed rows; keep IDs, payloads and pending order intact.
use super::SpaceState;

impl SpaceState {
    pub(crate) fn settle_confirmed_tail(&mut self, sequence: u64) {
        let tail = self
            .entries
            .iter()
            .rposition(|entry| entry.seq != sequence)
            .map_or(0, |index| index + 1);
        if tail == self.entries.len() {
            return;
        }
        let boundary = self.entries[..tail].partition_point(|entry| !entry.hash.is_empty());
        let moved = self.entries.len() - tail;
        self.entries[boundary..].rotate_right(moved);
    }

    pub(crate) fn confirm_pending_entries(&mut self, client_id: &str, sequence: u64, hash: &str) {
        let mut positions = Vec::new();
        for (index, entry) in self.entries.iter_mut().enumerate().filter(|(_, entry)| {
            entry.hash.is_empty()
                && (entry.client_id == client_id
                    || entry.client_id.strip_suffix(":result") == Some(client_id))
        }) {
            entry.seq = sequence;
            entry.hash = hash.to_owned();
            positions.push(index);
        }
        for index in positions {
            let boundary = self.entries[..index].partition_point(|entry| !entry.hash.is_empty());
            self.entries[boundary..=index].rotate_right(1);
        }
    }
}
