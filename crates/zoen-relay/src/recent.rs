//! Who wrote the last few messages of each Space this node delivered (SCALE.md §3).
//!
//! A receipt in a large Space goes to the authors it acknowledges, not to every member.
//! Finding those authors from the log is a range read per receipt; but every message a
//! receipt can acknowledge was just delivered through this node to the person sending the
//! receipt, so the node remembers it. A miss (evicted, restarted) falls back to the log.

use std::{
    collections::{HashMap, VecDeque},
    hash::{BuildHasher, RandomState},
    sync::Mutex,
};

/// Messages remembered per Space; also how far back a receipt looks for authors.
pub const WINDOW: usize = 16;
const SHARDS: usize = 16;
/// Spaces per shard before the shard starts over (a cache, so forgetting is safe).
const SPACES_PER_SHARD: usize = 16_384;

type Shard = Mutex<HashMap<String, VecDeque<(u64, String)>>>;

pub struct RecentAuthors {
    hasher: RandomState,
    shards: Vec<Shard>,
}

impl Default for RecentAuthors {
    fn default() -> Self {
        Self {
            hasher: RandomState::new(),
            shards: (0..SHARDS).map(|_| Mutex::default()).collect(),
        }
    }
}

impl RecentAuthors {
    fn shard(&self, space: &str) -> &Shard {
        &self.shards[self.hasher.hash_one(space) as usize % SHARDS]
    }

    pub fn record(&self, space: &str, seq: u64, author: &str) {
        let mut map = self.shard(space).lock().unwrap_or_else(|e| e.into_inner());
        if map.len() >= SPACES_PER_SHARD && !map.contains_key(space) {
            map.clear();
        }
        let ring = map.entry(space.to_string()).or_default();
        if ring.iter().any(|(s, _)| *s == seq) {
            return;
        }
        // Keep seq order; arrivals are nearly always in order, so this is the back.
        let at = ring
            .iter()
            .rposition(|(s, _)| *s < seq)
            .map_or(0, |i| i + 1);
        ring.insert(at, (seq, author.to_string()));
        if ring.len() > WINDOW {
            ring.pop_front();
        }
    }

    /// Distinct authors of the remembered messages in `(seq - WINDOW, seq]`, or `None` when
    /// message `seq` itself isn't remembered (ask the log).
    pub fn authors_through(&self, space: &str, seq: u64) -> Option<Vec<String>> {
        let map = self.shard(space).lock().unwrap_or_else(|e| e.into_inner());
        let ring = map.get(space)?;
        if !ring.iter().any(|(s, _)| *s == seq) {
            return None;
        }
        let floor = seq.saturating_sub(WINDOW as u64);
        let mut out: Vec<String> = Vec::new();
        for (_, a) in ring.iter().filter(|(s, _)| *s > floor && *s <= seq) {
            if !out.contains(a) {
                out.push(a.clone());
            }
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembers_a_window_in_order_and_misses_honestly() {
        let r = RecentAuthors::default();
        assert_eq!(r.authors_through("sp", 1), None);
        for seq in 1..=40u64 {
            r.record("sp", seq, if seq % 2 == 0 { "ana" } else { "bia" });
        }
        r.record("sp", 40, "dup"); // a second delivery of the same event changes nothing
        assert_eq!(r.authors_through("sp", 40).unwrap(), vec!["bia", "ana"]);
        assert_eq!(r.authors_through("sp", 3), None, "evicted: ask the log");
        // Out of order arrival still lands in order.
        r.record("sp2", 5, "caio");
        r.record("sp2", 3, "dora");
        assert_eq!(r.authors_through("sp2", 5).unwrap(), vec!["dora", "caio"]);
        assert_eq!(r.authors_through("sp2", 3).unwrap(), vec!["dora"]);
    }
}
